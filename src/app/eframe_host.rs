//! Thin `eframe::App` adapter (plan §4): owns the app model plus the main-thread
//! resources (hotkey manager, tray, clipboard thread), wires the producer event
//! handlers, and drives viewport lifecycle. All UI and event reduction live in
//! `model`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use egui::{Context, ViewportCommand};

use crate::app::model::loads_stored_history;
use crate::app::model::{AppEvent, AppModel, HotkeyStatus, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::core::config::{self, ConfigFile};
use crate::win::clipboard::ClipboardThread;
use crate::win::emoji_render::EmojiRenderer;
use crate::win::hotkey::{self, HotkeyManager};
use crate::win::paste;
use crate::win::persistence::{ClipboardStoreCommand, StatePaths, StateWriter};
use crate::win::shell::{self, IconWorker};
use crate::win::tray::{self, TrayHandle};

/// The `eframe::App` and main-thread resource owner.
pub struct EframeApp {
    model: AppModel,
    hotkeys: Option<HotkeyManager>,
    tray: Option<TrayHandle>,
    clipboard: Option<ClipboardThread>,
    refresh_worker: Option<JoinHandle<()>>,
    state_writer: Option<StateWriter>,
    icon_worker: Option<IconWorker>,
    emoji_renderer: Option<EmojiRenderer>,
    scan_job: Option<JoinHandle<()>>,
    /// Set during shutdown so a running scan stops between items (§1.5 step 5).
    scan_cancel: Arc<AtomicBool>,
    launch_jobs: Vec<JoinHandle<()>>,
    /// Paste finishers and their cancel flags (§8.2 steps 6–8).
    paste_jobs: Vec<(JoinHandle<()>, Arc<AtomicBool>)>,
    event_tx: Sender<AppEvent>,
    ctx: Context,
    paths: StatePaths,
    close_sent: bool,
    /// eframe 0.36 unconditionally shows the root window after its first painted
    /// frame (`EpiIntegration::post_rendering`), ignoring `with_visible(false)`.
    first_frame_seen: bool,
    /// Shared with every `Wake` producer; set first during shutdown (§1.5 step 1).
    shutting_down: Arc<AtomicBool>,
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
        paths: StatePaths,
        clipboard: ClipboardThread,
        shutting_down: Arc<AtomicBool>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let wake = Wake::new(event_tx.clone(), ctx.clone(), shutting_down.clone());

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
            // Script keys register after the summon key (§10.3).
            apply_script_hotkeys(h, &mut model);
            hotkey::install_event_handler(wake.clone(), h.routes());
        }

        // Launcher workers (§6): the usage writer and the icon extractor. A
        // failure disables that piece only and is reported.
        let state_writer = match StateWriter::start(
            paths.usage.clone(),
            paths.clipboard_dir.clone(),
            wake.clone(),
        ) {
            Ok(writer) => {
                // Stored clipboard history (§7.2): load it, or delete it when
                // persistence is off.
                let settings = &model.config().settings;
                if loads_stored_history(settings) {
                    writer.clipboard(ClipboardStoreCommand::Load {
                        item_cap: settings.clipboard_max_item_bytes,
                    });
                } else if !settings.clipboard_persist {
                    writer.clipboard(ClipboardStoreCommand::Clear);
                }
                Some(writer)
            }
            Err(e) => {
                model.push_notice(Notice::new(
                    Severity::Warning,
                    op::USAGE,
                    format!("Launch and clipboard history will not be saved: {e}"),
                ));
                model.clipboard_store_unavailable();
                None
            }
        };
        let icon_worker = match IconWorker::start(wake.clone()) {
            Ok(worker) => Some(worker),
            Err(e) => {
                model.push_notice(Notice::new(
                    Severity::Warning,
                    op::LAUNCHER,
                    format!("App icons disabled: {e}"),
                ));
                None
            }
        };

        let emoji_renderer = match EmojiRenderer::start(wake.clone()) {
            Ok(renderer) => Some(renderer),
            Err(e) => {
                model.push_notice(Notice::new(
                    Severity::Warning,
                    op::LAUNCHER,
                    format!("Emoji previews disabled: {e}"),
                ));
                None
            }
        };

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
            state_writer,
            icon_worker,
            emoji_renderer,
            scan_job: None,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            launch_jobs: Vec::new(),
            paste_jobs: Vec::new(),
            event_tx,
            ctx,
            paths,
            close_sent: false,
            first_frame_seen: false,
            shutting_down,
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

        self.run_launcher_requests();
        self.run_clipboard_requests();

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

    /// Execute the launcher's queued work (§6): scans, launches, icon lookups,
    /// and usage snapshots. The model guarantees at most one active scan.
    fn run_launcher_requests(&mut self) {
        if self.scan_job.as_ref().is_some_and(|job| job.is_finished())
            && let Some(job) = self.scan_job.take()
        {
            let _ = job.join();
        }
        self.launch_jobs.retain(|job| !job.is_finished());

        if let Some(generation) = self.model.take_scan_request() {
            match shell::spawn_scan(generation, self.scan_cancel.clone(), self.wake()) {
                Ok(job) => {
                    // A previous job has already delivered its result; join it
                    // before tracking the new one.
                    if let Some(old) = self.scan_job.replace(job) {
                        let _ = old.join();
                    }
                }
                Err(e) => self.model.scan_start_failed(generation, &e.to_string()),
            }
        }
        if let Some(request) = self.model.take_launch_request() {
            match shell::spawn_launch(request.app_id, request.spec, self.wake()) {
                Ok(job) => self.launch_jobs.push(job),
                Err(e) => self.model.launch_start_failed(&e.to_string()),
            }
        }
        let glyph_requests = self.model.take_glyph_requests();
        if let Some(renderer) = &self.emoji_renderer {
            for request in glyph_requests {
                renderer.request(request.index, request.text, request.edge);
            }
        }
        let icon_requests = self.model.take_icon_requests();
        if let Some(worker) = &self.icon_worker {
            for request in icon_requests {
                worker.request(request.app_id, request.icon_key, request.size);
            }
        }
        if let (Some(writer), Some((generation, bytes))) =
            (&self.state_writer, self.model.take_usage_snapshot())
        {
            writer.save_usage(generation, bytes);
        }
    }

    /// Forward clipboard commands to the clipboard thread and start paste
    /// finishers (§7.1, §8.2).
    fn run_clipboard_requests(&mut self) {
        if let Some(clipboard) = &self.clipboard {
            for command in self.model.take_clipboard_commands() {
                clipboard.send_command(command);
            }
        }
        let store = self.model.take_store_commands();
        if let Some(writer) = &self.state_writer {
            for command in store {
                writer.clipboard(command);
            }
        }
        self.paste_jobs.retain(|(job, _)| !job.is_finished());
        if let Some(request) = self.model.take_paste_finisher() {
            match paste::spawn_finisher(request.destination, request.cancel.clone(), self.wake()) {
                Ok(job) => self.paste_jobs.push((job, request.cancel)),
                Err(e) => self.model.paste_finisher_failed(&e.to_string()),
            }
        }
    }

    fn wake(&self) -> Wake {
        Wake::new(
            self.event_tx.clone(),
            self.ctx.clone(),
            self.shutting_down.clone(),
        )
    }

    /// Re-read config off the UI thread; the result arrives as a
    /// `ConfigReload` event (§5.2). The worker only does file I/O.
    fn start_refresh(&mut self) {
        if self.refresh_worker.is_some() {
            return;
        }
        let path = self.paths.config.clone();
        let wake = self.wake();
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
            self.refresh_script_hotkeys();
            return;
        }
        match self.hotkeys.as_mut().map(|h| h.set_summon(&new_key)) {
            Some(Ok(_)) => {
                self.model.accept_config(cfg);
                self.refresh_script_hotkeys();
            }
            Some(Err(e)) => self
                .model
                .report_hotkey_conflict(new_key, Some(e.to_string())),
            None => self
                .model
                .report_hotkey_conflict(new_key, Some("no hotkey manager".into())),
        }
    }

    /// Re-register script hotkeys for the accepted config.
    fn refresh_script_hotkeys(&mut self) {
        if let Some(h) = &mut self.hotkeys {
            apply_script_hotkeys(h, &mut self.model);
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
        // Step 1: set the shared flag so late handler/worker callbacks stop
        // sending. It also guards against re-entry.
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }

        hotkey::clear_event_handler();
        tray::clear_event_handlers();

        // Unregister the summon hotkey, then drop managers/tray (§1.5 step 3).
        if let Some(h) = &mut self.hotkeys {
            h.unregister_all();
        }
        self.hotkeys = None;
        self.tray = None;

        // Clipboard and persistence workers flush/stop and join (§1.5 step 4).
        if let Some(clipboard) = self.clipboard.take() {
            clipboard.shutdown();
        }
        if let Some(writer) = self.state_writer.take() {
            writer.shutdown();
        }

        // Scan/icon/launch jobs cancel between items and are joined (§1.5 step 5).
        self.scan_cancel.store(true, Ordering::Release);
        if let Some(job) = self.scan_job.take() {
            let _ = job.join();
        }
        if let Some(worker) = self.icon_worker.take() {
            worker.shutdown();
        }
        if let Some(renderer) = self.emoji_renderer.take() {
            renderer.shutdown();
        }
        for job in self.launch_jobs.drain(..) {
            let _ = job.join();
        }
        for (job, cancel) in self.paste_jobs.drain(..) {
            cancel.store(true, Ordering::Release);
            let _ = job.join();
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
        // Re-hide after eframe's forced first-frame show (§2.1: the root viewport
        // starts hidden). Viewport commands are applied after `post_rendering` in
        // the same frame, so this wins; at most one blank frame can flash.
        if !std::mem::replace(&mut self.first_frame_seen, true) && !self.model.is_visible() {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
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

/// Register the configured script hotkeys. A parse error or collision disables
/// only that script's hotkey; the script stays searchable (§10.3).
fn apply_script_hotkeys(hotkeys: &mut HotkeyManager, model: &mut AppModel) {
    let wanted: BTreeMap<String, String> = model
        .config()
        .scripts
        .iter()
        .filter_map(|(id, script)| script.hotkey.clone().map(|key| (id.clone(), key)))
        .collect();
    for failure in hotkeys.set_scripts(&wanted) {
        model.push_notice(Notice::new(
            Severity::Warning,
            op::SCRIPTS,
            format!(
                "Hotkey \"{}\" for script \"{}\" is disabled: {}",
                failure.key, failure.script_id, failure.error
            ),
        ));
    }
}
