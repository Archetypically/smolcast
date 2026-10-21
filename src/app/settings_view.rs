//! Settings form (plan §12.7): a structured editor for `[settings]` in
//! `config.toml`. The file stays the source of truth: Save asks the host to
//! write only the changed keys (`core::config_edit`) and then runs the normal
//! reload, so a saved form and a hand-edited file take the same path. Scripts
//! are edited in the file itself.

use std::path::Path;

use egui::{Color32, Key, Modifiers, RichText};

use crate::core::config::{
    ClipboardSource, MAX_CLIPBOARD_ENTRIES, MAX_CLIPBOARD_ITEM_BYTES, MAX_CLIPBOARD_TOTAL_BYTES,
    MAX_KEEP_STATE_SECS, MIN_CLIPBOARD_ENTRIES, MIN_CLIPBOARD_ITEM_BYTES, PaletteSize,
    SearchEngine, Settings, TextSize,
};
use crate::win::hotkey;

use super::theme;

const MIB: f64 = 1024.0 * 1024.0;

enum Status {
    Idle,
    Saving,
    Saved,
    Failed(String),
}

pub struct SettingsView {
    /// The accepted config's settings.
    saved: Settings,
    /// The form's current values.
    draft: Settings,
    status: Status,
    save_request: Option<Settings>,
    open_file: bool,
    record_request: bool,
}

impl SettingsView {
    pub fn new(settings: &Settings) -> Self {
        Self {
            saved: settings.clone(),
            draft: settings.clone(),
            status: Status::Idle,
            save_request: None,
            open_file: false,
            record_request: false,
        }
    }

    /// Opening the view starts from the accepted settings.
    pub fn open(&mut self) {
        if !matches!(self.status, Status::Saving) {
            self.draft = self.saved.clone();
            self.status = Status::Idle;
        }
    }

    /// A config was accepted (a save finished, or the file was reloaded).
    /// Unsaved edits survive an unrelated reload. Returns whether this
    /// completed a save from the form.
    pub fn config_accepted(&mut self, settings: &Settings) -> bool {
        let dirty = self.draft != self.saved;
        self.saved = settings.clone();
        if matches!(self.status, Status::Saving) {
            self.draft = settings.clone();
            self.status = Status::Saved;
            return true;
        }
        if !dirty {
            self.draft = settings.clone();
        }
        false
    }

    /// A save failed before or during the reload that applies it.
    pub fn save_failed(&mut self, message: String) {
        if matches!(self.status, Status::Saving) {
            self.status = Status::Failed(message);
        }
    }

    pub fn set_hotkey(&mut self, chord: String) {
        self.draft.summon_hotkey = chord;
    }

    pub fn take_save_request(&mut self) -> Option<Settings> {
        self.save_request.take()
    }

    pub fn take_open_file(&mut self) -> bool {
        std::mem::take(&mut self.open_file)
    }

    pub fn take_record_request(&mut self) -> bool {
        std::mem::take(&mut self.record_request)
    }

