//! Emoji picker view (plan §9): keyword search over the pinned table, a
//! keyboard-navigable grid, and paste through the shared paste service.
//!
//! Backend-neutral: it raises a paste request with the exact sequence text;
//! the model owns the paste service. Glyphs are color bitmaps rasterized by
//! Windows (DirectWrite) on request for visible cells only, and cached in a
//! bounded texture set; the pasted codepoints are the contract.

use std::collections::{HashMap, HashSet, VecDeque};

use egui::{Key, Modifiers, RichText, TextureHandle, TextureOptions, Vec2};

use crate::core::emoji::{self, Emoji};
use crate::core::emoji_data::GROUPS;
use crate::win::icon::IconRgba;

const COLUMNS: usize = 12;
const VISIBLE_ROWS: usize = 6;
const CELL: f32 = 38.0;
const GLYPH_SIZE: f32 = 24.0;
/// Glyph textures kept alive: a few grid pages at any display scale.
const GLYPH_CACHE_CAP: usize = 256;

/// A glyph the host should rasterize.
pub struct GlyphRequest {
    pub index: usize,
    pub text: &'static str,
    pub edge: u32,
}

/// Rasterized glyphs by emoji table index, least recently used first.
#[derive(Default)]
struct Glyphs {
    textures: HashMap<usize, TextureHandle>,
    recency: VecDeque<usize>,
    pending: HashSet<usize>,
    failed: HashSet<usize>,
    /// Pixel edge the cache was rendered at; a scale change starts over.
    edge: u32,
}

pub struct EmojiView {
    /// Indices into `emoji::all()` for `results_for`.
    results: Vec<usize>,
    results_for: Option<String>,
    selected: usize,
    first_row: usize,
    paste_request: Option<&'static str>,
    glyphs: Glyphs,
    requests: Vec<GlyphRequest>,
}

impl Default for EmojiView {
    fn default() -> Self {
        Self::new()
    }
}

