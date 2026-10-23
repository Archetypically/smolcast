//! Script view (plan §10.3): search and run tracked scripts, with their saved
//! arguments or extra ones typed for one run, and track, edit, or untrack
//! them in place.
//!
//! Backend-neutral, like the snippet view: it raises a run request with the
//! script id and typed arguments, and a `ScriptEdit` for the host to write to
//! config.toml. The file stays the source of truth: a save completes only
//! when the normal reload accepts the edited file.

use std::collections::BTreeMap;

use egui::{Key, Modifiers, RichText};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::core::app_index::Command;
use crate::core::config::{self, PaletteSize, ScriptConfig};
use crate::core::config_edit::ScriptEdit;
use crate::core::list_cursor::{self, ListCursor};
use crate::core::script_file;
use crate::win::hotkey;

use super::{
    form::{self, Status},
    list, theme,
};

/// Rows shown at once in a medium palette; the snippet view matches.
const MEDIUM_ROWS: usize = 6;

enum Mode {
    List,
    Edit(Box<Editor>),
    /// Extra arguments for one run of `id`, appended after its saved ones.
    RunWith {
        id: String,
        name: String,
        args: String,
        focus: bool,
    },
    ConfirmUntrack {
        id: String,
        name: String,
    },
}

/// The form's fields as typed. `args` is one line, split like a command line
/// ([`script_file::split_args`]). `interpreter` and `cwd` are shown only for
/// a hand-written command (no `path`).
#[derive(Clone, Default, PartialEq)]
struct Draft {
    path: String,
    name: String,
    args: String,
    interpreter: String,
    cwd: String,
    hotkey: String,
}

impl Draft {
    /// The path as it runs: trimmed, without the quotes a copied path carries.
    fn path(&self) -> &str {
        self.path.trim().trim_matches('"')
    }
}

struct Editor {
    /// `None` while tracking a new file.
    id: Option<String>,
    /// The saved entry; keys the form does not show (a tracked file's
    /// `interpreter` or `cwd` override) are kept from it.
    saved: ScriptConfig,
    draft: Draft,
    original: Draft,
    /// Focus the name field on the next frame.
    focus: bool,
    /// Escape was pressed once with unsaved changes; a second press discards.
    confirm_discard: bool,
}

impl Editor {
    fn new(id: Option<String>, saved: ScriptConfig) -> Self {
        let original = Draft {
            path: saved.path.clone().unwrap_or_default(),
            name: saved.name.clone(),
            args: script_file::join_args(&saved.args),
            interpreter: saved.interpreter.clone(),
            cwd: saved.cwd.clone(),
            hotkey: saved.hotkey.clone().unwrap_or_default(),
        };
        Self {
            id,
            saved,
            draft: original.clone(),
            original,
            focus: true,
            confirm_discard: false,
        }
    }

    /// A hand-written command rather than a tracked file.
    fn is_command(&self) -> bool {
        self.id.is_some() && self.saved.path.is_none()
    }

    fn dirty(&self) -> bool {
        self.draft != self.original
    }

    /// A file was chosen: it names a new script unless a name was typed.
    fn set_path(&mut self, path: &str) {
        self.draft.path = path.to_owned();
        if self.draft.name.trim().is_empty() {
            self.draft.name = script_file::default_name(path);
        }
    }

    /// Text fields are trimmed; an unchanged arguments line keeps the saved
    /// vector exactly; an empty hotkey means none.
    fn script(&self) -> ScriptConfig {
        let args = if self.draft.args == self.original.args {
            self.saved.args.clone()
        } else {
            script_file::split_args(&self.draft.args)
        };
        let hotkey = self.draft.hotkey.trim();
        let mut script = ScriptConfig {
            name: self.draft.name.trim().to_owned(),
            args,
            hotkey: (!hotkey.is_empty()).then(|| hotkey.to_owned()),
            ..self.saved.clone()
        };
        if self.is_command() {
            script.interpreter = self.draft.interpreter.trim().to_owned();
            script.cwd = self.draft.cwd.trim().to_owned();
        } else {
            script.path = Some(self.draft.path().to_owned());
        }
        script
    }

