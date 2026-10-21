//! Clipboard History view (plan §7): one list over either Windows clipboard
//! history or smolcast's session history (§7.0), with search, preview,
//! bounded thumbnails, delete, and clear.
//!
//! Backend-neutral: the clipboard thread and paste service are driven by the
//! model and host. This view only raises requests (`take_*`).

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use egui::{Color32, Key, Modifiers, RichText, TextureHandle, TextureOptions, Vec2};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::core::config::{ClipboardSource, PaletteSize};
use crate::core::dib::ImagePayload;
use crate::core::history::{History, InsertOutcome, Limits, Payload, encode_record};
use crate::win::clipboard::{ClipboardCommand, ClipboardError, WindowsContent, WindowsSnapshot};
use crate::win::persistence::{ClipboardStoreCommand, LoadedEntry, unix_ms_now};

use super::{list, theme};

/// Thumbnail textures kept alive (§7.5: about 8 MiB RGBA worst case).
const THUMBNAIL_CACHE_CAP: usize = 32;
/// Thumbnail edge bound in pixels (§7.5).
const THUMBNAIL_EDGE: u32 = 256;
/// Rows shown at once in a medium palette; the snippet view matches.
const MEDIUM_ROWS: usize = 6;
/// Characters of text considered by search.
const SEARCH_CHARS: usize = 512;
const ROW_CHARS: usize = 60;
const LIST_WIDTH: f32 = 300.0;

/// Which history the view is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Active {
    Windows,
    Smolcast,
}

/// Identity of a row across both sources.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum RowKey {
    Local(u64),
    Windows(String),
}

/// What the paste service should put on the clipboard (§8.2 step 1).
pub enum PasteRequest {
    Local(Payload),
    Windows {
        item_id: String,
    },
    /// Text that is pasted but not recorded in history (an emoji sequence).
    Text(String),
}

/// A large stored image the host should downscale off the UI thread.
pub struct ThumbnailRequest {
    pub id: u64,
    pub image: ImagePayload,
}

/// A borrowed, source-neutral row.
struct Row<'a> {
    key: RowKey,
    captured_at_ms: i64,
    content: RowContent<'a>,
}

enum RowContent<'a> {
    Text(&'a str),
    Image {
        width: u32,
        height: u32,
        pixels: &'a ImagePayload,
    },
    Other,
}

/// Rows of the active source, newest first.
fn collect_rows<'a>(
    active: Option<Active>,
    history: &'a History,
    windows: &'a Option<WindowsSnapshot>,
) -> Vec<Row<'a>> {
    match active {
        Some(Active::Smolcast) => history
            .iter()
            .map(|entry| Row {
                key: RowKey::Local(entry.id),
                captured_at_ms: entry.captured_at_ms,
                content: match &entry.payload {
                    Payload::Text(text) => RowContent::Text(text),
                    Payload::Image(image) => RowContent::Image {
                        width: image.width,
                        height: image.height,
                        pixels: image,
                    },
                },
            })
            .collect(),
        Some(Active::Windows) => windows
            .iter()
            .flat_map(|snapshot| &snapshot.items)
            .map(|item| Row {
                key: RowKey::Windows(item.id.clone()),
                captured_at_ms: item.captured_at_ms,
                content: match &item.content {
                    WindowsContent::Text(text) => RowContent::Text(text),
                    WindowsContent::Image {
                        width,
                        height,
                        thumbnail,
                    } => RowContent::Image {
                        width: *width,
                        height: *height,
                        pixels: thumbnail,
                    },
                    WindowsContent::Other => RowContent::Other,
                },
            })
            .collect(),
        None => Vec::new(),
    }
}

#[derive(Default)]
struct Thumbnails {
    textures: HashMap<RowKey, TextureHandle>,
    /// Least recently used at the front.
    recency: VecDeque<RowKey>,
    /// Stored images being downscaled by the host, and since when.
    pending: HashMap<u64, Instant>,
    requests: Vec<ThumbnailRequest>,
}

