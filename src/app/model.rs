//! Backend-neutral app state, event reduction, and the palette UI.
//!
//! `AppModel::ui` is shaped like `fn ui(&mut self, ctx: &egui::Context)` and
//! must not mention `eframe::Frame`, preserving the raw-winit fallback boundary
//! (`docs/systemPatterns.md` §4). Event handling lives here so the eframe
//! adapter can stay a thin lifecycle shim.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use egui::emath::TSTransform;
use egui::{Color32, Context, Key, Modifiers, RichText, ViewportCommand};

use crate::core::app_index::{AppId, Command};
use crate::core::config::{ClipboardSource, ConfigError, ConfigFile, PaletteSize, Settings};
use crate::core::config_edit::ConfigEdit;
use crate::core::favorites::Favorites;
use crate::core::file_search::FileHit;
use crate::core::history::{Limits, Payload};
use crate::core::motion::{self as timing, Tween};
use crate::core::update::{Update, Version};
use crate::core::usage::Usage;
use crate::win::clipboard::{
    ClipboardCommand, ClipboardError, ClipboardWriteReceipt, WindowsSnapshot,
};
use crate::win::icon::IconRgba;
use crate::win::paste::{
    Destination, PasteOutcome, capture_destination, destination_intact, palette_is_foreground,
    restore_foreground,
};
use crate::win::persistence::{ClipboardStoreCommand, LoadedEntry, Stored};
use crate::win::placement::PaletteWindow;
use crate::win::shell::{IconTarget, LaunchError, ScanOutcome};
use crate::win::update::CheckOutcome;

use super::about_view::AboutView;
use super::clipboard_view::{ClipboardView, PasteRequest, ThumbnailRequest};
use super::emoji_view::{EmojiView, GlyphRequest};
use super::file_view::{FileAction, FileView};
use super::launcher::{IconRequest, LaunchRequest, Launcher};
use super::motion;
use super::notice::{Notice, NoticeStore, Severity, op};
use super::settings_view::SettingsView;
use super::snippet_view::SnippetView;
use super::theme;

/// Typed application events produced by window/background components (§1.2).
/// This is the semantic event set; it grows as later milestones add clipboard,
/// index, launch, and script events.
#[derive(Debug)]
pub enum AppEvent {
    SummonRequested,
    /// A script hotkey fired (§10.3).
    RunScript(String),
    Tray(TrayCommand),
    /// Result of an off-thread config re-read. The last-known-good config stays
    /// active until the summon hotkey has been re-registered successfully.
    ConfigReload {
        result: Result<ConfigFile, ConfigError>,
    },
    /// A form's config.toml write failed before any reload (§12.7, §10.6).
    ConfigSaveFailed(String),
    IndexReady {
        generation: u64,
        result: Result<ScanOutcome, LaunchError>,
    },
    IconReady {
        target: IconTarget,
        /// The key the icon was extracted from; a result for a key the entry
        /// no longer has is stale and dropped.
        icon_key: String,
        /// The pixel edge requested; an icon for an edge the view no longer
        /// draws at (a DPI change in flight) is dropped.
        size: u32,
        result: Result<IconRgba, LaunchError>,
    },
    LaunchFinished {
        app_id: AppId,
        result: Result<(), LaunchError>,
    },
    /// Broken shortcuts were moved to the Recycle Bin; `Ok(false)` when a
    /// prompt was cancelled or some were skipped.
    ShortcutsRecycled {
        result: Result<bool, LaunchError>,
    },
    ClipboardCaptured(Payload),
    /// A stored clipboard image downscaled off the UI thread.
    ThumbnailReady {
        id: u64,
        image: egui::ColorImage,
    },
    /// A rasterized emoji glyph (§9) for table index `index`.
    EmojiGlyph {
        index: usize,
        result: Result<IconRgba, String>,
    },
    /// The clipboard changed while smolcast's own capture was off (§7.0).
    ClipboardChanged,
    /// Stored smolcast history entries, oldest first (§7.2).
    ClipboardHistoryLoaded(Vec<LoadedEntry>),
    WindowsHistory {
        generation: u64,
        result: Result<WindowsSnapshot, ClipboardError>,
    },
    ClipboardWriteFinished {
        request_id: u64,
        result: Result<ClipboardWriteReceipt, ClipboardError>,
    },
    PasteFinished(PasteOutcome),
    Notice(Notice),
    /// Windows Search answered a file query (§6.6).
    FilesFound {
        generation: u64,
        result: Result<Vec<FileHit>, String>,
    },
    /// An update check finished (§13); `manual` when the user asked.
    UpdateChecked {
        manual: bool,
        outcome: CheckOutcome,
    },
    /// A chosen update was swapped in (the exe to relaunch), or not.
    UpdateInstalled(Result<PathBuf, String>),
}

/// Work for the host's update worker (§13).
pub enum UpdateRequest {
    Check,
    Install(Update),
    Skip(Version),
}

/// What the updater is doing, for the root view's update row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateStatus {
    Idle,
    Checking,
    Installing,
}

/// The palette view (§5.4). Every summon opens at `Root`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Root,
    Clipboard,
    Emoji,
    Snippets,
    Settings,
    About,
    Files,
}

/// What a recorded key chord is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordTarget {
    /// A session-only replacement from the hotkey-conflict UI (§5.2).
    Trial,
    /// The summon-hotkey field of the settings form.
    Settings,
}

/// Shared paste service state (§8.2 steps 1–5); steps 6–8 run on the host's
/// finisher thread.
enum PasteState {
    Idle,
    /// Waiting for the clipboard-write receipt; commits are disabled.
    /// `payload` is set for smolcast-history items, which are promoted on success.
    Writing {
        request_id: u64,
        payload: Option<Payload>,
        since: Instant,
    },
}

/// A finisher the host should start: destination plus its cancel flag.
pub struct PasteFinisherRequest {
    pub destination: Destination,
    pub cancel: Arc<AtomicBool>,
}

/// Stored entries are loaded at startup unless persistence is off or the
/// source is always Windows (§7.2).
pub fn loads_stored_history(settings: &Settings) -> bool {
    settings.clipboard_persist && settings.clipboard_source != ClipboardSource::Windows
}

fn history_limits(settings: &Settings) -> Limits {
    Limits {
        max_entries: settings.clipboard_max_entries,
        max_item_bytes: settings.clipboard_max_item_bytes,
        max_total_bytes: settings.clipboard_max_total_bytes,
    }
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
}