    /// A field rule, a hotkey that does not parse, or a hotkey another
    /// script already uses.
    fn problem(&self, scripts: &BTreeMap<String, ScriptConfig>, summon: &str) -> Option<String> {
        if !self.is_command() && self.draft.path.trim().is_empty() {
            return Some("choose a script file".into());
        }
        let script = self.script();
        if let Some(problem) = script.problem() {
            return Some(problem);
        }
        let key = script.hotkey.as_deref()?;
        if let Err(e) = hotkey::check(key) {
            return Some(format!("hotkey: {e}"));
        }
        if hotkey::same_chord(key, summon) {
            return Some("hotkey is the summon hotkey".into());
        }
        scripts
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != self.id.as_deref())
            .find(|(_, s)| {
                s.hotkey
                    .as_deref()
                    .is_some_and(|k| hotkey::same_chord(k, key))
            })
            .map(|(_, s)| format!("hotkey is already used by \"{}\"", s.name))
    }
}

pub struct ScriptView {
    scripts: BTreeMap<String, ScriptConfig>,
    /// The summon hotkey, which no script hotkey may repeat.
    summon: String,
    /// Ids for `results_for`, best first.
    results: Vec<String>,
    results_for: Option<String>,
    cursor: ListCursor,
    mode: Mode,
    status: Status,
    /// Select this id once the list next shows it (after a save).
    select_id: Option<String>,
    /// A script id and the extra arguments typed for this run.
    run_request: Option<(String, Vec<String>)>,
    save_request: Option<ScriptEdit>,
    /// The editor asks the model to record a hotkey chord.
    record_request: bool,
    /// The editor asks the model for the Open dialog.
    pick_request: bool,
    /// The editor closed; `true` also asks the model to clear the search.
    left_editor: Option<bool>,
    open_file: bool,
    /// Rows shown at once, from the palette size.
    rows: usize,
}

impl ScriptView {
    pub fn new(scripts: &BTreeMap<String, ScriptConfig>, summon: &str) -> Self {
        Self {
            scripts: scripts.clone(),
            summon: summon.to_owned(),
            results: Vec::new(),
            results_for: None,
            cursor: ListCursor::default(),
            mode: Mode::List,
            status: Status::Idle,
            select_id: None,
            run_request: None,
            save_request: None,
            record_request: false,
            pick_request: false,
            left_editor: None,
            open_file: false,
            rows: MEDIUM_ROWS,
        }
    }

    /// A config was accepted (a save finished, or the file was reloaded).
    /// Returns whether this completed a save from the view.
    pub fn config_accepted(
        &mut self,
        scripts: &BTreeMap<String, ScriptConfig>,
        summon: &str,
    ) -> bool {
        self.scripts = scripts.clone();
        summon.clone_into(&mut self.summon);
        self.results_for = None;
        if !self.saving() {
            // An open editor saves on top of the file as it is now, so keys
            // the form does not show keep an outside edit.
            if let Mode::Edit(editor) = &mut self.mode
                && let Some(saved) = editor.id.as_ref().and_then(|id| scripts.get(id))
            {
                editor.saved = saved.clone();
            }
            return false;
        }
        self.status = match std::mem::replace(&mut self.mode, Mode::List) {
            Mode::Edit(_) => Status::Done("Saved."),
            _ => Status::Done("Untracked."),
        };
        self.left_editor = Some(true);
        true
    }

