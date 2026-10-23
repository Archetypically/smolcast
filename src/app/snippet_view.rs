//! Snippet view (plan §10.6): search, preview, and paste snippets, and create,
//! edit, or delete them in place.
//!
//! Backend-neutral: it raises a paste request with the exact text to write
//! (line breaks already CRLF) and a `SnippetEdit` for the host to write to
//! config.toml. The file stays the source of truth: a save completes only
//! when the normal reload accepts the edited file, like the settings form.

use std::collections::BTreeMap;

use egui::{Key, Modifiers, RichText};

use crate::core::app_index::Command;
use crate::core::config::{PaletteSize, SnippetConfig};
use crate::core::config_edit::SnippetEdit;
use crate::core::list_cursor::{self, ListCursor};
use crate::core::snippet::{self, Snippet};

use super::{
    form::{self, Status},
    list, theme,
};

/// Rows shown at once in a medium palette; the clipboard view matches.
const MEDIUM_ROWS: usize = 6;

enum Mode {
    List,
    Edit(Editor),
    ConfirmDelete { id: String, name: String },
}

/// The form's fields as typed. `id` is `None` for a new snippet.
#[derive(Clone, Default, PartialEq)]
struct Draft {
    name: String,
    keyword: String,
    text: String,
}

struct Editor {
    id: Option<String>,
    draft: Draft,
    original: Draft,
    /// Focus the name field on the next frame.
    focus: bool,
    /// Escape was pressed once with unsaved changes; a second press discards.
    confirm_discard: bool,
}

impl Editor {
    fn dirty(&self) -> bool {
        self.draft != self.original
    }

    /// Name and keyword are trimmed; an empty keyword means none.
    fn snippet(&self) -> SnippetConfig {
        let keyword = self.draft.keyword.trim();
        SnippetConfig {
            name: self.draft.name.trim().to_owned(),
            keyword: (!keyword.is_empty()).then(|| keyword.to_owned()),
            text: self.draft.text.clone(),
        }
    }

    fn problem(&self, snippets: &[Snippet]) -> Option<String> {
        snippet::draft_problem(&self.snippet(), self.id.as_deref(), snippets)
    }
}

pub struct SnippetView {
    snippets: Vec<Snippet>,
    /// Indices into `snippets` for `results_for`.
    results: Vec<usize>,
    results_for: Option<String>,
    cursor: ListCursor,
    mode: Mode,
    status: Status,
    /// Select this id once the list next shows it (after a save).
    select_id: Option<String>,
    paste_request: Option<String>,
    save_request: Option<SnippetEdit>,
    /// The editor closed; `true` also asks the model to clear the search.
    left_editor: Option<bool>,
    open_file: bool,
    /// Rows shown at once, from the palette size.
    rows: usize,
}

impl SnippetView {
    pub fn new(snippets: &BTreeMap<String, SnippetConfig>) -> Self {
        Self {
            snippets: snippet::from_config(snippets),
            results: Vec::new(),
            results_for: None,
            cursor: ListCursor::default(),
            mode: Mode::List,
            status: Status::Idle,
            select_id: None,
            paste_request: None,
            save_request: None,
            left_editor: None,
            open_file: false,
            rows: MEDIUM_ROWS,
        }
    }

    /// A config was accepted (a save finished, or the file was reloaded).
    /// Returns whether this completed a save from the view.
    pub fn config_accepted(&mut self, snippets: &BTreeMap<String, SnippetConfig>) -> bool {
        self.snippets = snippet::from_config(snippets);
        // The old indices may point at other snippets now; keys act on
        // nothing until the list is ranked again.
        self.results.clear();
        self.results_for = None;
        if !self.saving() {
            return false;
        }
        self.status = match std::mem::replace(&mut self.mode, Mode::List) {
            Mode::Edit(_) => Status::Done("Saved."),
            _ => Status::Done("Deleted."),
        };
        self.left_editor = Some(true);
        true
    }

    /// A save failed before or during the reload that applies it. The form
    /// stays open with the draft.
    pub fn save_failed(&mut self, message: String) {
        if self.saving() {
            self.status = Status::Failed(message);
        }
    }

    pub fn set_palette_size(&mut self, size: PaletteSize) {
        self.rows = theme::visible_rows(MEDIUM_ROWS, size);
    }