impl EmojiView {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            results_for: None,
            selected: 0,
            first_row: 0,
            paste_request: None,
            glyphs: Glyphs::default(),
            requests: Vec::new(),
        }
    }

    pub fn take_glyph_requests(&mut self) -> Vec<GlyphRequest> {
        std::mem::take(&mut self.requests)
    }

    pub fn on_glyph(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        result: Result<IconRgba, String>,
    ) {
        self.glyphs.pending.remove(&index);
        match result {
            Ok(glyph) => {
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [glyph.width as usize, glyph.height as usize],
                    &glyph.rgba_premultiplied,
                );
                let texture = ctx.load_texture("emoji", image, TextureOptions::LINEAR);
                if self.glyphs.textures.insert(index, texture).is_none() {
                    self.glyphs.recency.push_back(index);
                }
                while self.glyphs.recency.len() > GLYPH_CACHE_CAP {
                    if let Some(old) = self.glyphs.recency.pop_front() {
                        self.glyphs.textures.remove(&old);
                    }
                }
            }
            Err(_) => {
                self.glyphs.failed.insert(index);
            }
        }
    }

    /// The texture for `index`, requesting it when missing.
    fn glyph(&mut self, index: usize) -> Option<TextureHandle> {
        if let Some(texture) = self.glyphs.textures.get(&index).cloned() {
            if let Some(pos) = self.glyphs.recency.iter().position(|x| *x == index) {
                self.glyphs.recency.remove(pos);
                self.glyphs.recency.push_back(index);
            }
            return Some(texture);
        }
        if !self.glyphs.pending.contains(&index) && !self.glyphs.failed.contains(&index) {
            self.glyphs.pending.insert(index);
            self.requests.push(GlyphRequest {
                index,
                text: emoji::all()[index].text,
                edge: self.glyphs.edge,
            });
        }
        None
    }

    pub fn open(&mut self) {
        self.selected = 0;
        self.first_row = 0;
        self.results_for = None;
    }

    /// The exact sequence to paste, once per commit.
    pub fn take_paste_request(&mut self) -> Option<&'static str> {
        self.paste_request.take()
    }

    /// Consume grid navigation keys before the search field sees them. Left and
    /// Right move within the grid, so the search cursor cannot be moved with
    /// them while the picker is open.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui, paste_enabled: bool) {
        let (left, right, up, down, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        let count = self.results.len();
        if count > 0 {
            if right {
                self.selected = (self.selected + 1).min(count - 1);
            }
            if left {
                self.selected = self.selected.saturating_sub(1);
            }
            if down {
                self.selected = (self.selected + COLUMNS).min(count - 1);
            }
            if up {
                self.selected = self.selected.saturating_sub(COLUMNS);
            }
        }
        if enter && paste_enabled {
            self.commit(self.selected);
        }
    }

    fn commit(&mut self, index: usize) {
        if let Some(&entry) = self.results.get(index) {
            self.paste_request = Some(emoji::all()[entry].text);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, query: &str, paste_enabled: bool) {
        let query = query.trim();
        if self.results_for.as_deref() != Some(query) {
            self.results = emoji::search(query);
            self.results_for = Some(query.to_owned());
            self.selected = 0;
            self.first_row = 0;
        }
        if self.results.is_empty() {
            ui.label(RichText::new("No matching emoji.").weak());
            return;
        }
        self.selected = self.selected.min(self.results.len() - 1);

        let row = self.selected / COLUMNS;
        if row < self.first_row {
            self.first_row = row;
        } else if row >= self.first_row + VISIBLE_ROWS {
            self.first_row = row + 1 - VISIBLE_ROWS;
        }

        let edge = ((GLYPH_SIZE * ui.ctx().pixels_per_point()).round() as u32).clamp(16, 128);
        if edge != self.glyphs.edge {
            self.glyphs = Glyphs {
                edge,
                ..Glyphs::default()
            };
        }

        let table = emoji::all();
        let start = self.first_row * COLUMNS;
        let end = (start + VISIBLE_ROWS * COLUMNS).min(self.results.len());
        let mut clicked = None;
        egui::Grid::new("emoji-grid")
            .spacing(Vec2::splat(2.0))
            .show(ui, |ui| {
                for offset in 0..end - start {
                    let index = start + offset;
                    let entry = self.results[index];
                    let glyph = self.glyph(entry);
                    if cell_ui(ui, &table[entry], glyph.as_ref(), index == self.selected).clicked()
                    {
                        clicked = Some(index);
                    }
                    if (offset + 1) % COLUMNS == 0 {
                        ui.end_row();
                    }
                }
            });
        if let Some(index) = clicked {
            self.selected = index;
            if paste_enabled {
                self.commit(index);
            }
        }

        ui.add_space(6.0);
        let selected_index = self.results[self.selected];
        let selected = &table[selected_index];
        let selected_glyph = self.glyph(selected_index);
        ui.horizontal(|ui| {
            let size = Vec2::splat(GLYPH_SIZE * 1.5);
            match &selected_glyph {
                Some(texture) => {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                }
                None => {
                    ui.allocate_exact_size(size, egui::Sense::hover());
                }
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(selected.name).strong());
                ui.label(
                    RichText::new(format!(
                        "{} · {} of {}",
                        GROUPS[selected.group as usize],
                        self.selected + 1,
                        self.results.len()
                    ))
                    .weak()
                    .small(),
                );
            });
        });
    }
}

fn cell_ui(
    ui: &mut egui::Ui,
    emoji: &Emoji,
    glyph: Option<&TextureHandle>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CELL), egui::Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    // Until its bitmap arrives (a few milliseconds) the cell stays empty;
    // drawing egui's own emoji font would flash tofu boxes.
    if let Some(texture) = glyph {
        let rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(GLYPH_SIZE));
        egui::Image::new(texture).paint_at(ui, rect);
    }
    response.on_hover_text(emoji.name)
}
