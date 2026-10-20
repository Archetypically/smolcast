//! Launcher state (plan §6): the current index, ranking, selection, bounded
//! icon textures, scan scheduling, and launch/usage bookkeeping.
//!
//! Backend-neutral like `model`: it never starts threads. It raises requests
//! (`take_*`) that the host executes and consumes the resulting `AppEvent`s.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers, RichText, Sense, TextureHandle, TextureOptions, Vec2};

use std::collections::BTreeMap;

use crate::core::app_index::{
    AppId, Command, IndexedApp, LaunchSpec, builtin_commands, script_entries,
};
use crate::core::config::ScriptConfig;
use crate::core::matcher::{Ranked, rank};
use crate::core::usage::Usage;
use crate::win::icon::IconRgba;
use crate::win::persistence::unix_ms_now;
use crate::win::shell::{LaunchError, ScanOutcome};

use super::list;
use super::notice::{Notice, NoticeStore, Severity, op};

/// A summon rescans only when the last completed scan is older than this (§1.4).
const RESCAN_AFTER: Duration = Duration::from_secs(5 * 60);
/// Application icon textures kept alive (§6.4).
const ICON_CACHE_CAP: usize = 128;
/// Result rows shown at once.
const VISIBLE_ROWS: usize = 7;
/// Row icon edge in points.
const ICON_POINTS: f32 = 22.0;

/// A launch the host should start.
pub struct LaunchRequest {
    pub app_id: AppId,
    pub spec: LaunchSpec,
}

/// An icon the host should extract.
pub struct IconRequest {
    pub app_id: AppId,
    pub icon_key: String,
    pub size: u32,
}

#[derive(Default)]
struct IconCache {
    textures: HashMap<AppId, TextureHandle>,
    /// Least recently used at the front.
    recency: VecDeque<AppId>,
    pending: HashSet<AppId>,
    failed: HashSet<AppId>,
}

impl IconCache {
    fn touch(&mut self, id: &AppId) {
        if let Some(pos) = self.recency.iter().position(|x| x == id) {
            let id = self.recency.remove(pos).expect("position is in range");
            self.recency.push_back(id);
        }
    }

    fn insert(&mut self, id: AppId, texture: TextureHandle) {
        self.pending.remove(&id);
        if self.textures.insert(id.clone(), texture).is_none() {
            self.recency.push_back(id);
        } else {
            self.touch(&id);
        }
        while self.recency.len() > ICON_CACHE_CAP {
            // Dropping the handle frees the egui texture.
            if let Some(old) = self.recency.pop_front() {
                self.textures.remove(&old);
            }
        }
    }
}

pub struct Launcher {
    apps: Vec<IndexedApp>,
    usage: Usage,
    usage_generation: u64,

    /// True once any scan delivered apps; before that only commands and
    /// scripts are listed.
    indexed: bool,
    /// Latest scan result; `apps` is this plus commands and scripts.
    scanned: Vec<IndexedApp>,
    scripts: Vec<IndexedApp>,
    ranked: Vec<Ranked>,
    /// Query the current `ranked` was computed for; `None` forces a re-rank.
    ranked_for: Option<String>,
    selected: usize,
    first_row: usize,

    generation: u64,
    scan_active: bool,
    rescan_queued: bool,
    last_scan_finished: Option<Instant>,
    last_skipped: Option<usize>,

    scan_request: Option<u64>,
    launch_request: Option<LaunchRequest>,
    launch_pending: Option<AppId>,
    open_command: Option<Command>,
    icon_requests: Vec<IconRequest>,
    usage_snapshot: Option<(u64, Vec<u8>)>,
    icons: IconCache,
}

