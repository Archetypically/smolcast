//! Backend-neutral app state, event reduction, and the palette UI.
//!
//! `AppModel::ui` is shaped like `fn ui(&mut self, ctx: &egui::Context)` and
//! must not mention `eframe::Frame`, preserving the raw-winit fallback boundary
//! (`plans/IMPLEMENTATION_PLAN.md` §4). Event handling lives here so the eframe
//! adapter can stay a thin lifecycle shim.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use egui::{Color32, Context, Key, RichText, ViewportCommand};

use crate::core::config::{ConfigError, ConfigFile};
use crate::win::paste::{Destination, capture_destination};

use super::notice::{Notice, NoticeStore, Severity, op};

/// Typed application events produced by window/background components (§1.2).
/// This is the semantic event set; it grows as later milestones add clipboard,
/// index, launch, and script events.
#[derive(Debug)]
pub enum AppEvent {
    SummonRequested,
    Tray(TrayCommand),
    /// Result of an off-thread config re-read. The last-known-good config stays
    /// active until the summon hotkey has been re-registered successfully.
    ConfigReload {
        result: Result<ConfigFile, ConfigError>,
    },
    Notice(Notice),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    OpenPalette,
    Refresh,
    Quit,
}

/// Producer wake handle: one event sender plus an `egui::Context`. Enforces the
/// send-then-repaint producer contract (§1.2): send, then request a repaint,
/// never wait for the UI. Once the shared `shutting_down` flag is set (§1.5
/// step 1), late callbacks become no-ops.
#[derive(Clone, Debug)]
pub struct Wake {
    tx: mpsc::Sender<AppEvent>,
    ctx: Context,
    shutting_down: Arc<AtomicBool>,
}

impl Wake {
    pub fn new(tx: mpsc::Sender<AppEvent>, ctx: Context, shutting_down: Arc<AtomicBool>) -> Self {
        Self {
            tx,
            ctx,
            shutting_down,
        }
    }

    pub fn send(&self, event: AppEvent) {
        if self.shutting_down.load(Ordering::Acquire) {
            return;
        }
        if self.tx.send(event).is_ok() {
            self.ctx.request_repaint();
        }
    }

    pub fn sender(&self) -> mpsc::Sender<AppEvent> {
        self.tx.clone()
    }

    pub fn ctx(&self) -> &Context {
        &self.ctx
    }
}

/// Summon hotkey registration outcome (§5.2). `Conflict` keeps the palette
/// usable and offers a session-only replacement key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyStatus {
    Active {
        canonical: String,
    },
    Conflict {
        requested: String,
        reason: Option<String>,
    },
}

/// The number of most-recent notices rendered in the palette at once. The store
/// keeps up to `MAX_NOTICES`; the rest stay available on the next show.
const VISIBLE_NOTICES: usize = 5;

pub struct AppModel {
    receiver: Receiver<AppEvent>,
    config: ConfigFile,
    hotkey_status: HotkeyStatus,
    destination: Option<Destination>,
    notices: NoticeStore,
    visible: bool,
    search: String,
    refresh_requested: bool,
    pending_config: Option<ConfigFile>,
    /// The conflict UI is capturing the next key chord as a session-only
    /// summon key; Escape cancels capture instead of hiding.
    recording: bool,
    capture_hint: Option<String>,
    trial_request: Option<String>,
    quit_requested: bool,
    force_show: bool,
}

impl AppModel {
    /// `startup_notice` and `force_visible` let a fatal startup/config error
    /// force the palette into view before the first frame (§2.1, §5.3).
    pub fn new(
        receiver: Receiver<AppEvent>,
        config: ConfigFile,
        startup_notice: Option<Notice>,
        force_visible: bool,
    ) -> Self {
        let mut notices = NoticeStore::new();
        if let Some(n) = startup_notice {
            notices.push(n);
        }
        let hotkey_status = HotkeyStatus::Active {
            canonical: config.settings.summon_hotkey.clone(),
        };
        Self {
            receiver,
            config,
            hotkey_status,
            destination: None,
            notices,
            visible: false,
            search: String::new(),
            refresh_requested: false,
            pending_config: None,
            recording: false,
            capture_hint: None,
            trial_request: None,
            quit_requested: false,
            force_show: force_visible,
        }
    }

    /// Drain queued events and apply viewport transitions. Called before every
    /// painted frame and, via the host's `logic`, whenever a repaint arrives
    /// while hidden.
    pub fn step(&mut self, ctx: &Context) {
        if self.force_show && !self.visible {
            self.show(ctx);
            self.force_show = false;
        }

        while let Ok(event) = self.receiver.try_recv() {
            self.reduce(ctx, event);
        }

        if self.visible && !self.recording && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.hide(ctx);
        }