    /// A save failed before or during the reload that applies it. The form
    /// stays open with the draft.
    pub fn save_failed(&mut self, message: String) {
        if self.saving() {
            self.status = Status::Failed(message);
            // Nothing new to select; a retry asks again.
            self.select_id = None;
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

    /// Open a form for a new tracked file and ask for the Open dialog
    /// (Ctrl+N, or the root "Track Script File" command). An unsaved draft
    /// is resumed instead.
    pub fn start_track(&mut self) {
        if self.busy() {
            return;
        }
        self.mode = Mode::Edit(Box::new(Editor::new(None, ScriptConfig::default())));
        self.status = Status::Idle;
        self.pick_request = true;
    }

    /// The Open dialog chose `path`. Ignored unless a tracking form is still
    /// open.
    pub fn track_file(&mut self, path: &str) {
        let saving = self.saving();
        if let Mode::Edit(editor) = &mut self.mode
            && !editor.is_command()
            && !saving
        {
            editor.set_path(path);
        }
    }

    pub fn take_pick_request(&mut self) -> bool {
        std::mem::take(&mut self.pick_request)
    }

    /// A chord recorded for the hotkey field.
    pub fn set_hotkey(&mut self, chord: String) {
        if let Mode::Edit(editor) = &mut self.mode {
            editor.draft.hotkey = chord;
        }
    }

    /// The breadcrumb for the current mode.
    pub fn title(&self) -> &'static str {
        match &self.mode {
            Mode::Edit(editor) if editor.id.is_none() => Command::TrackScript.title(),
            Mode::Edit(_) => "Edit Script",
            Mode::RunWith { .. } => "Run with Arguments",
            Mode::List | Mode::ConfirmUntrack { .. } => Command::Scripts.title(),
        }
    }

    /// The footer's main action for the current mode.
    pub fn primary_action(&self) -> (&'static str, &'static [&'static str]) {
        match &self.mode {
            Mode::List | Mode::RunWith { .. } => ("Run", &["Enter"]),
            Mode::Edit(_) => ("Save", &["Ctrl", "S"]),
            Mode::ConfirmUntrack { .. } => ("Untrack", &["Enter"]),
        }
    }

    /// True while a form, the arguments prompt or a confirmation owns the
    /// view: the model hides the search field and leaves Backspace to it.
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
            Mode::Edit(_) | Mode::RunWith { .. } | Mode::ConfirmUntrack { .. } => {}
        }
        self.mode = Mode::List;
        self.status = Status::Idle;
        self.left_editor = Some(false);
        true
    }

    pub fn take_run_request(&mut self) -> Option<(String, Vec<String>)> {
        self.run_request.take()
    }

    pub fn take_save_request(&mut self) -> Option<ScriptEdit> {
        self.save_request.take()
    }