impl Launcher {
    /// Starts with the persisted usage and requests the startup scan.
    pub fn new(usage: Usage, scripts: &BTreeMap<String, ScriptConfig>) -> Self {
        let scripts = script_entries(scripts);
        let mut apps = builtin_commands();
        apps.extend(scripts.iter().cloned());
        let mut launcher = Self {
            apps,
            indexed: false,
            scanned: Vec::new(),
            scripts,
            usage,
            usage_generation: 0,
            ranked: Vec::new(),
            ranked_for: None,
            selected: 0,
            first_row: 0,
            generation: 0,
            scan_active: false,
            rescan_queued: false,
            last_scan_finished: None,
            last_skipped: None,
            scan_request: None,
            launch_request: None,
            launch_pending: None,
            open_command: None,
            icon_requests: Vec::new(),
            usage_snapshot: None,
            icons: IconCache::default(),
        };
        launcher.request_scan();
        launcher
    }

    // ---- scheduling -----------------------------------------------------------

    /// Tray Refresh and startup. At most one scan runs; a request during a scan
    /// queues exactly one follow-up.
    pub fn request_scan(&mut self) {
        if self.scan_active {
            self.rescan_queued = true;
            return;
        }
        self.generation += 1;
        self.scan_active = true;
        self.scan_request = Some(self.generation);
    }

    /// Summon rescans only a stale index; no periodic hidden wake exists (§1.4).
    pub fn on_summon(&mut self) {
        if !self.scan_active
            && self
                .last_scan_finished
                .is_some_and(|t| t.elapsed() > RESCAN_AFTER)
        {
            self.request_scan();
        }
    }

    /// The host could not start the scan thread.
    pub fn scan_start_failed(&mut self, generation: u64, notices: &mut NoticeStore, e: &str) {
        if generation == self.generation {
            self.scan_active = false;
            self.last_scan_finished = Some(Instant::now());
        }
        notices.push(Notice::new(
            Severity::Warning,
            op::LAUNCHER,
            format!("Could not start the app scan: {e}"),
        ));
    }

    pub fn on_index_ready(
        &mut self,
        generation: u64,
        result: Result<ScanOutcome, LaunchError>,
        notices: &mut NoticeStore,
    ) {
        // Only the newest issued scan may replace the index.
        if generation != self.generation {
            return;
        }
        self.scan_active = false;
        self.last_scan_finished = Some(Instant::now());
        match result {
            Ok(outcome) => {
                if outcome.skipped_shortcuts > 0
                    && self.last_skipped != Some(outcome.skipped_shortcuts)
                {
                    notices.push(Notice::new(
                        Severity::Info,
                        op::LAUNCHER,
                        format!(
                            "Skipped {} broken Start Menu shortcut(s).",
                            outcome.skipped_shortcuts
                        ),
                    ));
                }
                self.last_skipped = Some(outcome.skipped_shortcuts);
                self.scanned = outcome.apps;
                self.indexed = true;
                self.rebuild();
                // Icons of apps that left the index are no longer requested;
                // the LRU evicts them. Failed lookups are retried next index.
                self.icons.failed.clear();
            }
            Err(e) => notices.push(
                Notice::new(
                    Severity::Warning,
                    op::LAUNCHER,
                    format!("App scan failed: {e}"),
                )
                .with_optional_code(e.code),
            ),
        }
        if std::mem::take(&mut self.rescan_queued) {
            self.request_scan();
        }
    }