impl Thumbnails {
    /// The texture for `key`. Images that need downscaling are requested from
    /// the host; until they arrive this is the request time. Small ones load
    /// inline.
    fn get(
        &mut self,
        ctx: &egui::Context,
        key: &RowKey,
        image: &ImagePayload,
    ) -> Result<TextureHandle, Instant> {
        if let Some(texture) = self.textures.get(key).cloned() {
            if let Some(pos) = self.recency.iter().position(|x| x == key) {
                self.recency.remove(pos);
            }
            self.recency.push_back(key.clone());
            return Ok(texture);
        }
        match key {
            RowKey::Local(id) if needs_downscale(image) => {
                Err(*self.pending.entry(*id).or_insert_with(|| {
                    self.requests.push(ThumbnailRequest {
                        id: *id,
                        image: image.clone(),
                    });
                    Instant::now()
                }))
            }
            _ => Ok(self.insert(ctx, key.clone(), thumbnail(image))),
        }
    }

    fn insert(
        &mut self,
        ctx: &egui::Context,
        key: RowKey,
        image: egui::ColorImage,
    ) -> TextureHandle {
        let texture = ctx.load_texture("clip-thumb", image, TextureOptions::LINEAR);
        self.textures.insert(key.clone(), texture.clone());
        self.recency.push_back(key);
        while self.recency.len() > THUMBNAIL_CACHE_CAP {
            if let Some(old) = self.recency.pop_front() {
                self.textures.remove(&old);
            }
        }
        texture
    }

    /// Drop the texture of a deleted or evicted entry.
    fn forget(&mut self, key: &RowKey) {
        if let RowKey::Local(id) = key {
            self.pending.remove(id);
        }
        if self.textures.remove(key).is_some() {
            self.recency.retain(|x| x != key);
        }
    }

    fn clear(&mut self) {
        self.textures.clear();
        self.recency.clear();
        self.pending.clear();
    }
}

fn thumbnail_size(image: &ImagePayload) -> (u32, u32) {
    let scale = (THUMBNAIL_EDGE as f32 / image.width.max(image.height) as f32).min(1.0);
    (
        ((image.width as f32 * scale).round() as u32).max(1),
        ((image.height as f32 * scale).round() as u32).max(1),
    )
}

fn needs_downscale(image: &ImagePayload) -> bool {
    thumbnail_size(image) != (image.width, image.height)
}

/// Downscale to fit within `THUMBNAIL_EDGE` (never upscale). A full-size image
/// takes milliseconds, so the host runs it off the UI thread for stored images.
pub fn thumbnail(image: &ImagePayload) -> egui::ColorImage {
    let (w, h) = thumbnail_size(image);
    if (w, h) == (image.width, image.height) {
        return egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &image.rgba);
    }
    let source = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
        image.width,
        image.height,
        &image.rgba[..],
    )
    .expect("payload length is width * height * 4");
    let pixels = image::imageops::thumbnail(&source, w, h).into_raw();
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels)
}

pub struct ClipboardView {
    /// smolcast's session history (§7.1–§7.5).
    history: History,
    setting: ClipboardSource,
    /// `None` until the first Windows snapshot decides an `Auto` view.
    active: Option<Active>,
    windows: Option<WindowsSnapshot>,
    windows_error: Option<String>,
    windows_generation: u64,
    /// When the newest Windows refresh was requested, while it is in flight.
    windows_refreshing: Option<Instant>,
    thumbnails: Thumbnails,
    /// Rows shown at once, from the palette size.
    rows: usize,
    /// Mouse-wheel distance not yet worth a whole row.
    wheel: f32,
    /// Row keys matching `filtered_for`, newest first.
    filtered: Vec<RowKey>,
    filtered_for: Option<String>,
    selected: usize,
    first_row: usize,
    paste_request: Option<PasteRequest>,
    commands: Vec<ClipboardCommand>,
    open_settings: bool,
    /// Persist smolcast's history across restarts (§7.2).
    persist: bool,
    /// False until stored entries are loaded; captures wait in `pending`.
    loaded: bool,
    pending: Vec<(Payload, i64)>,
    /// Stored file token of each live history entry.
    tokens: HashMap<u64, String>,
    token_counter: u32,
    store: Vec<ClipboardStoreCommand>,
}

impl ClipboardView {
    /// `awaiting_load` is true when the host will deliver stored entries.
    pub fn new(
        limits: Limits,
        setting: ClipboardSource,
        persist: bool,
        awaiting_load: bool,
    ) -> Self {
        Self {
            history: History::new(limits),
            setting,
            active: None,
            windows: None,
            windows_error: None,
            windows_generation: 0,
            windows_refreshing: None,
            thumbnails: Thumbnails::default(),
            rows: MEDIUM_ROWS,
            wheel: 0.0,
            filtered: Vec::new(),
            filtered_for: None,
            selected: 0,
            first_row: 0,
            paste_request: None,
            commands: Vec::new(),
            open_settings: false,
            persist,
            loaded: !awaiting_load,
            pending: Vec::new(),
            tokens: HashMap::new(),
            token_counter: 0,
            store: Vec::new(),
        }
    }