    pub fn take_record_request(&mut self) -> bool {
        std::mem::take(&mut self.record_request)
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

    fn selected(&self) -> Option<(&String, &ScriptConfig)> {
        let id = self.results.get(self.cursor.selected)?;
        self.scripts.get_key_value(id)
    }

    fn save(&mut self) {
        let Mode::Edit(editor) = &self.mode else {
            return;
        };
        if !editor.dirty() || editor.problem(&self.scripts, &self.summon).is_some() || self.saving()
        {
            return;
        }
        let script = editor.script();
        let (id, create) = match &editor.id {
            Some(id) => (id.clone(), false),
            None => (
                config::new_id(
                    &script.name,
                    "script",
                    self.scripts.keys().map(String::as_str),
                ),
                true,
            ),
        };
        self.select_id = Some(id.clone());
        self.save_request = Some(ScriptEdit::Put { id, script, create });
        self.status = Status::Saving;
    }

    fn confirm_untrack(&mut self) {
        let Mode::ConfirmUntrack { id, .. } = &self.mode else {
            return;
        };
        if !self.saving() {
            self.save_request = Some(ScriptEdit::Delete { id: id.clone() });
            self.status = Status::Saving;
        }
    }

    fn ask_untrack(&mut self) {
        if let Some((id, s)) = self.selected() {
            self.mode = Mode::ConfirmUntrack {
                id: id.clone(),
                name: s.name.clone(),
            };
            self.status = Status::Idle;
        }
    }

    fn edit_selected(&mut self) {
        if let Some((id, script)) = self.selected() {
            let editor = Editor::new(Some(id.clone()), script.clone());
            self.mode = Mode::Edit(Box::new(editor));
            self.status = Status::Idle;
        }
    }

    fn ask_args(&mut self) {
        if let Some((id, s)) = self.selected() {
            self.mode = Mode::RunWith {
                id: id.clone(),
                name: s.name.clone(),
                args: String::new(),
                focus: true,
            };
            self.status = Status::Idle;
        }
    }

    fn run_selected(&mut self) {
        if let Some(id) = self.selected().map(|(id, _)| id.clone()) {
            self.run_request = Some((id, Vec::new()));
        }
    }

    /// Run with the typed arguments; the prompt closes, and the palette
    /// hides once the spawn succeeds.
    fn run_with_args(&mut self) {
        if let Mode::RunWith { id, args, .. } = &self.mode {
            self.run_request = Some((id.clone(), script_file::split_args(args)));
            self.mode = Mode::List;
            self.left_editor = Some(false);
        }
    }

    // ---- keys ---------------------------------------------------------------------

    /// Consume this mode's keys before the search field or form sees them.
    /// `run_enabled` is false while another launch is starting.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui, run_enabled: bool) {
        match self.mode {
            Mode::List => self.list_keys(ui, run_enabled),
            Mode::Edit(_) => {
                let save = ui.input_mut(|i| {
                    i.consume_key(Modifiers::COMMAND, Key::S)
                        | i.consume_key(Modifiers::COMMAND, Key::Enter)
                });
                if save {
                    self.save();
                }
            }
            Mode::RunWith { .. } => {
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) && run_enabled {
                    self.run_with_args();
                }
            }
            Mode::ConfirmUntrack { .. } => {
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    self.confirm_untrack();
                }
            }
        }
    }

    fn list_keys(&mut self, ui: &mut egui::Ui, run_enabled: bool) {
        let (down, up, with_args, enter, track, edit, untrack) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                // Before plain Enter, which would also match.
                i.consume_key(Modifiers::COMMAND, Key::Enter)
                    | i.consume_key(Modifiers::NONE, Key::Tab),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::N),
                i.consume_key(Modifiers::COMMAND, Key::E),
                i.consume_key(Modifiers::SHIFT, Key::Delete),
            )
        });
        self.cursor.step(self.results.len(), down, up);
        if with_args {
            self.ask_args();
        } else if enter && run_enabled {
            self.run_selected();
        } else if track {
            self.start_track();
        } else if edit {
            self.edit_selected();
        } else if untrack {
            self.ask_untrack();
        }
    }

    // ---- UI -----------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut egui::Ui, query: &str, run_enabled: bool, recording: bool) {
        match self.mode {
            Mode::Edit(_) => {
                theme::inset(ui, |ui| self.editor_ui(ui, recording));
                return;
            }
            Mode::RunWith { .. } => {
                theme::inset(ui, |ui| self.args_ui(ui, run_enabled));
                return;
            }
            Mode::List | Mode::ConfirmUntrack { .. } => {}
        }
        self.refresh_results(query);

        if self.scripts.is_empty() {
            theme::inset(ui, |ui| {
                ui.label("No scripts yet.");
                ui.label(
                    RichText::new(
                        "Track a script file you run often, then start it from here, \
                         the root search, or its own hotkey.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(theme::SECTION_GAP);
                ui.horizontal(|ui| {
                    if ui
                        .button("Track a script file…")
                        .on_hover_text("Ctrl+N")
                        .clicked()
                    {
                        self.start_track();
                    }
                    if form::config_link(ui, "Scripts are stored in config.toml") {
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
                    let Some(script) = self.scripts.get(&self.results[row]) else {
                        continue;
                    };
                    let state =
                        list::RowState::of(row, self.cursor.selected, &visible, self.results.len());
                    if row_ui(ui, script, state).clicked() {
                        clicked = Some(row);
                    }
                }
            },
            |ui| {
                if let Some((_, script)) = self.selected() {
                    theme::inset(ui, |ui| preview_ui(ui, script));
                }
            },
        );
        if let Some(row) = clicked {
            self.cursor.selected = row;
            if run_enabled && !self.is_editing() {
                self.run_selected();
            }
        }

        ui.add_space(theme::SECTION_GAP);
        theme::inset(ui, |ui| self.actions_ui(ui));
    }

    /// The row under the list: the untrack confirmation or Track / Run with
    /// Arguments / Edit / Untrack, then the save status.
    fn actions_ui(&mut self, ui: &mut egui::Ui) {
        if let Mode::ConfirmUntrack { name, .. } = &self.mode {
            let name = name.clone();
            ui.horizontal(|ui| {
                ui.label(format!("Untrack \"{name}\"? The file stays on disk."));
                let saving = self.saving();
                if ui
                    .add_enabled(!saving, egui::Button::new("Untrack"))
                    .on_hover_text("Enter")
                    .clicked()
                {
                    self.confirm_untrack();
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
            let has_selection = self.selected().is_some();
            ui.horizontal(|ui| {
                if ui.button("Track…").on_hover_text("Ctrl+N").clicked() {
                    self.start_track();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Run with Arguments…"))
                    .on_hover_text("Tab or Ctrl+Enter")
                    .clicked()
                {
                    self.ask_args();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Edit"))
                    .on_hover_text("Ctrl+E")
                    .clicked()
                {
                    self.edit_selected();
                }
                if ui
                    .add_enabled(has_selection, egui::Button::new("Untrack"))
                    .on_hover_text("Shift+Delete")
                    .clicked()
                {
                    self.ask_untrack();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if form::config_link(ui, "Scripts are stored in config.toml") {
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
            self.results = search(query, &self.scripts);
            self.results_for = Some(query.to_owned());
        }
        // Kept until the list shows it: the frame after a save may still
        // filter with the old query, which the saved script can fail.
        if let Some(row) = self
            .select_id
            .as_ref()
            .and_then(|id| self.results.iter().position(|r| r == id))
        {
            self.cursor.selected = row;
            self.select_id = None;
        }
        self.cursor.clamp_selected(self.results.len());
        list_cursor::keep_visible(self.cursor.selected, &mut self.cursor.first_row, self.rows);
    }

    /// The prompt for one run's extra arguments, with the saved command
    /// they follow.
    fn args_ui(&mut self, ui: &mut egui::Ui, run_enabled: bool) {
        let Mode::RunWith {
            id,
            name,
            args,
            focus,
        } = &mut self.mode
        else {
            return;
        };
        ui.label(RichText::new(name.as_str()).strong());
        if let Some(script) = self.scripts.get(id.as_str()) {
            ui.label(
                RichText::new(command_line(&script_file::resolve(script, &[])))
                    .weak()
                    .small()
                    .monospace(),
            );
        }
        ui.add_space(theme::SECTION_GAP);
        let field = ui.add(
            egui::TextEdit::singleline(args)
                .hint_text("arguments for this run")
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if std::mem::take(focus) {
            field.request_focus();
        }
        ui.label(
            RichText::new("Added after the saved arguments. Quote one that has spaces.")
                .weak()
                .small(),
        );
        ui.add_space(theme::SECTION_GAP);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(run_enabled, egui::Button::new("Run"))
                .on_hover_text("Enter")
                .clicked()
            {
                self.run_with_args();
            }
            if ui.button("Cancel").on_hover_text("Esc").clicked() {
                self.escape();
            }
        });
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui, recording: bool) {
        let saving = self.saving();
        let Mode::Edit(editor) = &mut self.mode else {
            return;
        };
        let mut record = false;
        let mut pick = false;
        fn field<'t>(text: &'t mut String, hint: &str) -> egui::TextEdit<'t> {
            egui::TextEdit::singleline(text)
                .hint_text(hint.to_owned())
                .desired_width(320.0)
        }
        let command = editor.is_command();
        egui::Grid::new("script-editor")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                if command {
                    ui.label("Program");
                    ui.add_enabled(
                        !saving,
                        field(&mut editor.draft.interpreter, "pwsh.exe")
                            .font(egui::TextStyle::Monospace),
                    );
                } else {
                    ui.label("Script file");
                    ui.horizontal(|ui| {
                        let file = ui.add_enabled(
                            !saving,
                            egui::TextEdit::singleline(&mut editor.draft.path)
                                .hint_text("C:\\Tools\\deploy.ps1")
                                .desired_width(232.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        if file.lost_focus() && editor.draft.name.trim().is_empty() {
                            let path = editor.draft.path().to_owned();
                            if !path.is_empty() {
                                editor.set_path(&path);
                            }
                        }
                        pick = ui
                            .add_enabled(!saving, egui::Button::new("Browse…"))
                            .clicked();
                    });
                }
                ui.end_row();

                ui.label("");
                let language = script_file::language(&editor.script());
                ui.label(RichText::new(format!("Runs as: {language}")).weak().small());
                ui.end_row();

                ui.label("Name");
                let name = ui.add_enabled(!saving, field(&mut editor.draft.name, "Deploy"));
                if std::mem::take(&mut editor.focus) {
                    name.request_focus();
                }
                ui.end_row();

                ui.label("Arguments");
                ui.vertical(|ui| {
                    ui.add_enabled(
                        !saving,
                        field(&mut editor.draft.args, "optional, e.g. -Env prod")
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.label(
                        RichText::new("Passed on every run. Quote one that has spaces.")
                            .weak()
                            .small(),
                    );
                });
                ui.end_row();

                if command {
                    ui.label("Working folder");
                    ui.add_enabled(
                        !saving,
                        field(&mut editor.draft.cwd, "C:\\Tools").font(egui::TextStyle::Monospace),
                    );
                    ui.end_row();
                }

                ui.label("Hotkey");
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !saving && !recording,
                        egui::TextEdit::singleline(&mut editor.draft.hotkey)
                            .hint_text("optional")
                            .desired_width(200.0),
                    );
                    let label = if recording { "Press keys…" } else { "Record" };
                    let response =
                        ui.add_enabled(!saving, egui::Button::new(label).selected(recording));
                    record = response.clicked() && !recording;
                });
                ui.end_row();
            });
        self.record_request |= record;
        self.pick_request |= pick;
        if editor.dirty() {
            // Any new edit withdraws a pending discard confirmation.
            editor.confirm_discard &= ui.input(|i| !i.events.iter().any(form::is_text_event));
        }

        let dirty = editor.dirty();
        let confirm_discard = editor.confirm_discard;
        let problem = dirty
            .then(|| editor.problem(&self.scripts, &self.summon))
            .flatten();
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

/// Ids matching `query` (trimmed), best first: fuzzy on the name or id by
/// score, ties by lowercase name. An empty query lists everything by name.
fn search(query: &str, scripts: &BTreeMap<String, ScriptConfig>) -> Vec<String> {
    let matcher = SkimMatcherV2::default();
    let mut hits: Vec<(i64, String, &String)> = scripts
        .iter()
        .filter_map(|(id, s)| {
            let score = if query.is_empty() {
                0
            } else {
                let name = matcher.fuzzy_match(&s.name, query);
                name.max(matcher.fuzzy_match(id, query))?
            };
            Some((score, s.name.to_lowercase(), id))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
    hits.into_iter().map(|(_, _, id)| id.clone()).collect()
}

/// The exact process as one readable line.
fn command_line(run: &script_file::Resolved) -> String {
    let mut line = script_file::join_args(std::slice::from_ref(&run.interpreter));
    if !run.args.is_empty() {
        line.push(' ');
        line.push_str(&script_file::join_args(&run.args));
    }
    line
}

/// What runs, the exact command, where, and the hotkey.
fn preview_ui(ui: &mut egui::Ui, script: &ScriptConfig) {
    let run = script_file::resolve(script, &[]);
    ui.label(RichText::new(script_file::language(script)).strong());
    ui.add_space(theme::SECTION_GAP);
    ui.label(RichText::new(command_line(&run)).monospace());
    ui.add_space(theme::SECTION_GAP);
    ui.label(RichText::new(format!("in {}", run.cwd)).weak().small());
    if let Some(key) = &script.hotkey {
        ui.label(RichText::new(key).weak().small().monospace());
    }
}

fn row_ui(ui: &mut egui::Ui, script: &ScriptConfig, state: list::RowState) -> egui::Response {
    list::row(
        ui,
        state,
        &script.name,
        |ui| {
            ui.label("⌨");
        },
        |ui| {
            let language = script_file::language(script);
            let trailing = match &script.hotkey {
                Some(key) => format!("{language} · {key}"),
                None => language,
            };
            ui.label(RichText::new(trailing).weak().small());
        },
    )
}