/// Summon hotkey registration outcome (§5.2). `Conflict` keeps the palette
/// usable and offers a session-only replacement key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyStatus {
    Active { canonical: String },
    Conflict { requested: String },
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
    /// Give the search field keyboard focus on the next painted frame.
    focus_search: bool,
    view: View,
    launcher: Launcher,
    clipboard: ClipboardView,
    emoji: EmojiView,
    snippets: SnippetView,
    files: FileView,
    paste: PasteState,
    next_request_id: u64,
    /// Cancel flag of the most recent finisher; a new summon sets it.
    paste_cancel: Option<Arc<AtomicBool>>,
    clipboard_commands: Vec<ClipboardCommand>,
    paste_finisher: Option<PasteFinisherRequest>,
    refresh_requested: bool,
    pending_config: Option<ConfigFile>,
    /// The conflict UI is capturing the next key chord as a session-only
    /// summon key; Escape cancels capture instead of hiding.
    recording: bool,
    record_target: RecordTarget,
    capture_hint: Option<String>,
    trial_request: Option<String>,
    quit_requested: bool,
    force_show: bool,
    /// Moved onto the cursor's monitor on every show (§12.7); `None` when the
    /// host could not resolve the window handle.
    palette_window: Option<PaletteWindow>,
    settings: SettingsView,
    about: AboutView,
    /// A form edit to write to config.toml; the host saves, then reloads
    /// (§12.7, §10.6).
    config_save: Option<ConfigEdit>,
    open_config_file: bool,
    /// The palette has held focus since it was shown; losing it afterwards
    /// dismisses the palette (click-away, §12.7).
    had_focus: bool,
    /// When the palette was dismissed with its state kept; `None` after a
    /// completed action, so the next summon starts fresh.
    hidden_at: Option<Instant>,
    /// How present the palette is, from 0 (gone) to 1 (shown): it fades and
    /// settles in on a show, and fades out on a dismiss.
    presence: Tween,
    /// A dismissed palette is fading out; it is hidden when the fade ends.
    /// A summon meanwhile brings it straight back.
    dismissing: bool,
    /// The view and form mode last painted, to animate moving between them.
    scene: Option<(View, bool)>,
    /// The body is sliding in since then, forward (deeper) or back.
    navigating: Option<(Instant, bool)>,
    /// Put the search field's cursor at the end on the next frame (after
    /// the launcher filled in a quicklink's keyword).
    search_cursor_to_end: bool,
    /// A newer release the user can install (§13).
    update: Option<Update>,
    update_status: UpdateStatus,
    update_requests: Vec<UpdateRequest>,
    /// The installed update to start once this process has quit.
    relaunch: Option<PathBuf>,
}

impl AppModel {
    /// `startup_notice` and `force_visible` let a fatal startup/config error
    /// force the palette into view before the first frame (§2.1, §5.3).
    pub fn new(
        receiver: Receiver<AppEvent>,
        config: ConfigFile,
        usage: Stored<Usage>,
        favorites: Stored<Favorites>,
        startup_notices: Vec<Notice>,
        force_visible: bool,
    ) -> Self {
        let mut notices = NoticeStore::new();
        for n in startup_notices {
            notices.push(n);
        }
        let hotkey_status = HotkeyStatus::Active {
            canonical: config.settings.summon_hotkey.clone(),
        };
        let palette_size = config.settings.palette_size;
        let mut model = Self {
            receiver,
            hotkey_status,
            destination: None,
            notices,
            visible: false,
            search: String::new(),
            focus_search: false,
            view: View::Root,
            launcher: Launcher::new(usage, favorites, &config.scripts, &config.quicklinks),
            clipboard: ClipboardView::new(
                history_limits(&config.settings),
                config.settings.clipboard_source,
                config.settings.clipboard_persist,
                loads_stored_history(&config.settings),
            ),
            emoji: EmojiView::new(),
            snippets: SnippetView::new(&config.snippets),
            files: FileView::new(),
            paste: PasteState::Idle,
            next_request_id: 0,
            paste_cancel: None,
            clipboard_commands: Vec::new(),
            paste_finisher: None,
            refresh_requested: false,
            pending_config: None,
            recording: false,
            record_target: RecordTarget::Trial,
            capture_hint: None,
            trial_request: None,
            quit_requested: false,
            force_show: force_visible,
            palette_window: None,
            settings: SettingsView::new(&config.settings),
            about: AboutView::default(),
            config_save: None,
            open_config_file: false,
            had_focus: false,
            hidden_at: None,
            presence: Tween::at(0.0, Instant::now()),
            dismissing: false,
            scene: None,
            navigating: None,
            search_cursor_to_end: false,
            update: None,
            update_status: UpdateStatus::Idle,
            update_requests: Vec::new(),
            relaunch: None,
            config,
        };
        model.set_palette_size(palette_size);
        let settings = &model.config.settings;
        model
            .launcher
            .set_search_engine(settings.search_engine, &settings.web_search_url);
        model
    }

    /// List row counts and the emoji grid follow the window size.
    fn set_palette_size(&mut self, size: PaletteSize) {
        self.launcher.set_palette_size(size);
        self.clipboard.set_palette_size(size);
        self.snippets.set_palette_size(size);
        self.emoji.set_palette_size(size);
        self.files.set_palette_size(size);
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

        // Consumed so the second `step` of the same frame cannot act on it again.
        if self.visible
            && !self.recording
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            if self.view == View::Root {
                self.dismiss();
            } else if !self.view_escape() {
                self.open_view(View::Root);
            }
        }

        // Clicking away dismisses, like any launcher. Only after the palette has
        // actually held focus (the first frames after a show may still report
        // the old state), and not while a paste awaits its write receipt: the
        // receipt reports "focus moved" and the next frame hides.
        if self.visible {
            match ctx.input(|i| i.viewport().focused) {
                Some(true) => self.had_focus = true,
                Some(false) if self.had_focus && matches!(self.paste, PasteState::Idle) => {
                    self.dismiss();
                }
                _ => {}
            }
        }