    fn list_height(&self) -> f32 {
        self.rows as f32 * theme::ROW_STRIDE
    }

    /// Opening the view resumes an unsaved draft; otherwise it starts at the
    /// top of the list.
    pub fn open(&mut self) {
        if self.busy() {
            return;
        }
        self.mode = Mode::List;
        self.status = Status::Idle;
        self.cursor.reset();
        self.results_for = None;
    }

    /// Open an empty editor (Ctrl+N, or the root "Create Snippet" command).
    /// An unsaved draft is resumed instead of discarded.
    pub fn start_new(&mut self) {
        if self.busy() {
            return;
        }
        self.begin_edit(None);
    }

    /// The breadcrumb for the current mode.
    pub fn title(&self) -> &'static str {
        match &self.mode {
            Mode::Edit(Editor { id: None, .. }) => "New Snippet",
            Mode::Edit(_) => "Edit Snippet",
            Mode::List | Mode::ConfirmDelete { .. } => Command::Snippets.title(),
        }
    }

    pub fn is_confirming_delete(&self) -> bool {
        matches!(self.mode, Mode::ConfirmDelete { .. })
    }

    /// True while the editor or a delete confirmation owns the view: the model
    /// hides the search field and leaves Backspace to the form.
    pub fn is_editing(&self) -> bool {
        !matches!(self.mode, Mode::List)
    }

    /// Escape inside the view. Returns false when the model should handle it
    /// (leave the view).
    pub fn escape(&mut self) -> bool {
        if self.saving() {
            return self.is_editing();
        }
        match &mut self.mode {
            Mode::List => return false,
            Mode::Edit(editor) if editor.dirty() && !editor.confirm_discard => {
                editor.confirm_discard = true;
                return true;
            }
            Mode::Edit(_) | Mode::ConfirmDelete { .. } => {}
        }
        self.mode = Mode::List;
        self.status = Status::Idle;
        self.left_editor = Some(false);
        true
    }

    pub fn take_paste_request(&mut self) -> Option<String> {
        self.paste_request.take()
    }

    pub fn take_save_request(&mut self) -> Option<SnippetEdit> {
        self.save_request.take()
    }

    /// `Some(clear_search)` once after the editor closes.
    pub fn take_left_editor(&mut self) -> Option<bool> {
        self.left_editor.take()
    }

    pub fn take_open_file(&mut self) -> bool {
        std::mem::take(&mut self.open_file)
    }

    fn saving(&self) -> bool {
        matches!(self.status, Status::Saving)
    }

    /// A save is in flight or a draft has unsaved changes: keep the editor.
    fn busy(&self) -> bool {
        self.saving() || matches!(&self.mode, Mode::Edit(editor) if editor.dirty())
    }

    fn selected_snippet(&self) -> Option<&Snippet> {
        self.results
            .get(self.cursor.selected)
            .and_then(|&i| self.snippets.get(i))
    }

    fn begin_edit(&mut self, id: Option<String>) {
        let original = id
            .as_deref()
            .and_then(|id| self.snippets.iter().find(|s| s.id == id))
            .map(|s| Draft {
                name: s.name.clone(),
                keyword: s.keyword.clone().unwrap_or_default(),
                text: s.text.clone(),
            })
            .unwrap_or_default();
        self.mode = Mode::Edit(Editor {
            id,
            draft: original.clone(),
            original,
            focus: true,
            confirm_discard: false,
        });
        self.status = Status::Idle;
    }

    fn save(&mut self) {
        let Mode::Edit(editor) = &self.mode else {
            return;
        };
        if !editor.dirty() || editor.problem(&self.snippets).is_some() || self.saving() {
            return;
        }
        let snippet = editor.snippet();
        let (id, create) = match &editor.id {
            Some(id) => (id.clone(), false),
            None => (snippet::new_id(&snippet.name, &self.snippets), true),
        };
        self.select_id = Some(id.clone());
        self.save_request = Some(SnippetEdit::Put {
            id,
            snippet,
            create,
        });
        self.status = Status::Saving;
    }

    fn confirm_delete(&mut self) {
        let Mode::ConfirmDelete { id, .. } = &self.mode else {
            return;
        };
        if !self.saving() {
            self.save_request = Some(SnippetEdit::Delete { id: id.clone() });
            self.status = Status::Saving;
        }
    }

    fn ask_delete(&mut self) {
        if let Some(s) = self.selected_snippet() {
            self.mode = Mode::ConfirmDelete {
                id: s.id.clone(),
                name: s.name.clone(),
            };
            self.status = Status::Idle;
        }
    }

    fn edit_selected(&mut self) {
        if let Some(id) = self.selected_snippet().map(|s| s.id.clone()) {
            self.begin_edit(Some(id));
        }
    }

    fn commit(&mut self) {
        if let Some(s) = self.selected_snippet() {
            self.paste_request = Some(snippet::paste_text(&s.text));
        }
    }

    // ---- keys ---------------------------------------------------------------------

    /// Consume this mode's keys before the search field or form sees them.
    /// `paste_enabled` is false while a paste is in flight.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui, paste_enabled: bool) {
        match self.mode {
            Mode::List => self.list_keys(ui, paste_enabled),
            Mode::Edit(_) => {
                let save = ui.input_mut(|i| {
                    i.consume_key(Modifiers::COMMAND, Key::S)
                        | i.consume_key(Modifiers::COMMAND, Key::Enter)
                });
                if save {
                    self.save();
                }
            }
            Mode::ConfirmDelete { .. } => {
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    self.confirm_delete();
                }
            }
        }
    }

    fn list_keys(&mut self, ui: &mut egui::Ui, paste_enabled: bool) {
        let (down, up, enter, new, edit, delete) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::N),
                i.consume_key(Modifiers::COMMAND, Key::E),
                i.consume_key(Modifiers::SHIFT, Key::Delete),
            )
        });
        self.cursor.step(self.results.len(), down, up);
        if enter && paste_enabled {
            self.commit();
        }
        if new {
            self.start_new();
        } else if edit {
            self.edit_selected();
        } else if delete {
            self.ask_delete();
        }
    }

    // ---- UI -----------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut egui::Ui, query: &str, paste_enabled: bool) {
        if matches!(self.mode, Mode::Edit(_)) {
            theme::inset(ui, |ui| self.editor_ui(ui));
            return;
        }
        self.refresh_results(query);

        if self.snippets.is_empty() {
            theme::inset(ui, |ui| {
                ui.label("No snippets yet.");
                ui.label(
                    RichText::new(
                        "Save text you paste often, then find it here by name or keyword.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(theme::SECTION_GAP);
                ui.horizontal(|ui| {
                    if ui.button("New snippet").on_hover_text("Ctrl+N").clicked() {
                        self.start_new();
                    }
                    if form::config_link(ui, "Snippets are stored in config.toml") {
                        self.open_file = true;
                    }
                });
                form::status_ui(ui, &self.status, None);
            });
            return;
        }

        let visible = list::scroll(ui, &mut self.cursor, self.results.len(), self.rows);

        let mut clicked = None;
        list::split(
            ui,
            self.list_height(),
            |ui| {
                if self.results.is_empty() {
                    theme::inset(ui, |ui| ui.label(RichText::new("No matches.").weak()));
                }
                for row in visible.clone() {
                    let snippet = &self.snippets[self.results[row]];
                    let state =
                        list::RowState::of(row, self.cursor.selected, &visible, self.results.len());
                    if row_ui(ui, snippet, state).clicked() {
                        clicked = Some(row);
                    }
                }
            },
            |ui| {
                if let Some(snippet) = self.selected_snippet() {
                    theme::inset(ui, |ui| preview_ui(ui, snippet));
                }
            },
        );
        if let Some(row) = clicked {
            self.cursor.selected = row;
            if paste_enabled && !self.is_editing() {
                self.commit();
            }
        }

        ui.add_space(theme::SECTION_GAP);
        theme::inset(ui, |ui| self.actions_ui(ui));
    }

    /// The row under the list: delete confirmation or New / Edit / Delete,
    /// then the save status.
    fn actions_ui(&mut self, ui: &mut egui::Ui) {
        if let Mode::ConfirmDelete { name, .. } = &self.mode {
            let name = name.clone();
            ui.horizontal(|ui| {
                ui.label(format!("Delete \"{name}\"?"));
                let saving = self.saving();
                if ui
                    .add_enabled(!saving, egui::Button::new("Delete"))
                    .on_hover_text("Enter")
                    .clicked()
                {
                    self.confirm_delete();
                }
                if ui
                    .add_enabled(!saving, egui::Button::new("Cancel"))
                    .on_hover_text("Esc")
                    .clicked()
                {
                    self.escape();
                }
            });
        } else {
            let has_selection = self.selected_snippet().is_some();
            ui.horizontal(|ui| {
                if ui.button("New").on_hover_text("Ctrl+N").clicked() {
                    self.start_new();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Edit"))
                    .on_hover_text("Ctrl+E")
                    .clicked()
                {
                    self.edit_selected();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Delete"))
                    .on_hover_text("Shift+Delete")
                    .clicked()
                {
                    self.ask_delete();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if form::config_link(ui, "Snippets are stored in config.toml") {
                        self.open_file = true;
                    }
                });
            });
        }
        form::status_ui(ui, &self.status, None);
    }

    fn refresh_results(&mut self, query: &str) {
        let query = query.trim();
        if self.results_for.as_deref() != Some(query) {
            if self.results_for.is_some() {
                self.cursor.reset();
            }
            self.results = snippet::search(query, &self.snippets);
            self.results_for = Some(query.to_owned());
        }
        if let Some(id) = self.select_id.take()
            && let Some(row) = self.results.iter().position(|&i| self.snippets[i].id == id)
        {
            self.cursor.selected = row;
        }
        self.cursor.clamp_selected(self.results.len());
        list_cursor::keep_visible(self.cursor.selected, &mut self.cursor.first_row, self.rows);
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let saving = self.saving();
        let list_height = self.list_height();
        let Mode::Edit(editor) = &mut self.mode else {
            return;
        };
        let snippets = &self.snippets;
        egui::Grid::new("snippet-editor")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label("Name");
                let name = ui.add_enabled(
                    !saving,
                    egui::TextEdit::singleline(&mut editor.draft.name).desired_width(320.0),
                );
                if std::mem::take(&mut editor.focus) {
                    name.request_focus();
                }
                ui.end_row();

                ui.label("Keyword");
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !saving,
                        egui::TextEdit::singleline(&mut editor.draft.keyword)
                            .desired_width(120.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.label(RichText::new("optional, e.g. ;sig").weak().small());
                });
                ui.end_row();
            });
        ui.add_space(theme::SECTION_GAP);
        ui.label("Text");
        egui::ScrollArea::vertical()
            .max_height(list_height)
            .show(ui, |ui| {
                ui.add_enabled(
                    !saving,
                    egui::TextEdit::multiline(&mut editor.draft.text)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });
        if editor.dirty() {
            // Any new edit withdraws a pending discard confirmation.
            editor.confirm_discard &= ui.input(|i| !i.events.iter().any(form::is_text_event));
        }

        let dirty = editor.dirty();
        let confirm_discard = editor.confirm_discard;
        let problem = dirty.then(|| editor.problem(snippets)).flatten();
        ui.add_space(theme::SECTION_GAP);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    dirty && problem.is_none() && !saving,
                    egui::Button::new("Save"),
                )
                .on_hover_text("Ctrl+S")
                .clicked()
            {
                self.save();
            }
            if ui
                .add_enabled(!saving, egui::Button::new("Cancel"))
                .on_hover_text("Esc")
                .clicked()
            {
                // A click is deliberate: no second confirmation.
                if let Mode::Edit(editor) = &mut self.mode {
                    editor.confirm_discard = true;
                }
                self.escape();
            }
        });
        let hint = confirm_discard.then_some("Unsaved changes. Press Esc again to discard.");
        form::status_ui(
            ui,
            &self.status,
            problem.as_deref().map(form::capitalize).as_deref().or(hint),
        );
    }
}

fn preview_ui(ui: &mut egui::Ui, snippet: &Snippet) {
    list::text_preview(ui, &snippet.text, snippet.chars);
}

fn row_ui(ui: &mut egui::Ui, snippet: &Snippet, state: list::RowState) -> egui::Response {
    list::row(
        ui,
        state,
        &snippet.name,
        |ui| {
            ui.label("📝");
        },
        |ui| {
            if let Some(keyword) = &snippet.keyword {
                ui.label(RichText::new(keyword).weak().small().monospace());
            }
        },
    )
}