    pub fn on_icon_ready(
        &mut self,
        ctx: &egui::Context,
        app_id: AppId,
        result: Result<IconRgba, LaunchError>,
    ) {
        match result {
            Ok(icon) => {
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba_premultiplied,
                );
                let texture = ctx.load_texture("app-icon", image, TextureOptions::LINEAR);
                self.icons.insert(app_id, texture);
            }
            Err(_) => {
                self.icons.pending.remove(&app_id);
                self.icons.failed.insert(app_id);
            }
        }
    }

    /// Returns true when the launch succeeded and the palette should hide.
    pub fn on_launch_finished(
        &mut self,
        app_id: AppId,
        result: Result<(), LaunchError>,
        notices: &mut NoticeStore,
    ) -> bool {
        if self.launch_pending.as_ref() == Some(&app_id) {
            self.launch_pending = None;
        }
        match result {
            Ok(()) => {
                // Usage increments only after the shell accepted the launch (§6.3).
                self.record_usage(&app_id);
                true
            }
            Err(e) => {
                let name = self
                    .apps
                    .iter()
                    .find(|a| a.id == app_id)
                    .map_or("the app", |a| a.display_name.as_str());
                notices.push(
                    Notice::new(
                        Severity::Error,
                        op::LAUNCHER,
                        format!("Could not launch {name}: {e}"),
                    )
                    .with_optional_code(e.code),
                );
                false
            }
        }
    }

    /// Replace the configured scripts after a config reload (§10.3).
    pub fn set_scripts(&mut self, scripts: &BTreeMap<String, ScriptConfig>) {
        self.scripts = script_entries(scripts);
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.apps = self.scanned.clone();
        self.apps.extend(builtin_commands());
        self.apps.extend(self.scripts.iter().cloned());
        self.ranked_for = None;
    }

    /// A script hotkey fired: run it without opening the palette (§10.3).
    pub fn run_script(&mut self, id: &str, notices: &mut NoticeStore) {
        let app_id = AppId::Script { id: id.to_owned() };
        match self.scripts.iter().find(|s| s.id == app_id) {
            Some(script) => {
                self.launch_request = Some(LaunchRequest {
                    app_id,
                    spec: script.launch.clone(),
                });
            }
            None => notices.push(Notice::new(
                Severity::Warning,
                op::LAUNCHER,
                format!("Script \"{id}\" is no longer configured."),
            )),
        }
    }

    fn record_usage(&mut self, app_id: &AppId) {
        self.usage.record_launch(app_id, unix_ms_now());
        self.usage_generation += 1;
        self.usage_snapshot = Some((self.usage_generation, self.usage.encode()));
        self.ranked_for = None;
    }

    pub fn launch_start_failed(&mut self, notices: &mut NoticeStore, e: &str) {
        self.launch_pending = None;
        notices.push(Notice::new(
            Severity::Error,
            op::LAUNCHER,
            format!("Could not start the launch: {e}"),
        ));
    }

    /// Selection returns to the top on every show.
    pub fn reset_view(&mut self) {
        self.selected = 0;
        self.first_row = 0;
        self.ranked_for = None;
    }

    // ---- host requests ----------------------------------------------------------

    pub fn take_scan_request(&mut self) -> Option<u64> {
        self.scan_request.take()
    }

    /// A built-in command the user chose; the model opens its view.
    pub fn take_open_command(&mut self) -> Option<Command> {
        self.open_command.take()
    }

    pub fn take_launch_request(&mut self) -> Option<LaunchRequest> {
        self.launch_request.take()
    }

    pub fn take_icon_requests(&mut self) -> Vec<IconRequest> {
        std::mem::take(&mut self.icon_requests)
    }

    pub fn take_usage_snapshot(&mut self) -> Option<(u64, Vec<u8>)> {
        self.usage_snapshot.take()
    }

    /// The footer's label for Enter on the selected row.
    pub fn primary_action(&self) -> &'static str {
        match self
            .ranked
            .get(self.selected)
            .map(|r| &self.apps[r.index].launch)
        {
            Some(LaunchSpec::Command(_)) => "Open Command",
            Some(LaunchSpec::Script(_)) => "Run Script",
            Some(_) => "Open Application",
            None => "Open",
        }
    }

    // ---- UI ---------------------------------------------------------------------

    /// Consume navigation keys before the search field sees them.
    pub fn handle_keys(&mut self, ui: &mut egui::Ui) {
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        list::step_selection(&mut self.selected, self.ranked.len(), down, up);
        if enter {
            self.commit(self.selected);
        }
    }

    pub fn results_ui(&mut self, ui: &mut egui::Ui, query: &str) {
        if self.ranked_for.as_deref() != Some(query) {
            if self.ranked_for.is_some() {
                // The query changed (not just the index): start from the top.
                self.selected = 0;
                self.first_row = 0;
            }
            self.ranked = rank(query, &self.apps, &self.usage, unix_ms_now());
            self.ranked_for = Some(query.to_owned());
        }
        self.selected = self.selected.min(self.ranked.len().saturating_sub(1));

        if self.ranked.is_empty() {
            let text = if !self.indexed && self.scan_active {
                "Indexing apps…"
            } else {
                "No matches."
            };
            ui.label(RichText::new(text).weak());
            return;
        }

        list::keep_visible(self.selected, &mut self.first_row, VISIBLE_ROWS);
        let end = (self.first_row + VISIBLE_ROWS).min(self.ranked.len());
        let icon_px = (ICON_POINTS * ui.ctx().pixels_per_point()).round() as u32;

        let mut clicked = None;
        for row in self.first_row..end {
            let app = &self.apps[self.ranked[row].index];
            let selected = row == self.selected;
            let icon = self.icons.textures.get(&app.id).cloned();
            if icon.is_some() {
                self.icons.touch(&app.id);
            } else if !app.icon_key.is_empty()
                && !self.icons.pending.contains(&app.id)
                && !self.icons.failed.contains(&app.id)
            {
                self.icons.pending.insert(app.id.clone());
                self.icon_requests.push(IconRequest {
                    app_id: app.id.clone(),
                    icon_key: app.icon_key.clone(),
                    size: icon_px.clamp(16, 256),
                });
            }

            let launching = self.launch_pending.as_ref() == Some(&app.id);
            let leading = |ui: &mut egui::Ui| {
                let size = Vec2::splat(ICON_POINTS);
                match (&icon, &app.launch) {
                    (Some(texture), _) => {
                        ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                    }
                    (None, LaunchSpec::Command(command)) => {
                        ui.add_sized(size, egui::Label::new(command_glyph(*command)));
                    }
                    (None, LaunchSpec::Script(_)) => {
                        ui.add_sized(size, egui::Label::new("▶"));
                    }
                    (None, _) => {
                        ui.allocate_exact_size(size, Sense::hover());
                    }
                }
            };
            // The right column is the entry type (§5.4).
            let kind = if launching {
                "Launching…"
            } else {
                kind_label(&app.launch)
            };
            let trailing = |ui: &mut egui::Ui| {
                ui.label(RichText::new(kind).weak().small());
            };
            if list::row(ui, selected, &app.display_name, leading, trailing).clicked() {
                clicked = Some(row);
            }
        }
        if let Some(row) = clicked {
            self.selected = row;
            self.commit(row);
        }
    }

    fn commit(&mut self, row: usize) {
        if self.launch_pending.is_some() {
            return; // one launch at a time; the result is on its way
        }
        let Some(ranked) = self.ranked.get(row) else {
            return;
        };
        let app = &self.apps[ranked.index];
        if let LaunchSpec::Command(command) = app.launch {
            // Opening a command view counts as a use for ranking.
            let id = app.id.clone();
            self.open_command = Some(command);
            self.record_usage(&id);
            return;
        }
        self.launch_pending = Some(app.id.clone());
        self.launch_request = Some(LaunchRequest {
            app_id: app.id.clone(),
            spec: app.launch.clone(),
        });
    }
}

fn kind_label(launch: &LaunchSpec) -> &'static str {
    match launch {
        LaunchSpec::Shortcut { .. } | LaunchSpec::Packaged { .. } => "Application",
        LaunchSpec::Command(_) => "Command",
        LaunchSpec::Script(_) => "Script",
    }
}

fn command_glyph(command: Command) -> &'static str {
    match command {
        Command::ClipboardHistory => "📋",
        Command::Emoji => "😀",
        Command::Snippets => "📝",
        Command::CreateSnippet => "✏",
        Command::Settings => "⚙",
    }
}