        // The window `X` hides rather than exits (§1.5): cancel the close and sink.
        // A pending tray Quit is the one close that must go through.
        if !self.quit_requested && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.visible {
                self.dismiss();
            }
        }

        if self.dismissing && self.presence.done(Instant::now()) {
            self.hide(ctx);
        }

        let view_commands = self.clipboard.take_commands();
        self.clipboard_commands.extend(view_commands);
    }

    /// Render the palette. Painting runs through `&mut egui::Ui` because egui
    /// 0.36 panel APIs take a `Ui`, not a `Context`; the model still never
    /// mentions `eframe::Frame`. The host wraps this in the root panel.
    /// Returns the slack padded above the footer (see `palette_ui`).
    pub fn ui(&mut self, ui: &mut egui::Ui) -> f32 {
        let now = Instant::now();
        if !self.presence.done(now) {
            ui.ctx().request_repaint();
        }
        let (scale, opacity) =
            timing::presence_look(self.presence.value(now), motion::reduced(ui.ctx()));
        // Scaled about the window's center. Always wrapped, even at rest, so
        // widget ids (and the search field's focus) stay the same.
        let center = ui.clip_rect().center().to_vec2();
        let transform = TSTransform::from_translation(center)
            * TSTransform::from_scaling(scale)
            * TSTransform::from_translation(-center);
        ui.with_visual_transform(transform, |ui| {
            ui.multiply_opacity(opacity);
            if self.dismissing {
                // Fading out: nothing may be clicked. Keep the fade as is.
                ui.visuals_mut().disabled_alpha = 1.0;
                ui.disable();
            }
            self.palette_ui(ui)
        })
        .inner
    }

    /// Whether the palette is on screen: shown, or fading out.
    pub fn is_painted(&self) -> bool {
        self.visible || self.dismissing
    }

    /// The host keeps a fast-path summon cloaked for its first frames; the
    /// appear animation starts once it is uncloaked.
    pub fn hold_appear(&mut self) {
        if self.visible {
            let now = Instant::now();
            self.presence = Tween::at(0.0, now);
            self.presence.retarget(1.0, timing::APPEAR, now);
        }
    }

    // ---- host-facing accessors -------------------------------------------------

    /// Only the form whose save is in flight takes the message.
    pub fn reduce_config_save_failed(&mut self, message: String) {
        self.settings.save_failed(message.clone());
        self.snippets.save_failed(message);
    }

    /// A form edit to write; the host saves and then reloads.
    pub fn take_config_save(&mut self) -> Option<ConfigEdit> {
        self.config_save.take()
    }

    pub fn take_open_config_file(&mut self) -> bool {
        std::mem::take(&mut self.open_config_file)
    }

    pub fn set_palette_window(&mut self, window: Option<PaletteWindow>) {
        self.palette_window = window;
    }

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

    // ---- launcher requests executed by the host ---------------------------------

    pub fn take_scan_request(&mut self) -> Option<u64> {
        self.launcher.take_scan_request()
    }

    pub fn scan_start_failed(&mut self, generation: u64, error: &str) {
        self.launcher
            .scan_start_failed(generation, &mut self.notices, error);
    }

    pub fn take_recycle_request(&mut self) -> Option<Vec<std::path::PathBuf>> {
        self.launcher.take_recycle_request()
    }

    pub fn recycle_start_failed(&mut self, error: &str) {
        let error = LaunchError {
            operation: "move to Recycle Bin",
            message: error.to_owned(),
            code: None,
        };
        self.launcher
            .on_shortcuts_recycled(Err(error), &mut self.notices);
    }

    pub fn take_launch_request(&mut self) -> Option<LaunchRequest> {
        self.launcher.take_launch_request()
    }

    pub fn launch_start_failed(&mut self, error: &str) {
        self.launcher.launch_start_failed(&mut self.notices, error);
    }

    pub fn take_icon_requests(&mut self) -> Vec<IconRequest> {
        self.launcher.take_icon_requests()
    }

    pub fn take_usage_snapshot(&mut self) -> Option<(u64, Vec<u8>)> {
        self.launcher.take_usage_snapshot()
    }

    pub fn take_favorites_snapshot(&mut self) -> Option<Vec<u8>> {
        self.launcher.take_favorites_snapshot()
    }

    /// A file search for the host's worker (§6.6).
    pub fn take_file_search_request(&mut self) -> Option<(u64, String)> {
        self.files.take_request()
    }

    /// Explorer icons for visible Search Files rows: (path, pixel edge).
    pub fn take_file_icon_requests(&mut self) -> Vec<(String, u32)> {
        self.files.take_icon_requests()
    }

    pub fn file_search_unavailable(&mut self, error: &str) {
        self.files
            .set_unavailable(format!("File search is unavailable: {error}"));
    }

    // ---- updates (§13) -----------------------------------------------------------

    pub fn take_update_requests(&mut self) -> Vec<UpdateRequest> {
        std::mem::take(&mut self.update_requests)
    }

    /// The installed update to start after this process exits.
    pub fn take_relaunch(&mut self) -> Option<PathBuf> {
        self.relaunch.take()
    }

    /// The host has no update worker.
    pub fn updates_unavailable(&mut self, error: &str) {
        self.update_status = UpdateStatus::Idle;
        self.notices.push(Notice::new(
            Severity::Warning,
            op::UPDATE,
            format!("Updates are unavailable: {error}"),
        ));
    }

    fn check_for_updates(&mut self) {
        if self.update_status == UpdateStatus::Idle {
            self.update_status = UpdateStatus::Checking;
            self.update_requests.push(UpdateRequest::Check);
        }
    }

    fn on_update_checked(&mut self, manual: bool, outcome: CheckOutcome) {
        if self.update_status == UpdateStatus::Checking {
            self.update_status = UpdateStatus::Idle;
        }
        match outcome {
            CheckOutcome::Available(update) => self.update = Some(update),
            CheckOutcome::UpToDate => {
                self.update = None;
                if manual {
                    self.notices.push(Notice::new(
                        Severity::Info,
                        op::UPDATE,
                        format!(
                            "smolcast {} is the latest version.",
                            env!("CARGO_PKG_VERSION")
                        ),
                    ));
                }
            }
            // An automatic check fails quietly and is retried later.
            CheckOutcome::Failed(message) if manual => {
                self.notices
                    .push(Notice::new(Severity::Warning, op::UPDATE, message));
            }
            CheckOutcome::Failed(_) => {}
        }
    }

    fn on_update_installed(&mut self, result: Result<PathBuf, String>) {
        self.update_status = UpdateStatus::Idle;
        match result {
            Ok(exe) => {
                self.relaunch = Some(exe);
                self.quit_requested = true;
            }
            Err(message) => self
                .notices
                .push(Notice::new(Severity::Error, op::UPDATE, message)),
        }
    }

    /// The update row under the root list: what is available, and buttons
    /// to install it, read its notes, or skip it.
    fn update_ui(&mut self, ui: &mut egui::Ui) {
        if self.update_status == UpdateStatus::Checking {
            ui.separator();
            theme::busy_small(ui, "Checking for updates…");
            return;
        }
        let Some(update) = &self.update else {
            return;
        };
        let version = &update.version;
        ui.separator();
        if self.update_status == UpdateStatus::Installing {
            theme::busy_small(ui, &format!("Downloading smolcast {version}…"));
            return;
        }
        ui.label(
            RichText::new(format!(
                "smolcast {version} is available (you have {}).",
                env!("CARGO_PKG_VERSION")
            ))
            .small(),
        );
        let (mut install, mut skip) = (false, false);
        ui.horizontal(|ui| {
            install = ui
                .button("Install and Restart")
                .on_hover_text("Downloads the release from GitHub and restarts smolcast")
                .clicked();
            if ui.button("What's New").clicked() {
                crate::win::shell::open_uri(update.page_url.clone());
            }
            skip = ui.button("Skip This Version").clicked();
        });
        if install {
            self.update_status = UpdateStatus::Installing;
            self.update_requests
                .push(UpdateRequest::Install(update.clone()));
        } else if skip {
            self.update_requests
                .push(UpdateRequest::Skip(version.clone()));
            self.update = None;
        }
    }

    // ---- clipboard and paste requests executed by the host ----------------------

    pub fn take_clipboard_commands(&mut self) -> Vec<ClipboardCommand> {
        std::mem::take(&mut self.clipboard_commands)
    }

    pub fn take_glyph_requests(&mut self) -> Vec<GlyphRequest> {
        self.emoji.take_glyph_requests()
    }

    pub fn take_store_commands(&mut self) -> Vec<ClipboardStoreCommand> {
        self.clipboard.take_store_commands()
    }

    pub fn take_thumbnail_requests(&mut self) -> Vec<ThumbnailRequest> {
        self.clipboard.take_thumbnail_requests()
    }

    pub fn clipboard_store_unavailable(&mut self) {
        self.clipboard.store_unavailable();
    }

    pub fn take_paste_finisher(&mut self) -> Option<PasteFinisherRequest> {
        self.paste_finisher.take()
    }

    pub fn paste_finisher_failed(&mut self, error: &str) {
        self.notices.push(Notice::new(
            Severity::Warning,
            op::CLIPBOARD,
            format!("Copied; paste manually (could not start paste: {error})."),
        ));
    }

    fn capture_policy(&self) -> ClipboardCommand {
        ClipboardCommand::SetCapturePolicy {
            source: self.config.settings.clipboard_source,
            capture: self.config.settings.clipboard_capture,
        }
    }

    pub fn config(&self) -> &ConfigFile {
        &self.config
    }

    pub fn summon_hotkey(&self) -> &str {
        &self.config.settings.summon_hotkey
    }

    pub fn hotkey_status(&self) -> &HotkeyStatus {
        &self.hotkey_status
    }

    /// Called by the host after a summon hotkey registered successfully.
    pub fn accept_config(&mut self, cfg: ConfigFile) {
        // An unchanged key was not re-registered: keep the live status, which
        // may name a session-only trial key rather than the configured one.
        let key_changed = cfg.settings.summon_hotkey != self.config.settings.summon_hotkey;
        if key_changed || matches!(self.hotkey_status, HotkeyStatus::Conflict { .. }) {
            self.hotkey_status = HotkeyStatus::Active {
                canonical: cfg.settings.summon_hotkey.clone(),
            };
        }
        self.clipboard.set_limits(history_limits(&cfg.settings));
        self.clipboard.set_setting(cfg.settings.clipboard_source);
        self.launcher.set_scripts(&cfg.scripts);
        self.launcher.set_quicklinks(&cfg.quicklinks);
        self.launcher
            .set_search_engine(cfg.settings.search_engine, &cfg.settings.web_search_url);
        if cfg.settings.palette_size != self.config.settings.palette_size {
            self.set_palette_size(cfg.settings.palette_size);
        }
        self.clipboard.set_persist(cfg.settings.clipboard_persist);
        self.clipboard_commands.push(ClipboardCommand::SetItemCap(
            cfg.settings.clipboard_max_item_bytes,
        ));
        // Both forms see every accepted config; at most one had a save in flight.
        let settings_saved = self.settings.config_accepted(&cfg.settings);
        let snippet_saved = self.snippets.config_accepted(&cfg.snippets);
        let from_form = settings_saved || snippet_saved;
        self.config = cfg;
        self.clipboard_commands.push(self.capture_policy());
        // The forms report their own "Saved."
        if !from_form {
            self.notices.push(Notice::new(
                Severity::Info,
                op::CONFIG,
                "Configuration reloaded.",
            ));
        }
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
        // The reload is not applied while the key conflicts, so a snippet save
        // in flight must not wait on it either.
        self.reduce_config_save_failed(format!(
            "Saved, but {requested} could not be registered; the previous hotkey stays active \
             and the file applies once the hotkey is resolved."
        ));
        self.capture_hint = None;
        self.hotkey_status = HotkeyStatus::Conflict {
            requested: requested.clone(),
        };
        // Already visible (a failed trial or settings save): nothing to force,
        // and a stale flag would undo the next dismissal.
        self.force_show = !self.visible;
        let reason = reason.map(|r| format!(" ({r})")).unwrap_or_default();
        self.notices.push(Notice::new(
            Severity::Error,
            op::SUMMON_HOTKEY,
            format!(
                "Hotkey \"{requested}\" could not be registered{reason}. Open the palette and \
                 apply a temporary key, or edit summon_hotkey = \"{requested}\" in config.toml."
            ),
        ));
    }

    // ---- event reduction --------------------------------------------------------

    fn reduce(&mut self, ctx: &Context, event: AppEvent) {
        match event {
            // A chord smolcast already registered never reaches the window, so
            // while recording say why instead of acting on it.
            AppEvent::SummonRequested if self.visible && self.recording => {
                self.capture_hint = Some("That combination is already the summon hotkey.".into());
            }
            AppEvent::SummonRequested => self.summon(ctx),
            AppEvent::RunScript(_) if self.visible && self.recording => {
                self.capture_hint = Some("That combination is used by a script.".into());
            }
            AppEvent::RunScript(id) => self.launcher.run_script(&id, &mut self.notices),
            AppEvent::Tray(TrayCommand::OpenPalette) => self.summon(ctx),
            AppEvent::Tray(TrayCommand::Refresh) => {
                self.refresh_requested = true;
                self.launcher.request_scan();
            }
            AppEvent::Tray(TrayCommand::Quit) => self.quit_requested = true,
            AppEvent::ConfigReload { result } => match result {
                Ok(cfg) => self.pending_config = Some(cfg),
                Err(e) => {
                    self.reduce_config_save_failed(format!("Reload failed: {e}"));
                    self.notices.push(Notice::new(
                        Severity::Warning,
                        op::CONFIG,
                        format!("Config reload failed: {e}"),
                    ));
                }
            },
            AppEvent::ConfigSaveFailed(message) => self.reduce_config_save_failed(message),
            AppEvent::IndexReady { generation, result } => {
                self.launcher
                    .on_index_ready(generation, result, &mut self.notices);
            }
            AppEvent::IconReady {
                target,
                icon_key,
                size,
                result,
            } => match target {
                IconTarget::App(app_id) => {
                    self.launcher
                        .on_icon_ready(ctx, app_id, &icon_key, size, result);
                }
                IconTarget::File(path) => self.files.on_icon_ready(ctx, path, size, result),
            },
            AppEvent::ShortcutsRecycled { result } => {
                self.launcher
                    .on_shortcuts_recycled(result, &mut self.notices);
            }
            AppEvent::LaunchFinished { app_id, result } => {
                if self
                    .launcher
                    .on_launch_finished(app_id, result, &mut self.notices)
                    && self.visible
                {
                    self.finish(ctx);
                }
            }
            AppEvent::ClipboardCaptured(payload) => self.clipboard.record(payload),
            AppEvent::ThumbnailReady { id, image } => self.clipboard.on_thumbnail(ctx, id, image),
            AppEvent::EmojiGlyph { index, result } => self.emoji.on_glyph(ctx, index, result),
            AppEvent::ClipboardHistoryLoaded(entries) => self.clipboard.on_loaded(entries),
            AppEvent::ClipboardChanged => self
                .clipboard
                .on_clipboard_changed(self.visible && self.view == View::Clipboard),
            AppEvent::WindowsHistory { generation, result } => {
                self.clipboard.on_windows_history(generation, result);
            }
            AppEvent::ClipboardWriteFinished { request_id, result } => {
                self.on_write_finished(ctx, request_id, result);
            }
            AppEvent::PasteFinished(outcome) => self.on_paste_finished(outcome),
            AppEvent::Notice(n) => self.notices.push(n),
            AppEvent::FilesFound { generation, result } => {
                self.files.on_results(generation, result);
            }
            AppEvent::UpdateChecked { manual, outcome } => self.on_update_checked(manual, outcome),
            AppEvent::UpdateInstalled(result) => self.on_update_installed(result),
        }
    }

    // ---- paste service (§8.2) -----------------------------------------------------

    /// Step 1: write the clipboard asynchronously; commits stay disabled until
    /// the receipt arrives.
    fn start_paste(&mut self, request: PasteRequest) {
        if !matches!(self.paste, PasteState::Idle) {
            return;
        }
        self.next_request_id += 1;
        let request_id = self.next_request_id;
        let (command, payload) = match request {
            PasteRequest::Local(payload) => (
                match &payload {
                    Payload::Text(text) => ClipboardCommand::SetText {
                        request_id,
                        text: text.clone(),
                    },
                    Payload::Image(image) => ClipboardCommand::SetImage {
                        request_id,
                        image: image.clone(),
                    },
                },
                Some(payload),
            ),
            PasteRequest::Text(text) => (ClipboardCommand::SetText { request_id, text }, None),
            PasteRequest::Windows { item_id } => (
                ClipboardCommand::WindowsSetContent {
                    request_id,
                    item_id,
                },
                None,
            ),
        };
        self.clipboard_commands.push(command);
        self.paste = PasteState::Writing {
            request_id,
            payload,
            since: Instant::now(),
        };
    }

    /// Steps 2–5. Every failure keeps the palette visible with a notice, and
    /// nothing is injected.
    fn on_write_finished(
        &mut self,
        ctx: &Context,
        request_id: u64,
        result: Result<ClipboardWriteReceipt, ClipboardError>,
    ) {
        let payload = match std::mem::replace(&mut self.paste, PasteState::Idle) {
            PasteState::Writing {
                request_id: pending,
                payload,
                ..
            } if pending == request_id => payload,
            // A stale receipt (Escape or a new summon cancelled the paste).
            other => {
                self.paste = other;
                return;
            }
        };
        if let Err(e) = result {
            self.notices.push(
                Notice::new(
                    Severity::Error,
                    op::CLIPBOARD,
                    format!("Could not copy: {e}"),
                )
                .with_optional_code(e.code),
            );
            return;
        }
        // A pasted smolcast entry becomes the most recent one; our own write is
        // never captured by the listener. Windows reorders its own history.
        if let Some(payload) = payload {
            self.clipboard.record(payload);
        }

        let manual = |reason: &str| {
            Notice::new(
                Severity::Warning,
                op::CLIPBOARD,
                format!("Copied; {reason} — paste manually."),
            )
        };
        let Some(dest) = self.destination else {
            self.notices
                .push(manual("there was no window to paste into"));
            return;
        };
        if !destination_intact(&dest) {
            self.notices.push(manual("destination closed or changed"));
            return;
        }
        if !palette_is_foreground() {
            // The user already focused another window; never steal it back.
            self.notices.push(manual("focus moved to another window"));
            return;
        }
        if !restore_foreground(&dest) {
            self.notices
                .push(manual("Windows refused to switch windows"));
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.paste_cancel = Some(cancel.clone());
        self.paste_finisher = Some(PasteFinisherRequest {
            destination: dest,
            cancel,
        });
        self.finish(ctx);
    }

    /// Steps 6–8 result: failures are stored for the next summon (§5.3).
    fn on_paste_finished(&mut self, outcome: PasteOutcome) {
        let reason = match outcome {
            PasteOutcome::Pasted | PasteOutcome::Cancelled => return,
            PasteOutcome::DestinationChanged => "the destination closed or changed",
            PasteOutcome::FocusMoved => "focus moved to another window",
            PasteOutcome::ForegroundTimeout => "the destination did not come to the front",
            PasteOutcome::ModifiersHeld => "modifier keys stayed held",
            PasteOutcome::InjectionBlocked => {
                "Windows blocked the keystroke (the app may be elevated)"
            }
        };
        self.notices.push(Notice::new(
            Severity::Warning,
            op::CLIPBOARD,
            format!("Copied, but not pasted: {reason}. Paste manually."),
        ));
    }

    fn open_view(&mut self, view: View) {
        // Recording belongs to the view it started in; the Back button can
        // leave without the Escape that would otherwise cancel it.
        if self.record_target == RecordTarget::Settings {
            self.recording = false;
            self.capture_hint = None;
        }
        self.view = view;
        self.search.clear();
        self.focus_search = true;
        match view {
            View::Root => self.launcher.reset_view(),
            View::Clipboard => self.clipboard.open(),
            View::Emoji => self.emoji.open(),
            View::Snippets => self.snippets.open(),
            View::Settings => self.settings.open(),
            View::About => self.about.open(),
            View::Files => self.files.open(),
        }
    }

    /// Summon is idempotent (§5.2): show and focus when hidden, only focus when
    /// already visible, and never replace the captured destination while showing.
    fn summon(&mut self, ctx: &Context) {
        if self.dismissing {
            // Still fading out: come back as it was, with the destination it
            // had (the palette itself is the foreground window now).
            self.show(ctx);
        } else if !self.visible {
            // A new summon cancels any stale paste before capturing a new
            // destination (§8.2).
            if let Some(cancel) = self.paste_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            self.paste = PasteState::Idle;
            self.destination = capture_destination();
            self.launcher.on_summon();
            self.show(ctx);
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    fn show(&mut self, ctx: &Context) {
        let returning = std::mem::take(&mut self.dismissing);
        // Move while still hidden so the window never appears at its old
        // spot; one still fading out stays put.
        if !returning && let Some(window) = &self.palette_window {
            window.place_on_cursor_monitor();
        }
        let now = Instant::now();
        if !returning {
            self.presence = Tween::at(0.0, now);
            self.scene = None;
            motion::new_epoch(ctx);
        }
        self.presence.retarget(1.0, timing::APPEAR, now);
        self.visible = true;
        self.had_focus = false;
        self.focus_search = true;
        // A quick re-summon returns to the view, query, selection, and any
        // draft as they were, so an accidental dismiss loses nothing.
        let keep = Duration::from_secs(self.config.settings.keep_state_secs.into());
        if self.hidden_at.take().is_none_or(|at| at.elapsed() >= keep) {
            self.open_view(View::Root);
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Dismiss with a quick fade, keeping the palette's state for
    /// `keep_state_secs`. `step` hides the window when the fade ends.
    fn dismiss(&mut self) {
        self.visible = false;
        self.dismissing = true;
        self.recording = false;
        // Kept state never includes a half-confirmed Shut Down (§6.5).
        self.launcher.cancel_confirmation();
        // Escape before the receipt cancels injection; the clipboard content,
        // if already written, stays (§8.2).
        self.paste = PasteState::Idle;
        let now = Instant::now();
        self.hidden_at = Some(now);
        self.presence.retarget(0.0, timing::DISMISS, now);
    }

    /// Hide at once, keeping the palette's state for `keep_state_secs`. A
    /// completed action hides without a fade so its window is not kept
    /// waiting (paste-back restores the destination next).
    fn hide(&mut self, ctx: &Context) {
        if !self.dismissing {
            self.dismiss();
        }
        self.dismissing = false;
        self.presence = Tween::at(0.0, Instant::now());
        self.destination = None;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    /// Hide after a completed launch or paste; the next summon starts fresh.
    fn finish(&mut self, ctx: &Context) {
        self.hide(ctx);
        self.hidden_at = None;
    }

    // ---- palette UI -------------------------------------------------------------

    /// Forms own the keyboard: the settings form, and the snippet editor or
    /// delete confirmation.
    fn search_hidden(&self) -> bool {
        match self.view {
            View::Settings | View::About => true,
            View::Snippets => self.snippets.is_editing(),
            View::Root | View::Clipboard | View::Emoji | View::Files => false,
        }
    }

    /// Let the view handle Escape itself (the snippet editor cancels before
    /// the view is left). False means Escape returns to the root.
    fn view_escape(&mut self) -> bool {
        match self.view {
            View::Snippets => self.snippets.escape(),
            View::Root
            | View::Clipboard
            | View::Emoji
            | View::Settings
            | View::About
            | View::Files => false,
        }
    }

    /// Raycast layout: search bar, divider, the view, then a footer action
    /// bar pinned to the bottom. Returns the slack added above the footer so
    /// the host can size the window to the natural content height.
    fn palette_ui(&mut self, ui: &mut egui::Ui) -> f32 {
        ui.set_min_width(440.0);
        let spacing = ui.spacing().item_spacing.y;
        // Sections butt against their dividers; each restores the spacing.
        ui.spacing_mut().item_spacing.y = 0.0;

        let crumb = match self.view {
            View::Root => None,
            View::Clipboard => Some(Command::ClipboardHistory.title()),
            View::Emoji => Some(Command::Emoji.title()),
            View::Snippets => Some(self.snippets.title()),
            View::Settings => Some(Command::Settings.title()),
            View::About => Some(Command::About.title()),
            View::Files => Some(Command::SearchFiles.title()),
        };
        let paste_enabled = matches!(self.paste, PasteState::Idle);
        if self.dismissing {
            // Fading out: paint only.
        } else if self.recording {
            self.capture_chord(ui);
        } else {
            // Backspace on an empty search leaves a sub-view (§5.4); forms
            // keep it for their fields.
            if self.view != View::Root
                && !self.search_hidden()
                && self.search.is_empty()
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace))
            {
                self.open_view(View::Root);
            }
            match self.view {
                View::Root => {
                    // Keys act on rows ranked against the current index, not
                    // one a scan just replaced.
                    self.launcher.refresh(&self.search);
                    self.launcher.handle_keys(ui);
                    if let Some(notice) = self.launcher.take_notice() {
                        self.notices.push(notice);
                    }
                }
                View::Clipboard => self.clipboard.handle_keys(ui, paste_enabled),
                View::Emoji => self.emoji.handle_keys(ui, paste_enabled),
                View::Snippets => self.snippets.handle_keys(ui, paste_enabled),
                View::Settings => self.settings.handle_keys(ui),
                View::About => {}
                View::Files => self.files.handle_keys(ui),
            }
        }
        let (nav_offset, nav_opacity) = self.navigation(ui.ctx());
        let hint = match self.view {
            View::Root => "Search apps and commands…",
            View::Clipboard => "Search clipboard history…",
            View::Emoji => "Search emoji…",
            View::Snippets => "Search snippets…",
            View::Files => "Search file names…",
            // Forms have no search field.
            View::Settings | View::About => "",
        };
        let mut back = false;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(theme::GUTTER, 12))
            .show(ui, |ui| {
                ui.multiply_opacity(nav_opacity);
                // A fixed 28-point line (the back button's height) so the
                // query sits at the same height with or without the button.
                fixed_row(ui, 28.0, |ui| {
                    if crumb.is_some() {
                        back = theme::back_button(ui).on_hover_text("Back (Esc)").clicked();
                    }
                    if self.search_hidden() {
                        self.focus_search = false;
                        ui.label(
                            RichText::new(crumb.unwrap_or_default())
                                .text_style(egui::TextStyle::Heading),
                        );
                    } else {
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text(hint)
                                .font(egui::TextStyle::Heading)
                                .frame(egui::Frame::NONE)
                                .margin(egui::Margin::ZERO)
                                .desired_width(f32::INFINITY),
                        );
                        if std::mem::take(&mut self.focus_search) {
                            search.request_focus();
                        }
                        if std::mem::take(&mut self.search_cursor_to_end)
                            && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), search.id)
                        {
                            let end = egui::text::CCursor::new(self.search.chars().count());
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(end)));
                            state.store(ui.ctx(), search.id);
                        }
                    }
                });
            });
        theme::divider(ui);

        egui::Frame::new()
            .inner_margin(egui::Margin::same(theme::BODY_MARGIN))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = spacing;
                // Paint only: clicks land where the rows will settle.
                let slide = TSTransform::from_translation(egui::vec2(nav_offset, 0.0));
                ui.with_visual_transform(slide, |ui| {
                    ui.multiply_opacity(nav_opacity);
                    self.body_ui(ui, paste_enabled);
                });
            });
        if back {
            self.open_view(View::Root);
        }

        // Pin the footer to the bottom of the window.
        let footer = theme::FOOTER_HEIGHT + 1.0;
        let slack = (ui.available_height() - footer).max(0.0);
        ui.add_space(slack);
        theme::divider(ui);
        self.footer_ui(ui, crumb);
        slack
    }

    /// Slide the body in after moving to another view or form, deeper from
    /// the right and back from the left. Returns its offset and opacity.
    fn navigation(&mut self, ctx: &Context) -> (f32, f32) {
        let depth = |(view, form): (View, bool)| u8::from(view != View::Root) + u8::from(form);
        let scene = (self.view, self.search_hidden());
        if let Some(previous) = self.scene.replace(scene)
            && previous != scene
        {
            self.navigating = Some((Instant::now(), depth(scene) > depth(previous)));
            motion::new_epoch(ctx);
        }
        let Some((since, forward)) = self.navigating else {
            return (0.0, 1.0);
        };
        let duration = timing::NAVIGATE.as_secs_f32();
        let t = (since.elapsed().as_secs_f32() / duration).min(1.0);
        if t < 1.0 {
            ctx.request_repaint();
        } else {
            self.navigating = None;
        }
        timing::navigate_look(t, forward, motion::reduced(ctx))
    }

    fn body_ui(&mut self, ui: &mut egui::Ui, paste_enabled: bool) {
        if self.view == View::Root {
            // A calculation leads with its answer card, as in Raycast.
            self.launcher.refresh(&self.search);
            let section = if self.launcher.has_answer() {
                "Calculator"
            } else if self.launcher.shows_favorites() {
                "Favorites"
            } else if self.search.is_empty() {
                "Suggestions"
            } else {
                "Results"
            };
            // The list is usable while apps are (re)indexed; say so.
            theme::section_header(ui, section, self.launcher.scan_status());
        }
        match self.view {
            View::Root => {
                self.launcher.results_ui(ui, &self.search);
                if let Some(answer) = self.launcher.take_paste_answer() {
                    self.start_paste(PasteRequest::Text(answer));
                }
                if let Some(notice) = self.launcher.take_notice() {
                    self.notices.push(notice);
                }
                // A web search is a completed action: the browser takes over
                // and the next summon starts fresh.
                if let Some(url) = self.launcher.take_open_url() {
                    crate::win::shell::open_uri(url);
                    let ctx = ui.ctx().clone();
                    self.finish(&ctx);
                }
                if let Some(query) = self.launcher.take_open_files() {
                    self.open_view(View::Files);
                    self.search = query;
                    self.search_cursor_to_end = true;
                }
                if let Some(query) = self.launcher.take_set_query() {
                    self.search = query;
                    self.focus_search = true;
                    self.search_cursor_to_end = true;
                }
                if let Some(command) = self.launcher.take_open_command() {
                    match command {
                        Command::ClipboardHistory => self.open_view(View::Clipboard),
                        Command::Emoji => self.open_view(View::Emoji),
                        Command::Snippets => self.open_view(View::Snippets),
                        Command::CreateSnippet => {
                            self.open_view(View::Snippets);
                            self.snippets.start_new();
                        }
                        Command::Settings => self.open_view(View::Settings),
                        Command::About => self.open_view(View::About),
                        Command::SearchFiles => self.open_view(View::Files),
                        Command::CheckForUpdates => {
                            self.search.clear();
                            self.check_for_updates();
                        }
                    }
                }
                theme::inset(ui, |ui| self.launcher.broken_shortcuts_ui(ui));
                if self.search.is_empty() {
                    theme::inset(ui, |ui| self.update_ui(ui));
                }
            }
            View::Clipboard => {
                self.clipboard.ui(ui, &self.search, paste_enabled);
                if let Some(payload) = self.clipboard.take_paste_request() {
                    self.start_paste(payload);
                }
                if self.clipboard.take_open_settings() {
                    crate::win::shell::open_uri("ms-settings:clipboard");
                }
            }
            View::Emoji => {
                self.emoji.ui(ui, &self.search, paste_enabled);
                if let Some(text) = self.emoji.take_paste_request() {
                    self.start_paste(PasteRequest::Text(text.to_owned()));
                }
            }
            View::Snippets => {
                self.snippets.ui(ui, &self.search, paste_enabled);
                if let Some(text) = self.snippets.take_paste_request() {
                    self.start_paste(PasteRequest::Text(text));
                }
                if let Some(edit) = self.snippets.take_save_request() {
                    self.config_save = Some(ConfigEdit::Snippet(edit));
                }
                if let Some(clear_search) = self.snippets.take_left_editor() {
                    if clear_search {
                        self.search.clear();
                    }
                    self.focus_search = true;
                }
                if self.snippets.take_open_file() {
                    self.open_config_file = true;
                }
            }
            View::About => theme::inset(ui, |ui| self.about.ui(ui)),
            View::Files => {
                self.files.ui(ui, &self.search);
                // Opening a file is a completed action: the palette hides
                // and the next summon starts fresh.
                if let Some(action) = self.files.take_action() {
                    match action {
                        FileAction::Open(path) => crate::win::shell::open_uri(path),
                        FileAction::Reveal(path) => crate::win::shell::reveal(path),
                    }
                    let ctx = ui.ctx().clone();
                    self.finish(&ctx);
                }
            }
            View::Settings => {
                let recording = self.recording && self.record_target == RecordTarget::Settings;
                theme::inset(ui, |ui| {
                    self.settings.ui(ui, recording);
                    if recording && let Some(hint) = &self.capture_hint {
                        ui.label(RichText::new(hint).weak().small());
                    }
                });
                if self.settings.take_record_request() {
                    self.recording = true;
                    self.record_target = RecordTarget::Settings;
                    self.capture_hint = None;
                }
                if let Some(settings) = self.settings.take_save_request() {
                    self.config_save = Some(ConfigEdit::Settings(settings));
                }
                if self.settings.take_open_file() {
                    self.open_config_file = true;
                }
            }
        }

        if matches!(self.hotkey_status, HotkeyStatus::Conflict { .. }) {
            ui.add_space(theme::SECTION_GAP);
            theme::inset(ui, |ui| self.hotkey_capture_ui(ui));
        }

        theme::inset(ui, |ui| self.notices_ui(ui));
    }

    /// Raycast's action bar: where you are on the left, the primary action
    /// and its keys on the right.
    fn footer_ui(&mut self, ui: &mut egui::Ui, crumb: Option<&str>) {
        let action: Option<(&str, &[&str])> = match self.view {
            View::Root => Some((self.launcher.primary_action(), &["Enter"])),
            View::Snippets if self.snippets.is_confirming_delete() => Some(("Delete", &["Enter"])),
            View::Snippets if self.snippets.is_editing() => Some(("Save", &["Ctrl", "S"])),
            View::Settings => Some(("Save", &["Ctrl", "S"])),
            View::Clipboard | View::Emoji | View::Snippets => Some(("Paste", &["Enter"])),
            View::Files => Some(("Open", &["Enter"])),
            View::About => None,
        };
        // An action in flight replaces its key hint until it finishes.
        let busy = match (self.view, &self.paste) {
            (_, PasteState::Writing { since, .. }) => Some((*since, "Pasting…")),
            (View::Root, _) => self.launcher.launching_since().map(|t| (t, "Launching…")),
            _ => None,
        }
        .filter(|(since, _)| theme::busy_due(ui, *since))
        .map(|(_, label)| label);
        fixed_row(ui, theme::FOOTER_HEIGHT, |ui| {
            ui.add_space(f32::from(theme::GUTTER));
            ui.label(RichText::new(crumb.unwrap_or("smolcast")).small().weak());
            match &self.hotkey_status {
                HotkeyStatus::Active { canonical } if crumb.is_none() => {
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for key in crate::core::config::key_names(canonical) {
                            theme::key_cap(ui, &key);
                        }
                    });
                }
                HotkeyStatus::Active { .. } => {}
                HotkeyStatus::Conflict { requested } => {
                    ui.label(
                        RichText::new(format!("hotkey conflict: {requested}"))
                            .small()
                            .color(Color32::from_rgb(220, 120, 60)),
                    );
                }
            }
            // Right to left: the rightmost hint comes first.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(f32::from(theme::GUTTER));
                if crumb.is_some() {
                    theme::action_hint(ui, "Back", &["Esc"]);
                    if action.is_some() {
                        ui.label(RichText::new("|").small().color(theme::divider_color(ui)));
                    }
                }
                match (busy, action) {
                    (Some(label), _) => theme::action_busy(ui, label),
                    (None, Some((action, keys))) => theme::action_hint(ui, action, keys),
                    (None, None) => {}
                }
            });
        });
    }

    /// Session-only replacement key (§5.2): record a pressed chord rather than
    /// asking the user to type `global-hotkey` syntax.
    fn hotkey_capture_ui(&mut self, ui: &mut egui::Ui) {
        let recording = self.recording && self.record_target == RecordTarget::Trial;
        ui.horizontal(|ui| {
            let label = if recording {
                "Press a key combination… (Esc cancels)"
            } else {
                "Record temporary key"
            };
            let response = ui.add(egui::Button::new(label).selected(recording));
            if response.clicked() {
                self.recording = !recording;
                self.record_target = RecordTarget::Trial;
                self.capture_hint = None;
                // Nothing may keep keyboard focus while recording: a focused
                // button activates on Space/Enter and a text field would
                // receive the chord's character.
                response.surrender_focus();
                ui.memory_mut(|m| m.stop_text_input());
            }
        });
        if self.record_target == RecordTarget::Trial
            && let Some(hint) = &self.capture_hint
        {
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
            // egui 0.36 reports modifiers as key presses of their own; they
            // only build the chord.
            if is_modifier_key(key) {
                continue;
            }
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
            match self.record_target {
                RecordTarget::Trial => {
                    self.capture_hint = Some(format!("Trying {chord}…"));
                    self.trial_request = Some(chord);
                }
                RecordTarget::Settings => {
                    self.capture_hint = None;
                    self.settings.set_hotkey(chord);
                }
            }
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
                Severity::Warning => theme::WARNING,
                Severity::Error => theme::ERROR,
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

/// A full-width row `height` points tall, its items centered vertically. Its
/// size is fixed, so the rows around it do not move with its contents.
fn fixed_row<R>(
    ui: &mut egui::Ui,
    height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let size = egui::vec2(ui.available_width(), height);
    ui.allocate_ui_with_layout(
        size,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(size);
            add_contents(ui)
        },
    )
    .inner
}

fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
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
