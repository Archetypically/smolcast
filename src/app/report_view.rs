//! Report a Bug (plan §14): shows what a new GitHub issue will carry, then
//! opens it in the browser. The last crash report goes along only when the
//! user ticks it; nothing is sent until they submit the issue on GitHub.

use egui::{Key, Modifiers, RichText};

use crate::core::crash_log::{self, LastCrash};

use super::theme;

/// The crash preview's height before it scrolls.
const PREVIEW_HEIGHT: f32 = 120.0;

pub struct ReportView {
    windows: String,
    /// The last crash, its text cut to what the issue carries, so the
    /// preview is exactly what is sent.
    crash: Option<LastCrash>,
    include_crash: bool,
    open_request: Option<String>,
}

impl ReportView {
    pub fn new(windows: String, crash: Option<LastCrash>) -> Self {
        Self {
            windows,
            crash: crash.map(|c| LastCrash {
                text: crash_log::issue_excerpt(&c.text),
                ..c
            }),
            include_crash: false,
            open_request: None,
        }
    }

    /// Each visit starts with the crash report left out.
    pub fn open(&mut self) {
        self.include_crash = false;
    }

    pub fn handle_keys(&mut self, ui: &mut egui::Ui) {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            self.request_open();
        }
    }

    /// The issue URL to open, once.
    pub fn take_open_request(&mut self) -> Option<String> {
        self.open_request.take()
    }

    fn request_open(&mut self) {
        let crash = self
            .crash
            .as_ref()
            .filter(|_| self.include_crash)
            .map(|c| c.text.as_str());
        self.open_request = Some(crash_log::issue_url(
            env!("CARGO_PKG_REPOSITORY"),
            env!("CARGO_PKG_VERSION"),
            &self.windows,
            crash,
        ));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(theme::SECTION_GAP);
        ui.label("Opens a new GitHub issue in your browser, filled in with:");
        ui.label(
            RichText::new(format!(
                "smolcast {} · {}",
                env!("CARGO_PKG_VERSION"),
                self.windows
            ))
            .monospace(),
        );
        if let Some(crash) = &self.crash {
            ui.add_space(theme::SECTION_GAP);
            ui.checkbox(
                &mut self.include_crash,
                format!("Include the crash report from {}", crash.when),
            );
            if self.include_crash {
                egui::ScrollArea::vertical()
                    .max_height(PREVIEW_HEIGHT)
                    .show(ui, |ui| {
                        ui.label(RichText::new(&crash.text).monospace().small());
                    });
            }
        }
        ui.add_space(theme::SECTION_GAP);
        ui.label(
            RichText::new("You can edit it all on GitHub. Nothing is sent until you submit it.")
                .small()
                .weak(),
        );
        ui.add_space(theme::SECTION_GAP);
        if ui.button("Open GitHub Issue").clicked() {
            self.request_open();
        }
    }
}