    // ---- inputs from the model ------------------------------------------------

    /// The view is being shown: pick or confirm the source (§7.0).
    pub fn open(&mut self) {
        self.selected = 0;
        self.first_row = 0;
        self.filtered_for = None;
        match self.setting {
            ClipboardSource::Smolcast => self.active = Some(Active::Smolcast),
            ClipboardSource::Windows | ClipboardSource::Auto => self.refresh_windows(),
        }
    }

    pub fn set_palette_size(&mut self, size: PaletteSize) {
        self.rows = theme::visible_rows(MEDIUM_ROWS, size);
    }

    pub fn set_setting(&mut self, setting: ClipboardSource) {
        if setting != self.setting {
            self.setting = setting;
            self.active = None;
            self.filtered_for = None;
        }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        for id in self.history.set_limits(limits) {
            self.forget_entry(id);
        }
        self.filtered_for = None;
    }

    /// Turning persistence off deletes the stored files; turning it on stores
    /// every current entry.
    pub fn set_persist(&mut self, persist: bool) {
        if persist == self.persist {
            return;
        }
        self.persist = persist;
        if persist {
            let ids: Vec<u64> = self.history.iter().map(|e| e.id).collect();
            for id in ids {
                self.save_entry(id);
            }
        } else {
            self.tokens.clear();
            self.store.push(ClipboardStoreCommand::Clear);
        }
    }

    /// A payload captured into smolcast's history, or one pasted from it and
    /// promoted to the front.
    pub fn record(&mut self, payload: Payload) {
        let now = unix_ms_now();
        if !self.loaded {
            self.pending.push((payload, now));
            return;
        }
        self.insert(payload, now, None);
    }

    /// Stored entries arrived (oldest first); captures that raced the load are
    /// applied after them so ordering stays newest-first.
    pub fn on_loaded(&mut self, entries: Vec<LoadedEntry>) {
        for entry in entries {
            self.insert(entry.payload, entry.captured_at_ms, Some(entry.token));
        }
        self.loaded = true;
        for (payload, at) in std::mem::take(&mut self.pending) {
            self.insert(payload, at, None);
        }
    }

    /// The store is unavailable; stop waiting for a load.
    pub fn store_unavailable(&mut self) {
        self.persist = false;
        self.on_loaded(Vec::new());
    }

    /// Insert and mirror the outcome to the store. `stored` is the token of an
    /// entry read from disk, which needs no rewrite.
    fn insert(&mut self, payload: Payload, at: i64, stored: Option<String>) {
        match self.history.insert(payload, at) {
            InsertOutcome::Inserted { id, evicted } => {
                match stored {
                    Some(token) => {
                        self.tokens.insert(id, token);
                    }
                    None => self.save_entry(id),
                }
                for old in evicted {
                    self.forget_entry(old);
                }
            }
            InsertOutcome::Promoted { id } => {
                // Its time changed: rewrite it. A duplicate file from disk is dropped.
                if let Some(token) = stored {
                    self.store.push(ClipboardStoreCommand::Delete { token });
                }
                self.save_entry(id);
            }
            InsertOutcome::Rejected { .. } => {
                if let Some(token) = stored {
                    self.store.push(ClipboardStoreCommand::Delete { token });
                }
            }
        }
        if self.active == Some(Active::Smolcast) {
            self.filtered_for = None;
        }
    }

    fn save_entry(&mut self, id: u64) {
        if !self.persist {
            return;
        }
        let Some(entry) = self.history.get(id) else {
            return;
        };
        let record = encode_record(&entry.payload, entry.captured_at_ms);
        let token = match self.tokens.get(&id) {
            Some(token) => token.clone(),
            None => {
                self.token_counter = self.token_counter.wrapping_add(1);
                let token = format!("{:013}-{:05}", unix_ms_now().max(0), self.token_counter);
                self.tokens.insert(id, token.clone());
                token
            }
        };
        self.store
            .push(ClipboardStoreCommand::Save { token, record });
    }

    /// Drop an evicted or deleted entry's texture and stored file.
    fn forget_entry(&mut self, id: u64) {
        self.thumbnails.forget(&RowKey::Local(id));
        if let Some(token) = self.tokens.remove(&id) {
            self.store.push(ClipboardStoreCommand::Delete { token });
        }
    }