    /// Ctrl+S saves.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui) {
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save();
        }
    }

    fn problem(&self) -> Option<String> {
        if let Err(e) = hotkey::check(&self.draft.summon_hotkey) {
            return Some(format!("Summon hotkey: {e}"));
        }
        // The path only appears in file errors; form errors drop the prefix.
        self.draft.validate(Path::new("")).err().map(|e| {
            let text = e.to_string();
            text.strip_prefix("invalid : ").unwrap_or(&text).to_owned()
        })
    }

    fn save(&mut self) {
        if self.draft != self.saved
            && self.problem().is_none()
            && !matches!(self.status, Status::Saving)
        {
            self.status = Status::Saving;
            self.save_request = Some(self.draft.clone());
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, recording: bool) {
        let draft = &mut self.draft;

        theme::section_label(ui, "General");
        form(ui, "general", |ui| {
            ui.label("Summon hotkey");
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !recording,
                    egui::TextEdit::singleline(&mut draft.summon_hotkey).desired_width(200.0),
                );
                let label = if recording {
                    "Press keys… (Esc cancels)"
                } else {
                    "Record"
                };
                let response = ui.add(egui::Button::new(label).selected(recording));
                if response.clicked() && !recording {
                    self.record_request = true;
                    // A focused button would activate on Space/Enter.
                    response.surrender_focus();
                    ui.memory_mut(|m| m.stop_text_input());
                }
            });
            ui.end_row();

            ui.label("Startup");
            ui.checkbox(&mut draft.start_at_login, "Start smolcast when you sign in");
            ui.end_row();

            ui.label("Web search");
            egui::ComboBox::from_id_salt("search_engine")
                .width(COMBO_WIDTH)
                .selected_text(draft.search_engine.name())
                .show_ui(ui, |ui| {
                    for engine in SearchEngine::ALL {
                        ui.selectable_value(&mut draft.search_engine, engine, engine.name());
                    }
                });
            ui.end_row();

            if draft.search_engine == SearchEngine::Custom {
                ui.label("");
                ui.vertical(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.web_search_url)
                            .desired_width(COMBO_WIDTH + 80.0),
                    );
                    hint(ui, "%s is replaced by the query");
                });
                ui.end_row();
            }
        });

        section_gap(ui);
        theme::section_label(ui, "Palette");
        form(ui, "palette", |ui| {
            ui.label("Window size");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("palette_size")
                    .width(SIZE_COMBO_WIDTH)
                    .selected_text(palette_size_label(draft.palette_size))
                    .show_ui(ui, |ui| {
                        for size in PaletteSize::ALL {
                            ui.selectable_value(
                                &mut draft.palette_size,
                                size,
                                palette_size_label(size),
                            );
                        }
                    });
                hint(ui, "larger shows more results");
            });
            ui.end_row();

            ui.label("Text size");
            egui::ComboBox::from_id_salt("text_size")
                .width(SIZE_COMBO_WIDTH)
                .selected_text(text_size_label(draft.text_size))
                .show_ui(ui, |ui| {
                    for size in TextSize::ALL {
                        ui.selectable_value(&mut draft.text_size, size, text_size_label(size));
                    }
                });
            ui.end_row();

            ui.label("Keep state");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.keep_state_secs)
                        .range(0..=MAX_KEEP_STATE_SECS)
                        .suffix(" s"),
                );
                hint(ui, "after dismissing; 0 always starts fresh");
            });
            ui.end_row();
        });

        section_gap(ui);
        theme::section_label(ui, "Clipboard");
        form(ui, "clipboard", |ui| {
            ui.label("History");
            ui.checkbox(&mut draft.clipboard_capture, "Record clipboard history");
            ui.end_row();

            ui.label("Source");
            egui::ComboBox::from_id_salt("clipboard_source")
                .width(COMBO_WIDTH)
                .selected_text(source_label(draft.clipboard_source))
                .show_ui(ui, |ui| {
                    for source in [
                        ClipboardSource::Auto,
                        ClipboardSource::Windows,
                        ClipboardSource::Smolcast,
                    ] {
                        ui.selectable_value(
                            &mut draft.clipboard_source,
                            source,
                            source_label(source),
                        );
                    }
                });
            ui.end_row();

            ui.label("Persistence");
            ui.checkbox(
                &mut draft.clipboard_persist,
                "Keep smolcast's history across restarts (encrypted)",
            );
            ui.end_row();

            ui.label("Max entries");
            ui.add(
                egui::DragValue::new(&mut draft.clipboard_max_entries)
                    .range(MIN_CLIPBOARD_ENTRIES..=MAX_CLIPBOARD_ENTRIES),
            );
            ui.end_row();

            ui.label("Max item size");
            mib_field(
                ui,
                &mut draft.clipboard_max_item_bytes,
                MIN_CLIPBOARD_ITEM_BYTES,
                MAX_CLIPBOARD_ITEM_BYTES,
            );
            ui.end_row();

            ui.label("Max total size");
            mib_field(
                ui,
                &mut draft.clipboard_max_total_bytes,
                MIN_CLIPBOARD_ITEM_BYTES,
                MAX_CLIPBOARD_TOTAL_BYTES,
            );
            ui.end_row();
        });

        section_gap(ui);
        self.actions_ui(ui);
    }

    /// Save and Revert with the form's status, then the config file link on
    /// the right.
    fn actions_ui(&mut self, ui: &mut egui::Ui) {
        let problem = self.problem();
        let dirty = self.draft != self.saved;
        let (text, color) = match (&problem, &self.status) {
            (Some(problem), _) => (problem.clone(), Color32::from_rgb(230, 90, 90)),
            (None, Status::Saving) => ("Saving…".to_owned(), ui.visuals().weak_text_color()),
            (None, Status::Failed(e)) => (e.clone(), Color32::from_rgb(230, 90, 90)),
            (None, Status::Saved) if !dirty => {
                ("Saved.".to_owned(), ui.visuals().weak_text_color())
            }
            (None, _) if dirty => (
                "Unsaved changes.".to_owned(),
                ui.visuals().weak_text_color(),
            ),
            (None, _) => (String::new(), ui.visuals().weak_text_color()),
        };
        ui.horizontal(|ui| {
            let saving = matches!(self.status, Status::Saving);
            let save = ui
                .add_enabled(
                    dirty && problem.is_none() && !saving,
                    egui::Button::new("Save"),
                )
                .on_hover_text("Ctrl+S");
            if save.clicked() {
                self.save();
            }
            if ui
                .add_enabled(dirty && !saving, egui::Button::new("Revert"))
                .clicked()
            {
                self.draft = self.saved.clone();
                self.status = Status::Idle;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Edit config file…")
                    .on_hover_text("Scripts are configured in config.toml")
                    .clicked()
                {
                    self.open_file = true;
                }
                if !text.is_empty() {
                    ui.add(egui::Label::new(RichText::new(text).color(color).small()).truncate());
                }
            });
        });
    }
}

