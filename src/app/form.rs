//! Parts the snippet and script editors share: the save status line, the
//! config file link, and the text helpers for form messages and the discard
//! prompt.

use egui::RichText;

use super::theme;

/// A form's save, shown on its status line.
pub enum Status {
    Idle,
    Saving,
    Done(&'static str),
    Failed(String),
}

/// One status line: a form problem or hint wins over the save status.
pub fn status_ui(ui: &mut egui::Ui, status: &Status, problem: Option<&str>) {
    let weak = ui.visuals().weak_text_color();
    let (text, color) = match (problem, status) {
        (Some(problem), _) => (problem.to_owned(), theme::ERROR),
        (None, Status::Saving) => ("Saving…".to_owned(), weak),
        (None, Status::Failed(e)) => (e.clone(), theme::ERROR),
        (None, Status::Done(done)) => ((*done).to_owned(), weak),
        (None, Status::Idle) => return,
    };
    ui.label(RichText::new(text).color(color).small());
}

/// The small link to config.toml, whose `hover` text says what the file
/// holds. True when clicked.
pub fn config_link(ui: &mut egui::Ui, hover: &str) -> bool {
    ui.small_button("Edit config file…")
        .on_hover_text(hover)
        .clicked()
}

pub fn is_text_event(event: &egui::Event) -> bool {
    matches!(event, egui::Event::Text(_) | egui::Event::Paste(_))
}

pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
