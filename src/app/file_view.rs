//! Search Files view (plan §6.6): file and folder names from the Windows
//! Search index, opened with Enter or shown in Explorer with Ctrl+Enter.
//!
//! Backend-neutral: it raises a search request per settled query and the
//! host's worker answers with `AppEvent::FilesFound`. Typing waits
//! `DEBOUNCE` before asking, so a fast typist sends one query, not one per
//! key.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers, RichText, Vec2};

use crate::core::config::PaletteSize;
use crate::core::file_search::FileHit;

use super::{list, theme};

/// Result rows shown at once in a medium palette.
const MEDIUM_ROWS: usize = 7;
/// How long the query must stay unchanged before it is sent.
const DEBOUNCE: Duration = Duration::from_millis(120);
/// Row glyph edge in points, like the launcher's icons.
const GLYPH_POINTS: f32 = 22.0;
/// Longest folder path shown in a row's right column, in characters.
const FOLDER_CHARS: usize = 48;

/// What the user chose to do with a file.
pub enum FileAction {
    Open(String),
    Reveal(String),
}

pub struct FileView {
    hits: Vec<FileHit>,
    /// The query `hits` answer; `None` before the first answer.
    shown_for: Option<String>,
    /// The query being typed, and when it last changed.
    typed: Option<(String, Instant)>,
    /// The newest query sent; answers for older ones are dropped.
    generation: u64,
    sent: Option<String>,
    error: Option<String>,
    selected: usize,
    first_row: usize,
    rows: usize,
    wheel: f32,
    request: Option<(u64, String)>,
    action: Option<FileAction>,
    /// Why file search cannot run at all (its thread did not start).
    unavailable: Option<String>,
}

impl Default for FileView {
    fn default() -> Self {
        Self::new()
    }
}

impl FileView {
    pub fn new() -> Self {
        Self {
            hits: Vec::new(),
            shown_for: None,
            typed: None,
            generation: 0,
            sent: None,
            error: None,
            selected: 0,
            first_row: 0,
            rows: MEDIUM_ROWS,
            wheel: 0.0,
            request: None,
            action: None,
            unavailable: None,
        }
    }

    pub fn set_palette_size(&mut self, size: PaletteSize) {
        self.rows = theme::visible_rows(MEDIUM_ROWS, size);
    }

    /// Opening the view starts empty; the model may then set the query.
    pub fn open(&mut self) {
        self.forget_sent();
        self.hits.clear();
        self.shown_for = None;
        self.typed = None;
        self.error = None;
        self.selected = 0;
        self.first_row = 0;
    }

    /// Drop the query in flight: a new generation makes its late answer stale.
    fn forget_sent(&mut self) {
        self.sent = None;
        self.generation += 1;
    }

    /// A query for the host's search worker.
    pub fn take_request(&mut self) -> Option<(u64, String)> {
        self.request.take()
    }

    pub fn take_action(&mut self) -> Option<FileAction> {
        self.action.take()
    }

    /// File search cannot run this session; the view says so instead.
    pub fn set_unavailable(&mut self, message: String) {
        self.unavailable = Some(message);
    }

    /// The worker's answer. Only the newest query's answer is shown.
    pub fn on_results(&mut self, generation: u64, result: Result<Vec<FileHit>, String>) {
        if generation != self.generation {
            return;
        }
        self.shown_for = self.sent.clone();
        match result {
            Ok(hits) => {
                self.hits = hits;
                self.error = None;
            }
            Err(e) => {
                self.hits.clear();
                self.error = Some(e);
            }
        }
        self.selected = 0;
        self.first_row = 0;
    }

    fn searching(&self) -> bool {
        self.sent.is_some() && self.sent != self.shown_for
    }