    pub fn take_store_commands(&mut self) -> Vec<ClipboardStoreCommand> {
        std::mem::take(&mut self.store)
    }

    pub fn take_thumbnail_requests(&mut self) -> Vec<ThumbnailRequest> {
        std::mem::take(&mut self.thumbnails.requests)
    }

    /// A downscaled stored image. Dropped when the entry is gone or was
    /// cleared meanwhile.
    pub fn on_thumbnail(&mut self, ctx: &egui::Context, id: u64, image: egui::ColorImage) {
        if self.thumbnails.pending.remove(&id).is_some() && self.history.get(id).is_some() {
            let _ = self.thumbnails.insert(ctx, RowKey::Local(id), image);
        }
    }

    pub fn on_windows_history(
        &mut self,
        generation: u64,
        result: Result<WindowsSnapshot, ClipboardError>,
    ) {
        if generation != self.windows_generation {
            return; // superseded by a newer refresh
        }
        self.windows_refreshing = None;
        match result {
            Ok(snapshot) => {
                self.windows_error = None;
                self.active = Some(match self.setting {
                    ClipboardSource::Auto if !snapshot.enabled => Active::Smolcast,
                    ClipboardSource::Smolcast => Active::Smolcast,
                    _ => Active::Windows,
                });
                // Thumbnails of items that left Windows history are dropped.
                let stale: Vec<RowKey> = self
                    .thumbnails
                    .textures
                    .keys()
                    .filter(|key| match key {
                        RowKey::Windows(id) => !snapshot.items.iter().any(|i| &i.id == id),
                        RowKey::Local(_) => false,
                    })
                    .cloned()
                    .collect();
                for key in stale {
                    self.thumbnails.forget(&key);
                }
                self.windows = Some(snapshot);
            }
            Err(e) => {
                self.windows_error = Some(e.to_string());
                if self.setting == ClipboardSource::Auto {
                    self.active = Some(Active::Smolcast);
                }
            }
        }
        self.filtered_for = None;
    }

    /// The clipboard changed while smolcast was not capturing it: a visible
    /// Windows-history view refreshes.
    pub fn on_clipboard_changed(&mut self, visible: bool) {
        if visible && self.active == Some(Active::Windows) {
            self.refresh_windows();
        }
    }

    fn refresh_windows(&mut self) {
        self.windows_refreshing.get_or_insert_with(Instant::now);
        self.windows_generation += 1;
        self.commands.push(ClipboardCommand::WindowsRefresh {
            generation: self.windows_generation,
        });
    }

    // ---- outputs to the model ---------------------------------------------------

    pub fn take_paste_request(&mut self) -> Option<PasteRequest> {
        self.paste_request.take()
    }

    pub fn take_commands(&mut self) -> Vec<ClipboardCommand> {
        std::mem::take(&mut self.commands)
    }

    /// The user asked to open Windows clipboard settings.
    pub fn take_open_settings(&mut self) -> bool {
        std::mem::take(&mut self.open_settings)
    }

    // ---- keys and actions ---------------------------------------------------------