        // The window `X` hides rather than exits (§1.5): cancel the close and sink.
        // A pending tray Quit is the one close that must go through.
        if !self.quit_requested && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide(ctx);
        }
    }

    /// Render the palette. Painting runs through `&mut egui::Ui` because egui
    /// 0.36 panel APIs take a `Ui`, not a `Context`; the model still never
    /// mentions `eframe::Frame`. The host wraps this in the root panel.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.palette_ui(ui);
    }

    // ---- host-facing accessors -------------------------------------------------

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn push_notice(&mut self, notice: Notice) {
        self.notices.push(notice);
    }

    pub fn take_refresh_requested(&mut self) -> bool {
        std::mem::take(&mut self.refresh_requested)
    }

    /// A config re-read that must be applied by the host (hotkey re-registration).
    pub fn take_pending_config(&mut self) -> Option<ConfigFile> {
        self.pending_config.take()
    }

    /// A session-only replacement summon key submitted from the conflict UI.
    /// Only set when the user explicitly applies; the edit buffer differs.
    pub fn take_trial_request(&mut self) -> Option<String> {
        self.trial_request.take()
    }

    pub fn summon_hotkey(&self) -> &str {
        &self.config.settings.summon_hotkey
    }

    pub fn hotkey_status(&self) -> &HotkeyStatus {
        &self.hotkey_status
    }

    /// Called by the host after a summon hotkey registered successfully.
    pub fn accept_config(&mut self, cfg: ConfigFile) {
        self.hotkey_status = HotkeyStatus::Active {
            canonical: cfg.settings.summon_hotkey.clone(),
        };
        self.config = cfg;
        self.notices.push(Notice::new(
            Severity::Info,
            op::CONFIG,
            "Configuration reloaded.",
        ));
    }

    /// Apply a session-only hotkey that registered successfully; it is not
    /// persisted, so the recovery UI keeps showing the needed config edit.
    pub fn accept_trial_hotkey(&mut self, key: String) {
        self.capture_hint = None;
        self.hotkey_status = HotkeyStatus::Active { canonical: key };
        self.notices.push(Notice::new(
            Severity::Info,
            op::SUMMON_HOTKEY,
            "Temporary hotkey active for this session. Set summon_hotkey in config.toml to persist it.",
        ));
    }

    /// A registration failure. Keep the old registration and registration state;
    /// force the palette visible so the recovery UI is observable (§5.2).
    pub fn report_hotkey_conflict(&mut self, requested: String, reason: Option<String>) {
        self.capture_hint = None;
        self.hotkey_status = HotkeyStatus::Conflict {
            requested: requested.clone(),
            reason,
        };
        self.force_show = true;
        self.notices.push(Notice::new(
            Severity::Error,
            op::SUMMON_HOTKEY,
            format!(
                "Hotkey \"{requested}\" could not be registered. Open the palette and apply a \
                 temporary key, or edit summon_hotkey = \"{requested}\" in config.toml."
            ),
        ));
    }

    // ---- event reduction --------------------------------------------------------

    fn reduce(&mut self, ctx: &Context, event: AppEvent) {
        match event {
            AppEvent::SummonRequested => self.summon(ctx),
            AppEvent::Tray(TrayCommand::OpenPalette) => self.summon(ctx),
            AppEvent::Tray(TrayCommand::Refresh) => self.refresh_requested = true,
            AppEvent::Tray(TrayCommand::Quit) => self.quit_requested = true,
            AppEvent::ConfigReload { result } => match result {
                Ok(cfg) => self.pending_config = Some(cfg),
                Err(e) => self.notices.push(Notice::new(
                    Severity::Warning,
                    op::CONFIG,
                    format!("Config reload failed: {e}"),
                )),
            },
            AppEvent::Notice(n) => self.notices.push(n),
        }
    }

    /// Summon is idempotent (§5.2): show and focus when hidden, only focus when
    /// already visible, and never replace the captured destination while showing.
    fn summon(&mut self, ctx: &Context) {
        if !self.visible {
            self.destination = capture_destination();
            self.show(ctx);
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    fn show(&mut self, ctx: &Context) {
        self.visible = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn hide(&mut self, ctx: &Context) {
        self.visible = false;
        self.recording = false;
        self.destination = None;
        self.search.clear();
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    // ---- palette UI -------------------------------------------------------------

    fn palette_ui(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(440.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("smolcast").heading().strong());
            match &self.hotkey_status {
                HotkeyStatus::Active { canonical } => {
                    ui.label(RichText::new(format!(" ({canonical})")).weak());
                }
                HotkeyStatus::Conflict { requested, .. } => {
                    ui.label(
                        RichText::new(format!(" (hotkey conflict: {requested})"))
                            .color(Color32::from_rgb(220, 120, 60)),
                    );
                }
            }
        });

        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search… apps arrive in milestone 2")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(8.0);

        if matches!(self.hotkey_status, HotkeyStatus::Conflict { .. }) {
            self.hotkey_capture_ui(ui);
        }

        self.notices_ui(ui);
    }

    /// Session-only replacement key (§5.2): record a pressed chord rather than
    /// asking the user to type `global-hotkey` syntax.
    fn hotkey_capture_ui(&mut self, ui: &mut egui::Ui) {
        if self.recording {
            self.capture_chord(ui);
        }
        ui.horizontal(|ui| {
            let label = if self.recording {
                "Press a key combination… (Esc cancels)"
            } else {
                "Record temporary key"
            };
            let response = ui.add(egui::Button::new(label).selected(self.recording));
            if response.clicked() {
                self.recording = !self.recording;
                self.capture_hint = None;
                // Nothing may keep keyboard focus while recording: a focused
                // button activates on Space/Enter and a text field would
                // receive the chord's character.
                response.surrender_focus();
                ui.memory_mut(|m| m.stop_text_input());
            }
        });
        if let Some(hint) = &self.capture_hint {
            ui.label(RichText::new(hint).weak().small());
        }
    }

    /// Consume key events while recording. A chord needs Ctrl or Alt so a
    /// global registration never swallows ordinary typing.
    fn capture_chord(&mut self, ui: &mut egui::Ui) {
        let events = ui.input_mut(|i| {
            let events = std::mem::take(&mut i.events);
            let (keys, rest): (Vec<_>, Vec<_>) = events
                .into_iter()
                .partition(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)));
            i.events = rest;
            keys
        });
        for event in events {
            let egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                repeat: false,
                modifiers,
            } = event
            else {
                continue;
            };
            let key = physical_key.unwrap_or(key);
            if key == Key::Escape && !modifiers.any() {
                self.recording = false;
                self.capture_hint = None;
                return;
            }
            if !(modifiers.ctrl || modifiers.alt) {
                self.capture_hint = Some("Include Ctrl or Alt in the combination.".into());
                continue;
            }
            let Some(code) = hotkey_code(key) else {
                self.capture_hint = Some(format!("{} can't be used as a hotkey.", key.name()));
                continue;
            };
            let mut chord = String::new();
            for (held, name) in [
                (modifiers.ctrl, "control"),
                (modifiers.alt, "alt"),
                (modifiers.shift, "shift"),
            ] {
                if held {
                    chord.push_str(name);
                    chord.push('+');
                }
            }
            chord.push_str(code);
            self.recording = false;
            self.capture_hint = Some(format!("Trying {chord}…"));
            self.trial_request = Some(chord);
            return;
        }
    }

    fn notices_ui(&mut self, ui: &mut egui::Ui) {
        if self.notices.is_empty() {
            return;
        }
        ui.separator();
        let total = self.notices.len();
        let shown = total.min(VISIBLE_NOTICES);
        for notice in self.notices.iter().skip(total - shown) {
            let color = match notice.severity {
                Severity::Info => Color32::from_gray(150),
                Severity::Warning => Color32::from_rgb(230, 180, 60),
                Severity::Error => Color32::from_rgb(230, 90, 90),
            };
            ui.horizontal_wrapped(|ui| {
                if let Some(code) = notice.code {
                    ui.label(
                        RichText::new(format!("{} [0x{code:08X}]", notice.message))
                            .color(color)
                            .small(),
                    );
                } else {
                    ui.label(RichText::new(&notice.message).color(color).small());
                }
            });
        }
        if total > shown {
            ui.label(
                RichText::new(format!("… and {} more", total - shown))
                    .weak()
                    .small(),
            );
        }
        if ui.button("Dismiss notices").clicked() {
            self.notices.clear();
        }
    }
}

