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
use std::time::{Duration, Instant};

use egui::{Context, ViewportCommand};

use crate::app::clipboard_view::{self, ThumbnailRequest};
use crate::app::effect::Effect;
use crate::app::model::loads_stored_history;
use crate::app::model::{AppEvent, AppModel, HotkeyStatus, UpdateRequest, Wake};
use crate::app::notice::{Notice, Severity, op};
use crate::app::{motion, theme};
use crate::core::config::{self, ConfigFile, PaletteSize};
use crate::core::config_edit::{self, ConfigEdit};
use crate::win::clipboard::ClipboardThread;
use crate::win::emoji_render::EmojiRenderer;
use crate::win::file_search::FileSearchWorker;
use crate::win::hotkey::{self, HotkeyManager};
use crate::win::paste;
use crate::win::persistence::{self, ClipboardStoreCommand, StatePaths, StateWriter};
use crate::win::placement::PaletteWindow;
use crate::win::shell::{self, IconTarget, IconWorker};
use crate::win::tray::{self, TrayHandle};
use crate::win::update::{self, UpdateWorker};
use crate::win::{appearance, autostart, file_dialog, join_by};

/// Startup facts decided before the window exists.
pub struct Startup {
    /// The config failed to load and defaults are in use (§10.2).
    pub config_failed: bool,
    /// The window was created transparent for a Mica backdrop (§12.7).
    pub mica: bool,
}

/// How long Quit waits for background threads other than the state writer
/// (§1.5).
const QUIT_GRACE: Duration = Duration::from_secs(2);

/// Frames a fast-path summon may stay cloaked waiting for a stable size; it
/// is uncloaked after this many even if the size is still changing.
const MAX_REVEAL_FRAMES: u8 = 3;

/// The `eframe::App` and main-thread resource owner.
pub struct EframeApp {
    model: AppModel,
    hotkeys: Option<HotkeyManager>,
    tray: Option<TrayHandle>,
    clipboard: Option<ClipboardThread>,
    refresh_worker: Option<JoinHandle<()>>,
    /// A config edit waiting for the refresh worker slot.
    queued_save: Option<ConfigEdit>,
    state_writer: Option<StateWriter>,
    icon_worker: Option<IconWorker>,
    emoji_renderer: Option<EmojiRenderer>,
    file_search: Option<FileSearchWorker>,
    updates: Option<UpdateWorker>,
    /// The `check_for_updates` setting the update worker was last given.
    update_automatic: bool,
    scan_job: Option<JoinHandle<()>>,
    /// Set during shutdown so a running scan stops between items (§1.5 step 5).
    scan_cancel: Arc<AtomicBool>,
    launch_jobs: Vec<JoinHandle<()>>,
    /// Paste finishers and their cancel flags (§8.2 steps 6–8).
    paste_jobs: Vec<(JoinHandle<()>, Arc<AtomicBool>)>,
    /// Clipboard image downscales.
    thumbnail_jobs: Vec<JoinHandle<()>>,
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
    /// Frames painted while the summon fast path keeps the window cloaked,
    /// and whether the last one was at the final size.
    reveal_frames: u8,
    reveal_settled: bool,
    /// Common glyphs were laid out ahead of the first summon.
    glyphs_warmed: bool,
    /// The palette size and look the window and style were last set for.
    applied_look: Option<(PaletteSize, theme::Look)>,
    /// Windows' "Animation effects" and high-contrast switches, re-read
    /// whenever the palette is shown.
    system_look: SystemLook,
    /// Whether the palette was visible at the last step.
    was_visible: bool,
}

#[derive(Clone, Copy)]
struct SystemLook {
    animations: bool,
    high_contrast: bool,
}