    /// Consume navigation and action keys before the search field sees them.
    /// `paste_enabled` is false while a paste is in flight.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui, paste_enabled: bool) {
        let (down, up, enter, delete) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::SHIFT, Key::Delete),
            )
        });
        list::step_selection(&mut self.selected, self.filtered.len(), down, up);
        if enter && paste_enabled {
            self.commit();
        }
        if delete {
            self.delete_selected();
        }
    }

    fn commit(&mut self) {
        let Some(key) = self.filtered.get(self.selected).cloned() else {
            return;
        };
        self.paste_request = match key {
            RowKey::Local(id) => self
                .history
                .get(id)
                .map(|entry| PasteRequest::Local(entry.payload.clone())),
            RowKey::Windows(item_id) => Some(PasteRequest::Windows { item_id }),
        };
    }

    fn delete_selected(&mut self) {
        let Some(key) = self.filtered.get(self.selected).cloned() else {
            return;
        };
        match &key {
            RowKey::Local(id) => {
                self.history.remove(*id);
                self.forget_entry(*id);
            }
            RowKey::Windows(item_id) => {
                self.commands.push(ClipboardCommand::WindowsDelete {
                    item_id: item_id.clone(),
                });
                if let Some(snapshot) = &mut self.windows {
                    snapshot.items.retain(|i| &i.id != item_id);
                }
                self.refresh_windows();
            }
        }
        self.thumbnails.forget(&key);
        self.filtered_for = None;
    }

    fn clear_all(&mut self) {
        match self.active {
            Some(Active::Smolcast) => {
                self.history.clear();
                self.tokens.clear();
                if self.persist {
                    self.store.push(ClipboardStoreCommand::Clear);
                }
            }
            Some(Active::Windows) => {
                // Windows keeps pinned items (§7.0); the refresh shows them.
                self.commands.push(ClipboardCommand::WindowsClear);
                self.refresh_windows();
            }
            None => {}
        }
        self.thumbnails.clear();
        self.filtered_for = None;
    }

    // ---- UI -------------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut egui::Ui, query: &str, paste_enabled: bool) {
        let query = query.trim();
        if self.filtered_for.as_deref() != Some(query) {
            let query_changed = self
                .filtered_for
                .as_deref()
                .is_some_and(|previous| previous != query);
            self.refilter(query);
            if query_changed {
                self.selected = 0;
                self.first_row = 0;
            }
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        let visible = self.rows;
        let list_height = visible as f32 * theme::ROW_STRIDE;
        let step = list::wheel_rows(ui, theme::ROW_STRIDE, &mut self.wheel);
        let count = self.filtered.len();
        list::scroll_window(
            &mut self.first_row,
            &mut self.selected,
            count,
            visible,
            step,
        );
        list::keep_visible(self.selected, &mut self.first_row, visible);

        theme::inset(ui, |ui| self.source_line(ui));
        if self.setting == ClipboardSource::Windows
            && self.windows.as_ref().is_some_and(|s| !s.enabled)
        {
            theme::inset(ui, |ui| {
                ui.label("Windows clipboard history is turned off.");
                if ui.button("Open clipboard settings").clicked() {
                    self.open_settings = true;
                }
            });
            return;
        }

        let now = unix_ms_now();
        let rows = collect_rows(self.active, &self.history, &self.windows);
        let by_key: HashMap<&RowKey, &Row> = rows.iter().map(|row| (&row.key, row)).collect();
        let thumbnails = &mut self.thumbnails;
        let (filtered, selected, first_row) = (&self.filtered, self.selected, self.first_row);
        let mut clicked = None;
        ui.horizontal_top(|ui| {
            // Rows stack top-down inside the horizontal list | preview split,
            // and the list has a fixed height so the action row stays visible.
            // The cap also stops the separator taking all remaining height,
            // which grew the window every frame.
            ui.set_max_height(list_height);
            let size = Vec2::new(LIST_WIDTH, list_height);
            ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_min_size(size);
                ui.set_width(LIST_WIDTH);
                if filtered.is_empty() {
                    let text = match self.active {
                        None => None,
                        Some(Active::Smolcast) if !self.loaded => None,
                        Some(_) if rows.is_empty() => Some("Nothing copied yet."),
                        Some(_) => Some("No matches."),
                    };
                    theme::inset(ui, |ui| match text {
                        Some(text) => {
                            ui.label(RichText::new(text).weak());
                        }
                        None => theme::busy(ui, "Loading clipboard history…"),
                    });
                }
                let end = (first_row + visible).min(filtered.len());
                for (index, key) in filtered.iter().enumerate().take(end).skip(first_row) {
                    if let Some(row) = by_key.get(key)
                        && row_ui(ui, row, index == selected, now).clicked()
                    {
                        clicked = Some(index);
                    }
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_max_height(list_height);
                if let Some(row) = filtered.get(selected).and_then(|k| by_key.get(k)) {
                    theme::inset(ui, |ui| preview_ui(ui, row, thumbnails, list_height));
                }
            });
        });
        if let Some(index) = clicked {
            self.selected = index;
            if paste_enabled {
                self.commit();
            }
        }

        ui.add_space(theme::SECTION_GAP);
        theme::inset(ui, |ui| self.actions_ui(ui));
    }

    /// Delete and Clear all, under the list.
    fn actions_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Delete").on_hover_text("Shift+Delete").clicked() {
                self.delete_selected();
            }
            let clear = ui.button("Clear all");
            let clear = if self.active == Some(Active::Windows) {
                clear.on_hover_text("Clears Windows clipboard history; pinned items stay")
            } else {
                clear
            };
            if clear.clicked() {
                self.clear_all();
            }
        });
    }

    fn refilter(&mut self, query: &str) {
        let matcher = SkimMatcherV2::default();
        self.filtered = collect_rows(self.active, &self.history, &self.windows)
            .into_iter()
            .filter(|row| {
                query.is_empty()
                    || match &row.content {
                        RowContent::Text(text) => {
                            let head: String = text.chars().take(SEARCH_CHARS).collect();
                            matcher.fuzzy_match(&head, query).is_some()
                        }
                        RowContent::Image { .. } => matcher.fuzzy_match("image", query).is_some(),
                        RowContent::Other => matcher.fuzzy_match("other", query).is_some(),
                    }
            })
            .map(|row| row.key)
            .collect();
        self.filtered_for = Some(query.to_owned());
    }

    fn source_line(&self, ui: &mut egui::Ui) {
        if self.active == Some(Active::Windows)
            && self
                .windows_refreshing
                .is_some_and(|since| theme::busy_due(ui, since))
        {
            theme::busy_small(ui, "Windows clipboard history · updating…");
            return;
        }
        let text = match (self.active, self.setting) {
            (Some(Active::Windows), _) => "Windows clipboard history".to_owned(),
            (Some(Active::Smolcast), ClipboardSource::Auto) => format!(
                "smolcast history · {} (Windows clipboard history is off)",
                human_bytes(self.history.total_bytes())
            ),
            (Some(Active::Smolcast), _) => {
                format!(
                    "smolcast history · {}",
                    human_bytes(self.history.total_bytes())
                )
            }
            (None, _) => String::new(),
        };
        if !text.is_empty() {
            ui.label(RichText::new(text).weak().small());
        }
        if let Some(error) = &self.windows_error {
            ui.label(
                RichText::new(format!("Windows clipboard history unavailable: {error}"))
                    .color(Color32::from_rgb(230, 180, 60))
                    .small(),
            );
        }
    }
}