/// Map an egui physical key to the `keyboard_types::Code` name accepted by
/// `global-hotkey` on Windows. Modifier-only and logical-only keys have none.
fn hotkey_code(key: Key) -> Option<&'static str> {
    use Key::*;
    Some(match key {
        A => "KeyA",
        B => "KeyB",
        C => "KeyC",
        D => "KeyD",
        E => "KeyE",
        F => "KeyF",
        G => "KeyG",
        H => "KeyH",
        I => "KeyI",
        J => "KeyJ",
        K => "KeyK",
        L => "KeyL",
        M => "KeyM",
        N => "KeyN",
        O => "KeyO",
        P => "KeyP",
        Q => "KeyQ",
        R => "KeyR",
        S => "KeyS",
        T => "KeyT",
        U => "KeyU",
        V => "KeyV",
        W => "KeyW",
        X => "KeyX",
        Y => "KeyY",
        Z => "KeyZ",
        Num0 => "Digit0",
        Num1 => "Digit1",
        Num2 => "Digit2",
        Num3 => "Digit3",
        Num4 => "Digit4",
        Num5 => "Digit5",
        Num6 => "Digit6",
        Num7 => "Digit7",
        Num8 => "Digit8",
        Num9 => "Digit9",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        F21 => "F21",
        F22 => "F22",
        F23 => "F23",
        F24 => "F24",
        ArrowDown => "ArrowDown",
        ArrowLeft => "ArrowLeft",
        ArrowRight => "ArrowRight",
        ArrowUp => "ArrowUp",
        Space => "Space",
        Enter => "Enter",
        Tab => "Tab",
        Backspace => "Backspace",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Backtick => "Backquote",
        Minus => "Minus",
        Equals => "Equal",
        OpenBracket => "BracketLeft",
        CloseBracket => "BracketRight",
        Backslash => "Backslash",
        Semicolon => "Semicolon",
        Quote => "Quote",
        Comma => "Comma",
        Period => "Period",
        Slash => "Slash",
        _ => return None,
    })
}