impl SystemLook {
    fn read() -> Self {
        Self {
            animations: appearance::animations_enabled(),
            high_contrast: appearance::high_contrast(),
        }
    }
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
        // The style is installed by `sync_look` on the first step. Sizes
        // come from the settings; Ctrl+plus/minus must not zoom egui.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);

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
            hotkey::install_event_handler(wake.clone(), h.routes(), palette_window);
        }

        // Launcher workers (§6): the usage writer and the icon extractor. A
        // failure disables that piece only and is reported.
        let state_writer = match StateWriter::start(&paths, wake.clone()) {
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

        let file_search = FileSearchWorker::start(wake.clone())
            .inspect_err(|e| model.file_search_unavailable(&e.to_string()))
            .ok();
        let update_automatic = model.config().settings.check_for_updates;
        let updates =
            UpdateWorker::start(paths.update_state.clone(), update_automatic, wake.clone())
                .inspect_err(|e| model.updates_unavailable(&e.to_string()))
                .ok();

        // Tray icon + menu (§5.2). Failure keeps the app running but reports a
        // visible notice; the hotkey path still works.
        let tray = match TrayHandle::build() {
            Ok(tray) => {
                tray::install_event_handlers(wake, palette_window);
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
            queued_save: None,
            state_writer,
            icon_worker,
            emoji_renderer,
            file_search,
            updates,
            update_automatic,
            scan_job: None,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            launch_jobs: Vec::new(),
            paste_jobs: Vec::new(),
            thumbnail_jobs: Vec::new(),
            event_tx,
            ctx,
            paths,
            close_sent: false,
            first_frame_seen: false,
            shutting_down,
            palette_window,
            mica: startup.mica,
            frame_theme: None,
            reveal_frames: 0,
            reveal_settled: false,
            glyphs_warmed: false,
            applied_look: None,
            system_look: SystemLook::read(),
            was_visible: false,
        }
    }

    /// Follow the `palette_size`, `text_size` and `reduce_motion` settings
    /// and Windows' animation and contrast switches: restyle, and resize the
    /// window when the palette size changes.
    fn sync_look(&mut self, ctx: &Context) {
        let visible = self.model.is_visible();
        if visible && !self.was_visible {
            self.system_look = SystemLook::read();
        }
        self.was_visible = visible;
        let settings = &self.model.config().settings;
        let look = theme::Look {
            text: settings.text_size,
            reduced_motion: settings.reduce_motion.reduced(self.system_look.animations),
            high_contrast: self.system_look.high_contrast,
        };
        let size = settings.palette_size;
        if self.applied_look == Some((size, look)) {
            return;
        }
        theme::install(ctx, look);
        motion::set_reduced(ctx, look.reduced_motion);
        if self.applied_look.is_none_or(|(applied, _)| applied != size) {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(theme::window_size(size)));
        }
        self.applied_look = Some((size, look));
    }

    /// Per-callback tick: drain events, execute the model's effects, and
    /// trigger quit. Called by both the hidden `logic` pass and the visible
    /// `ui` pass.
    fn step(&mut self, ctx: &Context) {
        self.model.step(ctx);
        self.sync_look(ctx);
        self.reap_jobs();
        self.run_effects();

        if let Some(worker) = &self.updates {
            let automatic = self.model.config().settings.check_for_updates;
            if automatic != self.update_automatic {
                self.update_automatic = automatic;
                worker.set_automatic(automatic);
            }
        }

        if self.model.quit_requested() && !self.close_sent {
            // Send Close exactly once; the actual resource teardown happens in
            // `on_exit` -> `shutdown_resources`.
            self.close_sent = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Join finished workers. Only one refresh and one scan run at a time
    /// (§1.4); shell, thumbnail and paste jobs each deliver one event.
    fn reap_jobs(&mut self) {
        if self
            .refresh_worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
            && let Some(worker) = self.refresh_worker.take()
        {
            let _ = worker.join();
        }
        if self.scan_job.as_ref().is_some_and(|job| job.is_finished())
            && let Some(job) = self.scan_job.take()
        {
            let _ = job.join();
        }
        self.launch_jobs.retain(|job| !job.is_finished());
        self.thumbnail_jobs.retain(|job| !job.is_finished());
        self.paste_jobs.retain(|(job, _)| !job.is_finished());
    }

    /// Execute the model's effects in the order it queued them (§1.2). A
    /// worker that did not start reports back through the model.
    fn run_effects(&mut self) {
        // Reloads and trial keys coalesce: only the latest of each applies.
        let (mut refresh, mut config, mut trial) = (false, None, None);
        for effect in self.model.take_effects() {
            match effect {
                Effect::Scan(generation) => self.start_scan(generation),
                // Shell jobs share `launch_jobs`: each delivers one event and
                // is joined on shutdown.
                Effect::Recycle(paths) => match shell::spawn_recycle(paths, self.wake()) {
                    Ok(job) => self.launch_jobs.push(job),
                    Err(e) => self.model.recycle_start_failed(&e.to_string()),
                },
                Effect::Launch(request) => match shell::spawn_launch(request, self.wake()) {
                    Ok(job) => self.launch_jobs.push(job),
                    Err(e) => self.model.launch_start_failed(&e.to_string()),
                },
                Effect::Icon(request) => {
                    if let Some(worker) = &self.icon_worker {
                        worker.request(
                            IconTarget::App(request.app_id),
                            request.icon_key,
                            request.size,
                        );
                    }
                }
                Effect::FileIcon(path, size) => {
                    if let Some(worker) = &self.icon_worker {
                        worker.request(IconTarget::File(path.clone()), path, size);
                    }
                }
                Effect::Glyph(request) => {
                    if let Some(renderer) = &self.emoji_renderer {
                        renderer.request(request.index, request.text, request.edge);
                    }
                }
                Effect::SaveUsage(generation, bytes) => {
                    if let Some(writer) = &self.state_writer {
                        writer.save_usage(generation, bytes);
                    }
                }
                Effect::SaveFavorites(bytes) => {
                    if let Some(writer) = &self.state_writer {
                        writer.save_favorites(bytes);
                    }
                }
                Effect::SaveHidden(bytes) => {
                    if let Some(writer) = &self.state_writer {
                        writer.save_hidden(bytes);
                    }
                }
                Effect::FileSearch(generation, query) => match &self.file_search {
                    Some(worker) => worker.request(generation, query),
                    None => self
                        .model
                        .file_search_unavailable("the search thread did not start"),
                },
                Effect::Update(request) => match &self.updates {
                    Some(worker) => match request {
                        UpdateRequest::Check => worker.check(),
                        UpdateRequest::Install(release) => worker.install(release),
                        UpdateRequest::Skip(version) => worker.skip(version),
                    },
                    None => self
                        .model
                        .updates_unavailable("the update thread did not start"),
                },
                Effect::Clipboard(command) => {
                    if let Some(clipboard) = &self.clipboard {
                        clipboard.send_command(command);
                    }
                }
                Effect::Store(command) => {
                    if let Some(writer) = &self.state_writer {
                        writer.clipboard(command);
                    }
                }
                Effect::Thumbnail(request) => self.start_thumbnail(request),
                Effect::PasteFinisher(request) => {
                    match paste::spawn_finisher(
                        request.destination,
                        request.cancel.clone(),
                        self.wake(),
                    ) {
                        Ok(job) => self.paste_jobs.push((job, request.cancel)),
                        Err(e) => self.model.paste_finisher_failed(&e.to_string()),
                    }
                }
                // The latest edit wins; it waits for the worker slot below.
                Effect::SaveConfig(edit) => self.queued_save = Some(edit),
                Effect::RefreshConfig => refresh = true,
                Effect::OpenConfigFile => open_config_file(&self.paths.config),
                Effect::PickScriptFile => {
                    let owner = self.palette_window.as_ref().map(PaletteWindow::raw);
                    if let Err(e) = file_dialog::spawn_pick_script(owner, self.wake()) {
                        self.model.script_file_picked(Err(format!(
                            "Could not open the file dialog: {e}"
                        )));
                    }
                }
                Effect::ApplyConfig(cfg) => config = Some(cfg),
                Effect::TrialHotkey(key) => trial = Some(key),
            }
        }

        // A save and a refresh share the worker slot; a save waits for it,
        // and a refresh while it is busy is dropped (the save reloads).
        if self.refresh_worker.is_none()
            && let Some(edit) = self.queued_save.take()
        {
            self.start_save(edit);
        }
        if refresh {
            self.start_refresh();
        }
        if let Some(cfg) = config {
            self.apply_config(cfg);
        }
        if let Some(key) = trial {
            self.apply_trial(key);
        }
    }

    /// The model guarantees at most one active scan.
    fn start_scan(&mut self, generation: u64) {
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

    /// Downscale a stored clipboard image off the UI thread.
    fn start_thumbnail(&mut self, request: ThumbnailRequest) {
        let wake = self.wake();
        let id = request.id;
        let spawned = std::thread::Builder::new()
            .name("smolcast-thumbnail".into())
            .spawn(move || {
                let image = clipboard_view::thumbnail(&request.image);
                wake.send(AppEvent::ThumbnailReady {
                    id: request.id,
                    image,
                });
            });
        match spawned {
            Ok(job) => self.thumbnail_jobs.push(job),
            // No thread: the preview says so; nothing is downscaled on the
            // UI thread.
            Err(e) => {
                self.model.thumbnail_failed(id);
                self.model.push_notice(Notice::new(
                    Severity::Warning,
                    op::CLIPBOARD,
                    format!("Could not start the image preview: {e}"),
                ));
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

    /// Uncloak a fast-path summon once a frame at the final size has been
    /// presented, so the window never composes a stale or resizing surface.
    fn advance_reveal(&mut self, ctx: &Context, resized: bool) {
        let Some(window) = &self.palette_window else {
            return;
        };
        if !window.revealing() {
            self.reveal_frames = 0;
        } else if self.reveal_settled || self.reveal_frames >= MAX_REVEAL_FRAMES {
            window.finish_reveal();
            self.reveal_frames = 0;
            self.reveal_settled = false;
        } else {
            // This frame is presented after `ui` returns; uncloak on the next
            // one. A resize this frame repaints at the new size first. The
            // appear animation waits until it can be seen.
            self.model.hold_appear();
            self.reveal_frames += 1;
            self.reveal_settled = !resized;
            ctx.request_repaint();
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
        // Only the state writer is waited for in full: it holds unsaved
        // state. Every other thread gets `QUIT_GRACE`, so a call that hangs
        // (a WinRT read, a launch on an unreachable share) cannot keep Quit or
        // an update's restart waiting.
        if let Some(clipboard) = self.clipboard.take() {
            clipboard.shutdown(Instant::now() + QUIT_GRACE);
        }
        if let Some(writer) = self.state_writer.take() {
            writer.shutdown();
        }
        // Counted from here, so a slow final save does not use up the others'
        // grace.
        let deadline = Instant::now() + QUIT_GRACE;

        // Scan/icon/launch jobs cancel between items and are joined (§1.5 step 5).
        self.scan_cancel.store(true, Ordering::Release);
        for (_, cancel) in &self.paste_jobs {
            cancel.store(true, Ordering::Release);
        }
        if let Some(job) = self.scan_job.take() {
            join_by(job, Some(deadline));
        }
        if let Some(worker) = self.icon_worker.take() {
            worker.shutdown(deadline);
        }
        if let Some(renderer) = self.emoji_renderer.take() {
            renderer.shutdown(deadline);
        }
        if let Some(worker) = self.file_search.take() {
            worker.shutdown(deadline);
        }
        if let Some(worker) = self.updates.take() {
            worker.shutdown(deadline);
        }
        for job in self.launch_jobs.drain(..) {
            join_by(job, Some(deadline));
        }
        for (job, _) in self.paste_jobs.drain(..) {
            join_by(job, Some(deadline));
        }
        for job in self.thumbnail_jobs.drain(..) {
            join_by(job, Some(deadline));
        }

        if let Some(worker) = self.refresh_worker.take() {
            join_by(worker, Some(deadline));
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
        // Re-hide after eframe's forced first-frame show on a quiet start (§2.1,
        // decision 037: a start by the user keeps it shown). Viewport commands
        // are applied after `post_rendering` in the same frame, so this wins; at
        // most one blank frame can flash.
        if !std::mem::replace(&mut self.first_frame_seen, true) && !self.model.is_visible() {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        // Rasterize common glyphs on the first hidden frame rather than in the
        // first summon's frame.
        if !std::mem::replace(&mut self.glyphs_warmed, true) && !self.model.is_visible() {
            theme::warm_glyphs(ui);
        }
        // The DWM frame (border, Mica tint) follows egui's system-driven theme.
        let theme = ctx.theme();
        if self.frame_theme != Some(theme)
            && let Some(window) = &self.palette_window
        {
            appearance::set_dark(window, theme == egui::Theme::Dark);
            self.frame_theme = Some(theme);
        }
        if self.model.is_painted() {
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
            let base = theme::window_size(self.model.config().settings.palette_size);
            let resized = fit_height(&ctx, content_height, base.y);
            self.advance_reveal(&ctx, resized);
        } else if let Some(window) = &self.palette_window {
            // A fast-path show the model did not follow (it dropped the
            // summon during shutdown, for example) must not stay on screen.
            window.cancel_reveal();
            self.reveal_frames = 0;
            self.reveal_settled = false;
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
        // An installed update starts now; it waits for this process to
        // exit before taking the single-instance mutex (§13).
        if let Some(exe) = self.model.take_relaunch()
            && let Err(e) = update::relaunch(&exe)
        {
            // The window and tray are gone: a message box is the only way
            // left to say why smolcast did not come back.
            update::report_relaunch_failure(&e);
        }
    }
}

/// Grow the palette to fit its content (§12.7): notices and the hotkey
/// recovery UI can exceed the base height. Never shrinks below the base, so
/// typing does not make the window jump, and never exceeds most of the monitor.
/// Returns whether a resize was requested.
fn fit_height(ctx: &Context, wanted: f32, base: f32) -> bool {
    let (current, monitor) = ctx.input(|i| {
        let info = i.viewport();
        (info.inner_rect.map(|r| r.size()), info.monitor_size)
    });
    let Some(current) = current else {
        return false;
    };
    let max = monitor.map_or(f32::INFINITY, |m| m.y * 0.85);
    let height = wanted.ceil().clamp(base, max.max(base));
    let resize = (height - current.y).abs() >= 1.0;
    if resize {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(current.x, height)));
    }
    resize
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
    shell::open_in_notepad(path);
}

/// Put Segoe UI ahead of egui's bundled fonts, which remain as fallbacks for
/// symbols it lacks, and add Windows' icon font as its own family
/// (`theme::ICON_FAMILY`). A missing file keeps that part at its default.
fn install_system_font(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = appearance::system_font() {
        fonts.font_data.insert(
            "segoe-ui".into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "segoe-ui".into());
        }
    }
    // Windows' icon font in a family of its own, never a text fallback.
    if let Some(bytes) = appearance::icon_font() {
        fonts.font_data.insert(
            theme::ICON_FAMILY.into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts.families.insert(
            egui::FontFamily::Name(theme::ICON_FAMILY.into()),
            vec![theme::ICON_FAMILY.into()],
        );
        theme::set_icon_font_installed();
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