fn preview_ui(ui: &mut egui::Ui, row: &Row<'_>, thumbnails: &mut Thumbnails, list_height: f32) {
    match &row.content {
        RowContent::Text(text) => {
            let shown: String = text.chars().take(2000).collect();
            ui.add(egui::Label::new(RichText::new(shown).monospace().small()).wrap());
            ui.label(
                RichText::new(format!("{} characters", text.chars().count()))
                    .weak()
                    .small(),
            );
        }
        RowContent::Image {
            width,
            height,
            pixels,
        } => {
            match thumbnails.get(ui.ctx(), &row.key, pixels) {
                Ok(texture) => {
                    let size = texture.size_vec2();
                    let fit = (ui.available_width() / size.x)
                        .min((list_height - 24.0) / size.y)
                        .min(1.0);
                    ui.add(egui::Image::new(&texture).fit_to_exact_size(size * fit));
                }
                Err(since) => {
                    if theme::busy_due(ui, since) {
                        theme::busy(ui, "Loading preview…");
                    }
                }
            }
            ui.label(RichText::new(format!("{width} × {height}")).weak().small());
        }
        RowContent::Other => {
            ui.label(RichText::new("No preview for this item.").weak());
        }
    }
}

fn row_ui(ui: &mut egui::Ui, row: &Row<'_>, selected: bool, now: i64) -> egui::Response {
    let (glyph, text) = match &row.content {
        RowContent::Text(text) => {
            // Runs every frame for every visible row over items of up to
            // 8 MiB, so it stops at the second non-blank line.
            let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
            let first = lines.next().unwrap_or("");
            let mut chars = first.chars();
            let mut shown: String = chars.by_ref().take(ROW_CHARS).collect();
            if chars.next().is_some() || lines.next().is_some() {
                shown.push('…');
            }
            ("📄", shown)
        }
        RowContent::Image { width, height, .. } => ("🖼", format!("Image {width} × {height}")),
        RowContent::Other => ("📦", "Other content".to_owned()),
    };
    let age = relative_age(now, row.captured_at_ms);
    list::row(
        ui,
        selected,
        &text,
        |ui| {
            ui.label(glyph);
        },
        |ui| {
            ui.label(RichText::new(age).weak().small());
        },
    )
}

fn relative_age(now_ms: i64, then_ms: i64) -> String {
    let seconds = now_ms.saturating_sub(then_ms).max(0) / 1000;
    match seconds {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

fn human_bytes(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KiB", b as f64 / 1024.0),
        b => format!("{:.1} MiB", b as f64 / (1024.0 * 1024.0)),
    }
}