/// Label column width, shared by every section so their controls line up.
const LABEL_WIDTH: f32 = 112.0;
const COMBO_WIDTH: f32 = 240.0;
const SIZE_COMBO_WIDTH: f32 = 120.0;
/// Vertical gap between form rows.
const ROW_GAP: f32 = 6.0;

/// A two-column section: labels on the left, controls on the right.
fn form(ui: &mut egui::Ui, id: &str, add_rows: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(LABEL_WIDTH)
        .spacing([theme::GUTTER as f32, ROW_GAP])
        .show(ui, add_rows);
}

fn section_gap(ui: &mut egui::Ui) {
    ui.add_space(2.0 * theme::SECTION_GAP);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).weak().small());
}

fn source_label(source: ClipboardSource) -> &'static str {
    match source {
        ClipboardSource::Auto => "Automatic (Windows history if on)",
        ClipboardSource::Windows => "Windows clipboard history",
        ClipboardSource::Smolcast => "smolcast history",
    }
}

fn palette_size_label(size: PaletteSize) -> &'static str {
    match size {
        PaletteSize::Small => "Small",
        PaletteSize::Medium => "Medium",
        PaletteSize::Large => "Large",
    }
}

fn text_size_label(size: TextSize) -> &'static str {
    match size {
        TextSize::Small => "Small",
        TextSize::Medium => "Medium",
        TextSize::Large => "Large",
    }
}

/// Edit a byte count in MiB. An untouched field keeps its exact byte value.
fn mib_field(ui: &mut egui::Ui, bytes: &mut usize, min: usize, max: usize) {
    let mut mib = *bytes as f64 / MIB;
    let response = ui.add(
        egui::DragValue::new(&mut mib)
            .range(min as f64 / MIB..=max as f64 / MIB)
            .speed(0.25)
            .max_decimals(2)
            .suffix(" MiB"),
    );
    if response.changed() {
        *bytes = (mib * MIB).round() as usize;
    }
}
