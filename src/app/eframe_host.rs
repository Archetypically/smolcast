//! Thin `eframe::App` adapter (plan §4): owns the app model plus the main-thread
//! resources (hotkey manager, tray, clipboard thread), wires the producer event
//! handlers, and drives viewport lifecycle. All UI and event reduction live in
//! `model`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use egui::{Context, ViewportCommand};

use crate::app::model::loads_stored_history;
use crate::app::model::{AppEvent, AppModel, HotkeyStatus, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::app::theme;
use crate::core::config::{self, ConfigFile};
use crate::core::config_edit::{self, ConfigEdit};
use crate::win::clipboard::ClipboardThread;
use crate::win::emoji_render::EmojiRenderer;
use crate::win::hotkey::{self, HotkeyManager};
use crate::win::paste;
use crate::win::persistence::{self, ClipboardStoreCommand, StatePaths, StateWriter};
use crate::win::placement::PaletteWindow;
use crate::win::shell::{self, IconWorker};
use crate::win::tray::{self, TrayHandle};
use crate::win::{appearance, autostart};

/// Startup facts decided before the window exists.
pub struct Startup {
    /// The config failed to load and defaults are in use (§10.2).
    pub config_failed: bool,
    /// The window was created transparent for a Mica backdrop (§12.7).
    pub mica: bool,
}

/// Base palette size in points; the height grows to fit content.
pub const PALETTE_SIZE: [f32; 2] = [640.0, 460.0];

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
    palette_window: Option<PaletteWindow>,
    mica: bool,
    /// The theme the DWM frame was last styled for.
    frame_theme: Option<egui::Theme>,
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
        startup: Startup,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let wake = Wake::new(event_tx.clone(), ctx.clone(), shutting_down.clone());

        let mut model = model;
        // Login registration follows a valid startup config only: a malformed
        // file falls back to defaults, which must not remove the entry (§12.7).
        if !startup.config_failed {
            let enabled = model.config().settings.start_at_login;
            sync_autostart(&mut model, enabled);
        }
        let palette_window = PaletteWindow::from_handle(cc);
        model.set_palette_window(palette_window);
        if let Some(window) = &palette_window {
            appearance::style_window(window, startup.mica);
        }
        install_system_font(&ctx);
        theme::install(&ctx);

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
            palette_window,
            mica: startup.mica,
            frame_theme: None,
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

        // A save and a refresh share the worker slot; a save waits for it.
        if self.refresh_worker.is_none()
            && let Some(edit) = self.model.take_config_save()
        {
            self.start_save(edit);
        }
        if self.model.take_refresh_requested() {
            self.start_refresh();
        }
        if self.model.take_open_config_file() {
            open_config_file(&self.paths.config);
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

    /// Write a form edit (settings or a snippet) to config.toml off the UI
    /// thread, then reload it through the normal `ConfigReload` path (§12.7,
    /// §10.6). Only changed keys are rewritten, and an unreadable or invalid
    /// file is never overwritten.
    fn start_save(&mut self, edit: ConfigEdit) {
        let path = self.paths.config.clone();
        let wake = self.wake();
        let spawned = std::thread::Builder::new()
            .name("smolcast-config-save".into())
            .spawn(move || match save_config(&path, &edit) {
                Ok(()) => wake.send(AppEvent::ConfigReload {
                    result: config::load(&path),
                }),
                Err(message) => wake.send(AppEvent::ConfigSaveFailed(message)),
            });
        match spawned {
            Ok(handle) => self.refresh_worker = Some(handle),
            Err(e) => self
                .model
                .reduce_config_save_failed(format!("Could not start saving: {e}")),
        }
    }

    /// Register a valid new summon key before unregistering the old one; on
    /// conflict retain the old registration and report (§5.2).
    fn apply_config(&mut self, cfg: ConfigFile) {
        let new_key = cfg.settings.summon_hotkey.clone();
        let conflicting = matches!(self.model.hotkey_status(), HotkeyStatus::Conflict { .. });
        let changed = new_key != self.model.summon_hotkey();
        let start_at_login = cfg.settings.start_at_login;
        if !changed && !conflicting {
            self.model.accept_config(cfg);
            self.refresh_script_hotkeys();
            sync_autostart(&mut self.model, start_at_login);
            return;
        }
        match self.hotkeys.as_mut().map(|h| h.set_summon(&new_key)) {
            Some(Ok(_)) => {
                self.model.accept_config(cfg);
                self.refresh_script_hotkeys();
                sync_autostart(&mut self.model, start_at_login);
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
        // The DWM frame (border, Mica tint) follows egui's system-driven theme.
        let theme = ctx.theme();
        if self.frame_theme != Some(theme)
            && let Some(window) = &self.palette_window
        {
            appearance::set_dark(window, theme == egui::Theme::Dark);
            self.frame_theme = Some(theme);
        }
        if self.model.is_visible() {
            // DWM draws the rounded corners and border; with Mica the panel is
            // transparent so the backdrop shows through.
            let fill = if self.mica {
                egui::Color32::TRANSPARENT
            } else {
                ui.visuals().panel_fill
            };
            // Sections draw their own margins so dividers span the window.
            let frame = egui::Frame::new().fill(fill);
            let content_height = egui::CentralPanel::default()
                .frame(frame)
                // The panel's own rect always fills the window, and the model
                // pads its footer to the bottom, so measure without that slack.
                .show(ui, |ui| {
                    let inner = ui.vertical(|ui| self.model.ui(ui));
                    inner.response.rect.height() - inner.inner
                })
                .inner;
            fit_height(&ctx, content_height);
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.mica {
            [0.0; 4]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    /// Called once on shutdown, before eframe destroys the viewport. Ordered
    /// cleanup happens here so handlers are gone before the event loop stops.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_resources();
    }
}

/// Grow the palette to fit its content (§12.7): notices and the hotkey
/// recovery UI can exceed the base height. Never shrinks below the base, so
/// typing does not make the window jump, and never exceeds most of the monitor.
fn fit_height(ctx: &Context, wanted: f32) {
    let (current, monitor) = ctx.input(|i| {
        let info = i.viewport();
        (info.inner_rect.map(|r| r.size()), info.monitor_size)
    });
    let Some(current) = current else {
        return;
    };
    let max = monitor.map_or(f32::INFINITY, |m| m.y * 0.85);
    let height = wanted
        .ceil()
        .clamp(PALETTE_SIZE[1], max.max(PALETTE_SIZE[1]));
    if (height - current.y).abs() >= 1.0 {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(current.x, height)));
    }
}

/// Apply a form edit to the file's current text and write it atomically.
/// Content-safe messages only.
fn save_config(path: &Path, edit: &ConfigEdit) -> Result<(), String> {
    let existing = match std::fs::read(path) {
        Ok(bytes) => Some(
            String::from_utf8(bytes)
                .map_err(|_| format!("{} is not UTF-8; not saved.", path.display()))?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
    };
    let text = config_edit::apply(existing.as_deref(), edit, path)
        .map_err(|e| format!("Not saved: {e}"))?;
    persistence::atomic_write(path, text.as_bytes()).map_err(|e| format!("Not saved: {e}"))
}

/// Open config.toml in Notepad, which exists on every Windows install and
/// offers to create the file when it is absent.
fn open_config_file(path: &Path) {
    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
}

/// Put Segoe UI ahead of egui's bundled fonts, which remain as fallbacks for
/// symbols it lacks. Missing file: keep the defaults.
fn install_system_font(ctx: &Context) {
    let Some(bytes) = appearance::system_font() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "segoe-ui".into(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "segoe-ui".into());
    }
    ctx.set_fonts(fonts);
}

/// Reconcile the Run key with `start_at_login`; failures become a notice.
fn sync_autostart(model: &mut AppModel, enabled: bool) {
    if let Err(e) = autostart::reconcile(enabled) {
        model.push_notice(
            Notice::new(
                Severity::Warning,
                op::AUTOSTART,
                format!("Start at login: {e}"),
            )
            .with_optional_code(e.code),
        );
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