    /// Enter opens, Ctrl+Enter shows the item in Explorer.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui) {
        let (down, up, enter, reveal) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Enter),
            )
        });
        list::step_selection(&mut self.selected, self.hits.len(), down, up);
        if let Some(hit) = self.hits.get(self.selected) {
            if reveal {
                self.action = Some(FileAction::Reveal(hit.path.clone()));
            } else if enter {
                self.action = Some(FileAction::Open(hit.path.clone()));
            }
        }
    }

    /// Send `query` once it has settled.
    fn schedule(&mut self, ui: &egui::Ui, query: &str) {
        let query = query.trim();
        let now = Instant::now();
        match &self.typed {
            Some((typed, _)) if typed == query => {}
            _ => self.typed = Some((query.to_owned(), now)),
        }
        let Some((typed, since)) = &self.typed else {
            return;
        };
        if self.sent.as_deref() == Some(typed.as_str()) {
            return;
        }
        if typed.is_empty() {
            // Nothing to ask for: clear at once.
            self.forget_sent();
            self.shown_for = None;
            self.hits.clear();
            self.error = None;
            return;
        }
        let waited = now.duration_since(*since);
        if waited < DEBOUNCE {
            ui.ctx().request_repaint_after(DEBOUNCE - waited);
            return;
        }
        self.generation += 1;
        self.sent = Some(typed.clone());
        self.request = Some((self.generation, typed.clone()));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, query: &str) {
        if let Some(message) = &self.unavailable {
            theme::inset(ui, |ui| {
                ui.label(RichText::new(message).color(theme::WARNING));
            });
            return;
        }
        self.schedule(ui, query);
        theme::section_header(ui, "Files", self.searching().then_some("Searching…"));
        if query.trim().is_empty() {
            theme::inset(ui, |ui| {
                ui.label(RichText::new("Type part of a file or folder name.").weak());
                ui.label(
                    RichText::new(
                        "Results come from the Windows Search index. Enter opens, \
                         Ctrl+Enter shows the item in Explorer.",
                    )
                    .weak()
                    .small(),
                );
            });
            return;
        }
        if let Some(error) = &self.error {
            theme::inset(ui, |ui| {
                ui.label(RichText::new(error).color(theme::WARNING));
            });
            return;
        }
        if self.hits.is_empty() {
            if self.shown_for.is_some() && !self.searching() {
                theme::inset(ui, |ui| {
                    ui.label(RichText::new("No matches.").weak());
                    ui.label(
                        RichText::new(
                            "Only indexed locations are searched: see Windows Settings > \
                             Privacy & security > Searching Windows.",
                        )
                        .weak()
                        .small(),
                    );
                });
            }
            return;
        }

        let step = list::wheel_rows(ui, theme::ROW_STRIDE, &mut self.wheel);
        let count = self.hits.len();
        self.selected =
            list::scroll_window(&mut self.first_row, self.selected, count, self.rows, step);
        list::keep_visible(self.selected, &mut self.first_row, self.rows);
        let end = (self.first_row + self.rows).min(count);
        let mut clicked = None;
        for row in self.first_row..end {
            let hit = &self.hits[row];
            let leading = |ui: &mut egui::Ui| {
                let glyph = if hit.folder { "📁" } else { "📄" };
                ui.add_sized(Vec2::splat(GLYPH_POINTS), egui::Label::new(glyph));
            };
            let folder = shorten(hit.parent(), FOLDER_CHARS);
            let trailing = |ui: &mut egui::Ui| {
                ui.label(RichText::new(folder).weak().small())
                    .on_hover_text(&hit.path);
            };
            if list::row(ui, row == self.selected, &hit.name, leading, trailing).clicked() {
                clicked = Some(row);
            }
        }
        if let Some(row) = clicked {
            self.selected = row;
            self.action = Some(FileAction::Open(self.hits[row].path.clone()));
        }
    }
}

/// `path` cut to its last `max` characters behind a leading "…", so the
/// nearest folders stay readable.
fn shorten(path: &str, max: usize) -> String {
    let count = path.chars().count();
    if count <= max {
        return path.to_owned();
    }
    let tail: String = path.chars().skip(count - (max - 1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_folders_keep_their_end() {
        assert_eq!(shorten("C:\\a", 10), "C:\\a");
        assert_eq!(shorten("C:\\Users\\me\\Documents", 10), "…Documents");
        assert_eq!(shorten("C:\\Users\\me\\Documents", 10).chars().count(), 10);
    }
}
