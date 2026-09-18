//! Thin `eframe::App` adapter (plan §4): owns the app model plus the main-thread
//! resources (hotkey manager, tray, clipboard thread), wires the producer event
//! handlers, and drives viewport lifecycle. All UI and event reduction live in
//! `model`.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use egui::{Context, ViewportCommand};

use crate::app::model::{AppEvent, AppModel, HotkeyStatus, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::core::config::{self, ConfigFile};
use crate::win::clipboard::ClipboardThread;
use crate::win::hotkey::{self, HotkeyManager};
use crate::win::tray::{self, TrayHandle};

/// The `eframe::App` and main-thread resource owner.
pub struct EframeApp {
    model: AppModel,
    hotkeys: Option<HotkeyManager>,
    tray: Option<TrayHandle>,
    clipboard: Option<ClipboardThread>,
    refresh_worker: Option<JoinHandle<()>>,
    event_tx: Sender<AppEvent>,
    ctx: Context,
    config_path: PathBuf,
    close_sent: bool,
    shutdown_done: bool,
}

impl EframeApp {
    /// Build the full app from within the eframe app-creation callback (main
    /// thread): hotkey manager, tray, handler installation, and initial summon
    /// registration (§1.1). Infallible: every failure is surfaced through the
    /// notice/conflict machinery rather than aborting creation.
    pub fn build(
        cc: &eframe::CreationContext<'_>,
        model: AppModel,
        event_tx: Sender<AppEvent>,
        config_path: PathBuf,
        clipboard: ClipboardThread,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let wake = Wake::new(event_tx.clone(), ctx.clone());

        let mut model = model;

        // Hotkey manager + initial summon registration (§5.2). A conflict forces
        // the visible recovery state; a missing manager leaves hotkeys off but
        // keeps the tray operable.
        let mut hotkeys = match HotkeyManager::new() {
            Ok(h) => Some(h),
            Err(e) => {
                model.report_hotkey_conflict(model.summon_hotkey().to_owned(), Some(e.to_string()));
                None
            }
        };
        if let Some(h) = &mut hotkeys {
            let key = model.summon_hotkey().to_owned();
            if let Err(e) = h.set_summon(&key) {
                model.report_hotkey_conflict(key, Some(e.to_string()));
            }
            hotkey::install_event_handler(wake.clone(), h.routes());
        }

        // Tray icon + menu (§5.2). Failure keeps the app running but reports a
        // visible notice; the hotkey path still works.
        let tray = match TrayHandle::build() {
            Ok(tray) => {
                tray::install_event_handlers(wake);
                Some(tray)
            }
            Err(e) => {
                model.push_notice(Notice::new(
                    Severity::Warning,
                    op::TRAY,
                    format!("Tray icon disabled: {e}"),
                ));
                None
            }
        };

        Self {
            model,
            hotkeys,
            tray,
            clipboard: Some(clipboard),
            refresh_worker: None,
            event_tx,
            ctx,
            config_path,
            close_sent: false,
            shutdown_done: false,
        }
    }

    /// Per-callback tick: drain events, service refresh/trial requests, and
    /// trigger quit. Called by both the hidden `logic` pass and the visible
    /// `ui` pass.
    fn step(&mut self, ctx: &Context) {
        self.model.step(ctx);

        // Reap a finished refresh worker; only one runs at a time (§1.4).
        if self
            .refresh_worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
            && let Some(worker) = self.refresh_worker.take()
        {
            let _ = worker.join();
        }

        if self.model.take_refresh_requested() {
            self.start_refresh();
        }

        if let Some(cfg) = self.model.take_pending_config() {
            self.apply_config(cfg);
        }

        if let Some(key) = self.model.take_trial_request() {
            self.apply_trial(key);
        }

        if self.model.quit_requested() && !self.close_sent {
            // Send Close exactly once; the actual resource teardown happens in
            // `on_exit` -> `shutdown_resources`.
            self.close_sent = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Re-read config off the UI thread; the result arrives as a
    /// `ConfigReload` event (§5.2). The worker only does file I/O.
    fn start_refresh(&mut self) {
        if self.refresh_worker.is_some() {
            return;
        }
        let path = self.config_path.clone();
        let wake = Wake::new(self.event_tx.clone(), self.ctx.clone());
        match std::thread::Builder::new()
            .name("smolcast-refresh".into())
            .spawn(move || {
                let result = config::load(&path);
                wake.send(AppEvent::ConfigReload { result });
            }) {
            Ok(handle) => self.refresh_worker = Some(handle),
            Err(e) => self.model.push_notice(Notice::new(
                Severity::Warning,
                op::CONFIG,
                format!("Could not start refresh: {e}"),
            )),
        }
    }

    /// Register a valid new summon key before unregistering the old one; on
    /// conflict retain the old registration and report (§5.2).
    fn apply_config(&mut self, cfg: ConfigFile) {
        let new_key = cfg.settings.summon_hotkey.clone();
        let conflicting = matches!(self.model.hotkey_status(), HotkeyStatus::Conflict { .. });
        let changed = new_key != self.model.summon_hotkey();
        if !changed && !conflicting {
            self.model.accept_config(cfg);
            return;
        }
        match self.hotkeys.as_mut().map(|h| h.set_summon(&new_key)) {
            Some(Ok(_)) => self.model.accept_config(cfg),
            Some(Err(e)) => self
                .model
                .report_hotkey_conflict(new_key, Some(e.to_string())),
            None => self
                .model
                .report_hotkey_conflict(new_key, Some("no hotkey manager".into())),
        }
    }

    /// Apply a session-only replacement key from the conflict UI (§5.2); it is
    /// not persisted.
    fn apply_trial(&mut self, key: String) {
        match self.hotkeys.as_mut().map(|h| h.set_summon(&key)) {
            Some(Ok(_)) => self.model.accept_trial_hotkey(key),
            Some(Err(e)) => self.model.report_hotkey_conflict(key, Some(e.to_string())),
            None => self
                .model
                .report_hotkey_conflict(key, Some("no hotkey manager".into())),
        }
    }

    /// Ordered resource shutdown (§1.5 steps 1-5): clear handlers, unregister
    /// and drop hotkeys/tray, then stop and join the clipboard thread. Runs
    /// inside `on_exit`, before eframe destroys the viewport.
    fn shutdown_resources(&mut self) {
        // eframe invokes `on_exit` once per shutdown; guard against re-entry.
        if self.shutdown_done {
            return;
        }
        self.shutdown_done = true;

        hotkey::clear_event_handler();
        tray::clear_event_handlers();

        // Unregister the summon hotkey, then drop managers/tray (§1.5 step 3).
        if let Some(h) = &mut self.hotkeys {
            h.unregister_all();
        }
        self.hotkeys = None;
        self.tray = None;

        // Clipboard worker stop and join (§1.5 step 4).
        if let Some(clipboard) = self.clipboard.take() {
            clipboard.shutdown();
        }

        if let Some(worker) = self.refresh_worker.take() {
            let _ = worker.join();
        }
    }
}

impl eframe::App for EframeApp {
    /// Runs even when hidden, whenever a producer requests a repaint. Drains
    /// events without painting.
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.step(ctx);
    }

    /// Painted frame for the root viewport.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.step(&ctx);
        if self.model.is_visible() {
            egui::CentralPanel::default()
                .frame(egui::Frame::window(ui.style()).inner_margin(egui::Margin::same(16)))
                .show(ui, |ui| self.model.ui(ui));
        }
    }

    /// Called once on shutdown, before eframe destroys the viewport. Ordered
    /// cleanup happens here so handlers are gone before the event loop stops.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_resources();
    }
}
