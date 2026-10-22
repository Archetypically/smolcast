//! Launcher state (plan §6): the current index, ranking, selection, bounded
//! icon textures, scan scheduling, and launch/usage bookkeeping.
//!
//! Backend-neutral like `model`: it never starts threads. It raises requests
//! (`take_*`) that the host executes and consumes the resulting `AppEvent`s.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{
    Color32, Key, Modifiers, Pos2, Rect, RichText, Sense, TextureHandle, TextureOptions, Vec2,
};

use std::collections::BTreeMap;

use crate::core::app_index::{
    AppId, Command, IndexedApp, LaunchSpec, SystemAction, builtin_commands, quicklink_entries,
    script_entries,
};
use crate::core::calc::{self, Answer};
use crate::core::config::{PaletteSize, QuicklinkConfig, ScriptConfig, SearchEngine};
use crate::core::favorites::{Favorites, MAX_FAVORITES};
use crate::core::list_cursor::{self, ListCursor};
use crate::core::matcher::{Ranked, rank};
use crate::core::quicklink;
use crate::core::request_cache::RequestCache;
use crate::core::usage::Usage;
use crate::core::web;
use crate::win::icon::IconRgba;
use crate::win::persistence::{Stored, unix_ms_now};
use crate::win::shell::{BrokenShortcut, LaunchError, ScanOutcome};

use super::notice::{Notice, NoticeStore, Severity, op};
use super::theme::Glyph;
use super::{list, theme};

/// A summon rescans only when the last completed scan is older than this (§1.4).
const RESCAN_AFTER: Duration = Duration::from_secs(5 * 60);
/// Application icon textures kept alive (§6.4).
const ICON_CACHE_CAP: usize = 128;
/// Result rows shown at once in a medium palette.
const MEDIUM_ROWS: usize = 7;
/// Row icon edge in points.
const ICON_POINTS: f32 = 22.0;
/// The search glyph: the Search Files command and the file fallback row.
const GLYPH_SEARCH: Glyph = Glyph::new('\u{E721}', "🔍");

/// A row below the ranked entries that hands the query to something else.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Fallback {
    /// Search the web for it.
    Web(String),
    /// Search file names for it (§6.6).
    Files(String),
}

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

pub struct Launcher {
    apps: Vec<IndexedApp>,
    usage: Usage,
    usage_generation: u64,

    /// True once any scan delivered apps; before that only commands and
    /// scripts are listed.
    indexed: bool,
    /// Latest scan result; `apps` is this plus commands, scripts and
    /// quicklinks.
    scanned: Vec<IndexedApp>,
    scripts: Vec<IndexedApp>,
    quicklinks: Vec<IndexedApp>,
    ranked: Vec<Ranked>,
    /// Query the current `ranked` was computed for; `None` forces a re-rank.
    ranked_for: Option<String>,
    cursor: ListCursor,

    generation: u64,
    scan_active: bool,
    rescan_queued: bool,
    last_scan_finished: Option<Instant>,
    /// Broken Start Menu shortcuts from the last scan, offered for removal.
    broken: Vec<BrokenShortcut>,
    /// The broken set the user chose to ignore; the offer returns when the
    /// set changes.
    ignored_broken: Vec<PathBuf>,
    recycle_request: Option<Vec<PathBuf>>,
    recycling: bool,

    scan_request: Option<u64>,
    launch_request: Option<LaunchRequest>,
    /// The app being launched, and since when.
    launch_pending: Option<(AppId, Instant)>,
    open_command: Option<Command>,
    icon_requests: Vec<IconRequest>,
    usage_snapshot: Option<(u64, Vec<u8>)>,
    /// Pinned entries, listed first under "Favorites" on an empty query.
    favorites: Favorites,
    /// False when the file on disk could not be read: changes stay in
    /// memory rather than replace it (`persistence::Stored`).
    usage_writable: bool,
    favorites_writable: bool,
    /// The favorites document to save after a change.
    favorites_snapshot: Option<Vec<u8>>,
    /// How many of `ranked`'s leading entries are favorites (empty query
    /// only; 0 while searching).
    favorite_rows: usize,
    /// A notice for the model to show (favorites full).
    notice: Option<Notice>,
    icons: RequestCache<AppId, TextureHandle>,
    /// Result rows shown at once, from the palette size.
    rows: usize,
    /// The calculator's answer for the query; when present it is row 0,
    /// above the ranked apps.
    answer: Option<Answer>,
    /// The answer the user chose; the model pastes it.
    paste_answer: Option<String>,
    /// The web search fallback's engine, and the template for a custom one.
    search_engine: SearchEngine,
    custom_search_url: String,
    /// Rows after the ranked entries: a web search and a file search for
    /// any query (decision 022).
    fallbacks: Vec<Fallback>,
    /// A search URL the user chose; the model opens it in the browser.
    open_url: Option<String>,
    /// A file search the user chose; the model opens the Files view on it.
    open_files: Option<String>,
    /// The quicklink typed as a keyword (`gh rust`): its index in `apps`
    /// and the query after it. It leads the ranked entries.
    keyword_query: Option<(usize, String)>,
    /// A new root query for the model to put in the search field.
    set_query: Option<String>,
    /// The system action waiting for a second Enter (`SystemAction::confirms`).
    confirming: Option<AppId>,
}

impl Launcher {
    /// Starts with the persisted usage and requests the startup scan.
    pub fn new(
        usage: Stored<Usage>,
        favorites: Stored<Favorites>,
        scripts: &BTreeMap<String, ScriptConfig>,
        quicklinks: &BTreeMap<String, QuicklinkConfig>,
    ) -> Self {
        let scripts = script_entries(scripts);
        let quicklinks = quicklink_entries(quicklinks);
        let mut launcher = Self {
            apps: Vec::new(),
            usage_writable: usage.writable,
            favorites_writable: favorites.writable,
            favorites: favorites.value,
            favorites_snapshot: None,
            favorite_rows: 0,
            notice: None,
            indexed: false,
            scanned: Vec::new(),
            scripts,
            quicklinks,
            usage: usage.value,
            usage_generation: 0,
            ranked: Vec::new(),
            ranked_for: None,
            cursor: ListCursor::default(),
            generation: 0,
            scan_active: false,
            rescan_queued: false,
            last_scan_finished: None,
            broken: Vec::new(),
            ignored_broken: Vec::new(),
            recycle_request: None,
            recycling: false,
            scan_request: None,
            launch_request: None,
            launch_pending: None,
            open_command: None,
            icon_requests: Vec::new(),
            usage_snapshot: None,
            icons: RequestCache::new(ICON_CACHE_CAP),
            rows: MEDIUM_ROWS,
            answer: None,
            paste_answer: None,
            search_engine: SearchEngine::default(),
            custom_search_url: web::DEFAULT_SEARCH_URL.to_owned(),
            fallbacks: Vec::new(),
            open_url: None,
            open_files: None,
            keyword_query: None,
            set_query: None,
            confirming: None,
        };
        // No scan has run yet, so the list is just commands, scripts and
        // quicklinks.
        launcher.rebuild();
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
        // Re-rank even on failure: the web fallback waits on the first scan.
        self.ranked_for = None;
        self.last_scan_finished = Some(Instant::now());
        match result {
            Ok(outcome) => {
                // Offered for removal by `broken_shortcuts_ui`.
                self.broken = outcome.broken_shortcuts;
                self.scanned = outcome.apps;
                self.indexed = true;
                self.rebuild();
                // Icons of apps that left the index are no longer requested;
                // the LRU evicts them. Failed lookups are retried next index.
                self.icons.retry_failed();
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
        icon_key: &str,
        size: u32,
        result: Result<IconRgba, LaunchError>,
    ) {
        // A script's interpreter can change while its icon is extracted; the
        // reissued request for the new key is the one to keep.
        if !self
            .apps
            .iter()
            .any(|a| a.id == app_id && a.icon_key == icon_key)
        {
            // Allow a fresh request; at worst one icon is extracted twice.
            self.icons.cancel(&app_id, size);
            return;
        }
        // Extracted before a DPI change, or no longer wanted.
        if !self.icons.accept(&app_id, size) {
            return;
        }
        match result {
            Ok(icon) => {
                let image = egui::ColorImage::from_rgba_premultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba_premultiplied,
                );
                let texture = ctx.load_texture("app-icon", image, TextureOptions::LINEAR);
                self.icons.insert(app_id, texture);
            }
            Err(_) => self.icons.fail(app_id),
        }
    }

    /// Returns true when the launch succeeded and the palette should hide.
    pub fn on_launch_finished(
        &mut self,
        app_id: AppId,
        result: Result<(), LaunchError>,
        notices: &mut NoticeStore,
    ) -> bool {
        if self
            .launch_pending
            .as_ref()
            .is_some_and(|(id, _)| *id == app_id)
        {
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

    pub fn set_search_engine(&mut self, engine: SearchEngine, custom_url: &str) {
        self.search_engine = engine;
        self.custom_search_url = custom_url.to_owned();
    }

    /// A web search the user chose, to open in the default browser.
    pub fn take_open_url(&mut self) -> Option<String> {
        self.open_url.take()
    }

    /// A file search the user chose from the fallback row.
    pub fn take_open_files(&mut self) -> Option<String> {
        self.open_files.take()
    }

    /// A query to put in the search field: a quicklink's keyword, ready
    /// for its query.
    pub fn take_set_query(&mut self) -> Option<String> {
        self.set_query.take()
    }

    pub fn set_palette_size(&mut self, size: PaletteSize) {
        self.rows = theme::visible_rows(MEDIUM_ROWS, size);
    }

    /// Replace the configured scripts after a config reload (§10.3).
    pub fn set_scripts(&mut self, scripts: &BTreeMap<String, ScriptConfig>) {
        let scripts = script_entries(scripts);
        // An edited interpreter changes the icon: forget only the scripts whose
        // icon key changed (or that are gone), so unrelated saves don't flash.
        // A result still in flight for the old key is dropped on arrival.
        let stale: HashSet<AppId> = self
            .scripts
            .iter()
            .filter(|old| {
                !scripts
                    .iter()
                    .any(|new| new.id == old.id && new.icon_key == old.icon_key)
            })
            .map(|old| old.id.clone())
            .collect();
        self.icons.retain(|id| !stale.contains(id));
        self.scripts = scripts;
        self.rebuild();
    }

    /// Replace the configured quicklinks after a config reload (§10.7).
    pub fn set_quicklinks(&mut self, quicklinks: &BTreeMap<String, QuicklinkConfig>) {
        self.quicklinks = quicklink_entries(quicklinks);
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.apps = self.scanned.clone();
        self.apps.extend(builtin_commands());
        self.apps.extend(self.scripts.iter().cloned());
        self.apps.extend(self.quicklinks.iter().cloned());
        self.ranked_for = None;
    }

    /// A script hotkey fired: run it without opening the palette (§10.3).
    pub fn run_script(&mut self, id: &str, notices: &mut NoticeStore) {
        let app_id = AppId::Script { id: id.to_owned() };
        match self.scripts.iter().find(|s| s.id == app_id) {
            // One request slot: replacing a launch the host has not taken yet
            // would drop it and leave `launch_pending` set for good.
            Some(_) if self.launch_request.is_some() => notices.push(Notice::new(
                Severity::Info,
                op::LAUNCHER,
                format!("Script \"{id}\" did not run: another launch was starting. Try again."),
            )),
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
        if self.usage_writable {
            self.usage_snapshot = Some((self.usage_generation, self.usage.encode()));
        }
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

    /// Forget a system action waiting for its second Enter.
    pub fn cancel_confirmation(&mut self) {
        self.confirming = None;
    }

    /// Selection returns to the top on every show.
    pub fn reset_view(&mut self) {
        self.cursor.reset();
        self.ranked_for = None;
        self.confirming = None;
    }

    // ---- host requests ----------------------------------------------------------

    pub fn take_scan_request(&mut self) -> Option<u64> {
        self.scan_request.take()
    }

    /// A built-in command the user chose; the model opens its view.
    pub fn take_open_command(&mut self) -> Option<Command> {
        self.open_command.take()
    }

    /// Broken shortcuts the user chose to move to the Recycle Bin.
    pub fn take_recycle_request(&mut self) -> Option<Vec<PathBuf>> {
        self.recycle_request.take()
    }

    /// The Recycle Bin move finished; rescan so the index and the offer
    /// reflect what is left.
    pub fn on_shortcuts_recycled(
        &mut self,
        result: Result<bool, LaunchError>,
        notices: &mut NoticeStore,
    ) {
        self.recycling = false;
        match result {
            Ok(true) => {}
            Ok(false) => notices.push(Notice::new(
                Severity::Info,
                op::LAUNCHER,
                "Some broken shortcuts were not removed.",
            )),
            Err(e) => notices.push(
                Notice::new(
                    Severity::Warning,
                    op::LAUNCHER,
                    format!("Could not remove broken shortcuts: {e}"),
                )
                .with_optional_code(e.code),
            ),
        }
        self.request_scan();
    }

    /// The broken-shortcut offer under the root list: what is broken, and
    /// buttons to move those shortcuts to the Recycle Bin or ignore them.
    pub fn broken_shortcuts_ui(&mut self, ui: &mut egui::Ui) {
        let paths: Vec<PathBuf> = self.broken.iter().map(|b| b.path.clone()).collect();
        if paths.is_empty() || paths == self.ignored_broken {
            return;
        }
        ui.separator();
        let count = paths.len();
        let shown = self
            .broken
            .iter()
            .take(3)
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let more = if count > 3 {
            format!(" and {} more", count - 3)
        } else {
            String::new()
        };
        let lead = if count == 1 {
            "1 Start Menu shortcut points to an app that is no longer installed".to_owned()
        } else {
            format!("{count} Start Menu shortcuts point to apps that are no longer installed")
        };
        let detail = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ui.label(
            RichText::new(format!("{lead}: {shown}{more}."))
                .weak()
                .small(),
        )
        .on_hover_text(detail);
        ui.horizontal(|ui| {
            if self.recycling {
                theme::busy(ui, "Moving to Recycle Bin…");
                return;
            }
            if ui
                .button("Move to Recycle Bin")
                .on_hover_text("Restore them from the Recycle Bin if needed")
                .clicked()
            {
                self.recycling = true;
                self.recycle_request = Some(paths.clone());
            }
            if ui.button("Ignore").clicked() {
                self.ignored_broken = paths.clone();
            }
        });
    }

    /// A calculator answer to paste into the previous window.
    pub fn take_paste_answer(&mut self) -> Option<String> {
        self.paste_answer.take()
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

    pub fn take_favorites_snapshot(&mut self) -> Option<Vec<u8>> {
        self.favorites_snapshot.take()
    }

    pub fn take_notice(&mut self) -> Option<Notice> {
        self.notice.take()
    }

    /// Whether the root list leads with a "Favorites" section.
    pub fn shows_favorites(&self) -> bool {
        self.favorite_rows > 0
    }

    // ---- favorites --------------------------------------------------------------

    /// The ranked entry on list row `row`, if that row is one (not the
    /// calculator card or a fallback).
    fn ranked_at(&self, row: usize) -> Option<&Ranked> {
        self.ranked.get(row.checked_sub(self.answer_rows())?)
    }

    /// The app on list row `row`, if that row is an app (not the calculator
    /// card or a fallback).
    fn app_at(&self, row: usize) -> Option<&IndexedApp> {
        self.apps.get(self.ranked_at(row)?.index)
    }

    /// The fallback on list row `row`, if that row is one.
    fn fallback_at(&self, row: usize) -> Option<&Fallback> {
        let index = row.checked_sub(self.answer_rows() + self.ranked.len())?;
        self.fallbacks.get(index)
    }

    /// The query typed after the keyword of the quicklink at `apps[index]`.
    fn keyword_query_for(&self, index: usize) -> Option<&str> {
        self.keyword_query
            .as_ref()
            .filter(|(at, _)| *at == index)
            .map(|(_, query)| query.as_str())
    }

    /// Whether the row is a quicklink that still needs its query typed.
    fn needs_query(&self, row: usize) -> bool {
        let Some(ranked) = self.ranked_at(row) else {
            return false;
        };
        matches!(
            &self.apps[ranked.index].launch,
            LaunchSpec::Quicklink { target } if quicklink::takes_query(target)
        ) && self.keyword_query_for(ranked.index).is_none()
    }

    /// Favorites whose app is in the index right now.
    fn shown_favorites(&self) -> usize {
        self.favorites
            .ids()
            .iter()
            .filter(|id| self.apps.iter().any(|a| &a.id == *id))
            .count()
    }

    fn toggle_favorite(&mut self, row: usize) {
        let Some(id) = self.app_at(row).map(|a| a.id.clone()) else {
            return;
        };
        if self.favorites.contains(&id) {
            self.favorites.remove(&id);
        } else if self.favorites.add(id, self.shown_favorites()).is_err() {
            self.notice = Some(Notice::new(
                Severity::Info,
                op::LAUNCHER,
                format!("Favorites hold up to {MAX_FAVORITES}; remove one first."),
            ));
            return;
        }
        self.favorites_changed();
    }

    /// Move the favorite on `row` one place, keeping it selected.
    fn move_favorite(&mut self, row: usize, up: bool) {
        let Some(id) = self.app_at(row).map(|a| a.id.clone()) else {
            return;
        };
        let apps = &self.apps;
        let shown = |x: &AppId| apps.iter().any(|a| &a.id == x);
        if self.favorites.shift(&id, up, shown) {
            self.cursor.selected = if up { row.saturating_sub(1) } else { row + 1 };
            self.favorites_changed();
        }
    }

    fn favorites_changed(&mut self) {
        if self.favorites_writable {
            self.favorites_snapshot = Some(self.favorites.encode());
        }
        // Re-rank in place: the query is unchanged, so keep the selection.
        let query = self.ranked_for.clone().unwrap_or_default();
        self.rank_now(&query);
    }

    /// What a running scan is doing, for the section header.
    pub fn scan_status(&self) -> Option<&'static str> {
        match (self.scan_active, self.indexed) {
            (false, _) => None,
            (true, false) => Some("Indexing apps…"),
            (true, true) => Some("Refreshing apps…"),
        }
    }

    /// When the launch the shell has not answered yet started.
    pub fn launching_since(&self) -> Option<Instant> {
        self.launch_pending.as_ref().map(|(_, since)| *since)
    }

    /// The footer's label for Enter on the selected row.
    pub fn primary_action(&self) -> &'static str {
        if self.answer.is_some() && self.cursor.selected == 0 {
            return "Paste Answer";
        }
        match self.fallback_at(self.cursor.selected) {
            Some(Fallback::Web(_)) => return "Search Web",
            Some(Fallback::Files(_)) => return "Search Files",
            None => {}
        }
        if self.needs_query(self.cursor.selected) {
            return "Type Query";
        }
        let Some(app) = self.app_at(self.cursor.selected) else {
            return "Open";
        };
        match &app.launch {
            LaunchSpec::Command(_) => "Open Command",
            LaunchSpec::Script(_) => "Run Script",
            LaunchSpec::System(_) if self.confirming.as_ref() == Some(&app.id) => "Confirm",
            LaunchSpec::System(_) => "Run Command",
            LaunchSpec::Quicklink { .. } => "Open Quicklink",
            LaunchSpec::Shortcut { .. }
            | LaunchSpec::Packaged { .. }
            | LaunchSpec::Executable { .. } => "Open Application",
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
        let count = self.row_count();
        self.cursor.step(count, down, up);
        if down || up {
            self.confirming = None;
        }
        if enter {
            self.commit(self.cursor.selected);
        }
        // Tab completes a quicklink's keyword so its query can be typed.
        if self.needs_query(self.cursor.selected)
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab))
        {
            self.commit(self.cursor.selected);
        }
        // Raycast's favorite shortcuts, with Ctrl for Cmd.
        let shift_ctrl = Modifiers::CTRL | Modifiers::SHIFT;
        let (toggle, move_up, move_down) = ui.input_mut(|i| {
            (
                i.consume_key(shift_ctrl, Key::F),
                i.consume_key(shift_ctrl, Key::ArrowUp),
                i.consume_key(shift_ctrl, Key::ArrowDown),
            )
        });
        if toggle {
            self.toggle_favorite(self.cursor.selected);
        }
        let on_favorite = self.cursor.selected >= self.answer_rows()
            && self.cursor.selected - self.answer_rows() < self.favorite_rows;
        if on_favorite && (move_up || move_down) {
            self.move_favorite(self.cursor.selected, move_up);
        }
    }

    /// 1 when a calculator answer occupies row 0.
    fn answer_rows(&self) -> usize {
        usize::from(self.answer.is_some())
    }

    fn row_count(&self) -> usize {
        self.answer_rows() + self.list_len()
    }

    /// Rows in the scrolling list: the ranked entries, then the fallbacks.
    fn list_len(&self) -> usize {
        self.ranked.len() + self.fallbacks.len()
    }

    /// Rank apps and evaluate the calculator for `query`, once per change.
    /// The model calls it before drawing the section header, which depends
    /// on whether there is an answer.
    pub fn refresh(&mut self, query: &str) {
        if self.ranked_for.as_deref() != Some(query) {
            if self.ranked_for.is_some() {
                // The query changed (not just the index): start from the top.
                self.cursor.reset();
                self.confirming = None;
            }
            self.rank_now(query);
        }
    }

    fn rank_now(&mut self, query: &str) {
        self.ranked = rank(query, &self.apps, &self.usage, unix_ms_now());
        let trimmed = query.trim();
        // An empty query leads with the favorites, in the user's order.
        self.favorite_rows = 0;
        if trimmed.is_empty() {
            let mut leading = Vec::new();
            for id in self.favorites.ids() {
                if let Some(at) = self
                    .ranked
                    .iter()
                    .position(|r| &self.apps[r.index].id == id)
                {
                    leading.push(self.ranked.remove(at));
                }
            }
            self.favorite_rows = leading.len();
            leading.append(&mut self.ranked);
            self.ranked = leading;
        }
        // A quicklink's keyword followed by a query (`gh rust egui`) puts
        // that quicklink first, ready to open with the query.
        self.keyword_query = quicklink::split_keyword(trimmed).and_then(|(keyword, rest)| {
            let index = self.apps.iter().position(|app| {
                matches!(&app.id, AppId::Quicklink { id } if id.eq_ignore_ascii_case(keyword))
                    && matches!(&app.launch, LaunchSpec::Quicklink { target }
                        if quicklink::takes_query(target))
            })?;
            Some((index, rest.to_owned()))
        });
        if let Some((index, _)) = &self.keyword_query {
            let ranked = match self.ranked.iter().position(|r| r.index == *index) {
                Some(at) => self.ranked.remove(at),
                None => Ranked {
                    index: *index,
                    fuzzy_score: 0,
                    combined_score: 0,
                },
            };
            self.ranked.insert(0, ranked);
        }
        self.answer = calc::evaluate(query);
        // Fallbacks, like Raycast's: the web and file names, after the
        // results of any query that is not a calculation or a quicklink's
        // (decision 022). Not during the first scan, when the app may
        // simply not be indexed yet.
        let first_scan = !self.indexed && self.scan_active;
        self.fallbacks.clear();
        if !trimmed.is_empty()
            && !first_scan
            && self.answer.is_none()
            && self.keyword_query.is_none()
        {
            self.fallbacks.push(Fallback::Web(trimmed.to_owned()));
            self.fallbacks.push(Fallback::Files(trimmed.to_owned()));
        }
        self.ranked_for = Some(query.to_owned());
    }

    pub fn has_answer(&self) -> bool {
        self.answer.is_some()
    }

    pub fn results_ui(&mut self, ui: &mut egui::Ui, query: &str) {
        self.refresh(query);
        let total = self.row_count();
        self.cursor.clamp_selected(total);

        if total == 0 {
            // The header already shows a running scan.
            let text = if !self.indexed && self.scan_active {
                "No matches yet."
            } else {
                "No matches."
            };
            theme::inset(ui, |ui| ui.label(RichText::new(text).weak()));
            return;
        }

        // The answer card stays put above the list, which scrolls in the
        // rows the card and the "Results" header leave.
        let offset = self.answer_rows();
        let list_len = self.list_len();
        // Favorites and the other suggestions are split by a "Suggestions"
        // header, which takes one row.
        let split = self.favorite_rows > 0 && self.ranked.len() > self.favorite_rows;
        // Fallbacks after results sit under their own header, which also
        // takes a row. With no results they are the results: no header.
        let fallback_header = !self.ranked.is_empty() && !self.fallbacks.is_empty();
        let list_rows = if offset == 1 {
            self.rows.saturating_sub(CARD_ROWS).max(1)
        } else if split || fallback_header {
            self.rows.saturating_sub(1).max(1)
        } else {
            self.rows
        };
        let step = list::wheel_rows(ui, theme::ROW_STRIDE, &mut self.cursor.wheel);
        if self.cursor.selected >= offset {
            let row = list_cursor::scroll_window(
                &mut self.cursor.first_row,
                self.cursor.selected - offset,
                list_len,
                list_rows,
                step,
            );
            self.cursor.selected = row + offset;
            list_cursor::keep_visible(
                self.cursor.selected - offset,
                &mut self.cursor.first_row,
                list_rows,
            );
        } else {
            // The calculator card stays selected; only the list scrolls.
            list_cursor::scroll_window(
                &mut self.cursor.first_row,
                self.cursor.selected,
                list_len,
                list_rows,
                step,
            );
        }
        let end = (self.cursor.first_row + list_rows).min(list_len);
        let icon_px = (ICON_POINTS * ui.ctx().pixels_per_point()).round() as u32;
        self.icons.set_edge(list::icon_request_edge(icon_px));

        let mut clicked = None;
        if let Some(answer) = &self.answer {
            if answer_card(ui, answer, self.cursor.selected == 0).clicked() {
                clicked = Some(0);
            }
            if list_len > 0 {
                theme::section_header(ui, "Results", None);
            }
        }
        let mut menu = None;
        for row in self.cursor.first_row + offset..end + offset {
            if split && row - offset == self.favorite_rows {
                theme::section_header(ui, "Suggestions", None);
            }
            if fallback_header && row - offset == self.ranked.len() {
                theme::section_header(ui, &fallback_title(query), None);
            }
            let selected = row == self.cursor.selected;
            if let Some(fallback) = self.fallback_at(row) {
                if self.fallback_row(ui, fallback, selected).clicked() {
                    clicked = Some(row);
                }
                continue;
            }
            let index = self.ranked[row - offset].index;
            let app = &self.apps[index];
            let favorite = self.favorites.contains(&app.id);
            let in_favorites = row - offset < self.favorite_rows;
            let icon = self.icons.get(&app.id);
            if icon.is_none() && !app.icon_key.is_empty() && self.icons.begin(&app.id) {
                self.icon_requests.push(IconRequest {
                    app_id: app.id.clone(),
                    icon_key: app.icon_key.clone(),
                    size: self.icons.edge(),
                });
            }

            let launching = self
                .launch_pending
                .as_ref()
                .is_some_and(|(id, _)| *id == app.id);
            let confirming = self.confirming.as_ref() == Some(&app.id);
            let leading = |ui: &mut egui::Ui| {
                let size = Vec2::splat(ICON_POINTS);
                match (&icon, &app.launch) {
                    (Some(texture), _) => list::icon(ui, texture, ICON_POINTS),
                    (None, LaunchSpec::Command(command)) => {
                        theme::glyph(ui, command_glyph(*command), ICON_POINTS);
                    }
                    (None, LaunchSpec::System(action)) => {
                        theme::glyph(ui, system_glyph(*action), ICON_POINTS);
                    }
                    (None, LaunchSpec::Quicklink { .. }) => {
                        theme::glyph(ui, Glyph::new('\u{E71B}', "🔗"), ICON_POINTS);
                    }
                    (None, LaunchSpec::Script(_)) => {
                        theme::glyph(ui, Glyph::new('\u{E768}', "▶"), ICON_POINTS);
                    }
                    (None, _) => {
                        ui.allocate_exact_size(size, Sense::hover());
                    }
                }
            };
            // The right column is the entry type (§5.4); while searching, a
            // favorite says so, since it is not in its own section then. A
            // system action waiting for its second Enter says that instead.
            let kind = if launching {
                RichText::new("Launching…").weak()
            } else if confirming {
                RichText::new("Press Enter again to confirm").color(theme::WARNING)
            } else if favorite && self.favorite_rows == 0 {
                RichText::new(format!("Favorite · {}", kind_label(&app.launch))).weak()
            } else {
                RichText::new(kind_label(&app.launch)).weak()
            };
            let trailing = |ui: &mut egui::Ui| {
                ui.label(kind.small());
            };
            // A quicklink typed as a keyword shows the query it will open.
            let name = match self.keyword_query_for(index) {
                Some(query) => format!("{} \u{201C}{query}\u{201D}", app.display_name),
                None => app.display_name.clone(),
            };
            let response = list::row(ui, selected, &name, leading, trailing);
            if response.clicked() {
                clicked = Some(row);
            }
            response.context_menu(|ui| {
                let toggle = if favorite {
                    "Remove from Favorites"
                } else {
                    "Add to Favorites"
                };
                let shortcut = "Ctrl+Shift+F";
                if ui
                    .add(egui::Button::new(toggle).shortcut_text(shortcut))
                    .clicked()
                {
                    menu = Some((row, FavoriteAction::Toggle));
                }
                if in_favorites {
                    let up = egui::Button::new("Move Up").shortcut_text("Ctrl+Shift+Up");
                    if ui.add(up).clicked() {
                        menu = Some((row, FavoriteAction::Up));
                    }
                    let down = egui::Button::new("Move Down").shortcut_text("Ctrl+Shift+Down");
                    if ui.add(down).clicked() {
                        menu = Some((row, FavoriteAction::Down));
                    }
                }
            });
        }
        if let Some((row, action)) = menu {
            self.cursor.selected = row;
            match action {
                FavoriteAction::Toggle => self.toggle_favorite(row),
                FavoriteAction::Up => self.move_favorite(row, true),
                FavoriteAction::Down => self.move_favorite(row, false),
            }
        } else if let Some(row) = clicked {
            if row != self.cursor.selected {
                self.confirming = None;
            }
            self.cursor.selected = row;
            self.commit(row);
        }
    }

    /// A web or file search row for the typed query.
    fn fallback_row(
        &self,
        ui: &mut egui::Ui,
        fallback: &Fallback,
        selected: bool,
    ) -> egui::Response {
        let (name, kind) = match fallback {
            Fallback::Web(query) => {
                let name = match self.search_engine {
                    SearchEngine::Custom => format!("Search the web for \u{201C}{query}\u{201D}"),
                    engine => format!("Search {} for \u{201C}{query}\u{201D}", engine.name()),
                };
                (name, "Web Search")
            }
            Fallback::Files(query) => {
                (format!("Search Files for \u{201C}{query}\u{201D}"), "Files")
            }
        };
        let engine = self.search_engine;
        let leading = |ui: &mut egui::Ui| match fallback {
            Fallback::Web(_) => {
                theme::engine_badge(ui, engine, ICON_POINTS);
            }
            Fallback::Files(_) => {
                theme::glyph(ui, GLYPH_SEARCH, ICON_POINTS);
            }
        };
        let trailing = |ui: &mut egui::Ui| {
            ui.label(RichText::new(kind).weak().small());
        };
        list::row(ui, selected, &name, leading, trailing)
    }

    fn commit(&mut self, row: usize) {
        if self.launch_pending.is_some() {
            return; // one launch at a time; the result is on its way
        }
        if let Some(answer) = &self.answer
            && row == 0
        {
            self.paste_answer = Some(answer.result.clone());
            return;
        }
        match self.fallback_at(row) {
            Some(Fallback::Web(query)) => {
                let template = self.search_engine.template_or(&self.custom_search_url);
                self.open_url = Some(web::search_url(template, query));
                return;
            }
            Some(Fallback::Files(query)) => {
                self.open_files = Some(query.clone());
                return;
            }
            None => {}
        }
        let Some(index) = self.ranked_at(row).map(|ranked| ranked.index) else {
            return;
        };
        let app = &self.apps[index];
        let (app_id, mut spec) = (app.id.clone(), app.launch.clone());
        match &spec {
            LaunchSpec::Command(command) => {
                // Opening a command view counts as a use for ranking.
                self.open_command = Some(*command);
                self.record_usage(&app_id);
                return;
            }
            LaunchSpec::System(action)
                if action.confirms() && self.confirming.as_ref() != Some(&app_id) =>
            {
                self.confirming = Some(app_id);
                return;
            }
            LaunchSpec::Quicklink { target } if quicklink::takes_query(target) => {
                let Some(query) = self.keyword_query_for(index) else {
                    // Put the keyword in the search field; the query comes next.
                    if let AppId::Quicklink { id } = &app_id {
                        self.set_query = Some(format!("{id} "));
                    }
                    return;
                };
                spec = LaunchSpec::Quicklink {
                    target: quicklink::expand(target, query),
                };
            }
            _ => {}
        }
        self.confirming = None;
        self.launch_pending = Some((app_id.clone(), Instant::now()));
        self.launch_request = Some(LaunchRequest { app_id, spec });
    }
}

/// The fallback section's header: `Search for "query"`, the query cut
/// with "…" so the header stays one line.
fn fallback_title(query: &str) -> String {
    const MAX_CHARS: usize = 40;
    let query = query.trim();
    let shown: String = query.chars().take(MAX_CHARS).collect();
    let ellipsis = if query.chars().count() > MAX_CHARS {
        "…"
    } else {
        ""
    };
    format!("Search for \u{201C}{shown}{ellipsis}\u{201D}")
}

/// A favorites change chosen from a row's context menu.
enum FavoriteAction {
    Toggle,
    Up,
    Down,
}

/// List rows the calculator card and the "Results" header below it replace.
const CARD_ROWS: usize = 3;
/// The card fills those rows exactly, so the window keeps its height.
const CARD_HEIGHT: f32 =
    CARD_ROWS as f32 * theme::ROW_STRIDE - theme::SECTION_HEADER_HEIGHT - theme::SPACING;
/// Space each side of the arrow between the card's halves.
const CARD_ARROW_GAP: f32 = 24.0;

/// Raycast's calculator card: the input and the result side by side in
/// large text, each over a gray unit caption, with an arrow between them.
fn answer_card(ui: &mut egui::Ui, answer: &Answer, selected: bool) -> egui::Response {
    let size = Vec2::new(ui.available_width(), CARD_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let widgets = &ui.visuals().widgets;
    let fill = if selected {
        theme::selected_fill(ui)
    } else if response.hovered() {
        widgets.hovered.bg_fill
    } else {
        widgets.inactive.bg_fill
    };
    let caption_fill = widgets.inactive.bg_fill;
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let heading = egui::TextStyle::Heading.resolve(ui.style()).size;
    let big = egui::FontId::proportional((heading * 1.4).round());
    let small = egui::TextStyle::Small.resolve(ui.style());

    let painter = ui.painter();
    painter.rect_filled(rect, 8, fill);
    let half = rect.width() / 2.0;
    let sides = [
        (&answer.input, answer.input_label),
        (&answer.result, answer.result_label),
    ];
    for (i, (value, label)) in sides.into_iter().enumerate() {
        let side = Rect::from_min_size(
            rect.min + Vec2::new(i as f32 * half, 0.0),
            Vec2::new(half, rect.height()),
        )
        .shrink2(Vec2::new(CARD_ARROW_GAP, 0.0));
        let value = one_line(ui, value, big.clone(), text, side.width());
        let caption = one_line(ui, label, small.clone(), weak, side.width() - 12.0);
        let pill = caption.size() + Vec2::new(12.0, 4.0);
        let gap = 6.0;
        let top = side.center().y - (value.size().y + gap + pill.y) / 2.0;
        let value_x = side.center().x - value.size().x / 2.0;
        let pill_rect = Rect::from_min_size(
            Pos2::new(side.center().x - pill.x / 2.0, top + value.size().y + gap),
            pill,
        );
        painter.galley(Pos2::new(value_x, top), value, text);
        if !label.is_empty() {
            painter.rect_filled(pill_rect, 4, caption_fill);
            painter.galley(pill_rect.center() - caption.size() / 2.0, caption, weak);
        }
    }

    // A painted arrow, so it does not depend on the font having one.
    let c = rect.center();
    let stroke = egui::Stroke::new(1.5, weak);
    painter.line_segment([c + Vec2::new(-6.0, 0.0), c + Vec2::new(6.0, 0.0)], stroke);
    painter.line_segment([c + Vec2::new(6.0, 0.0), c + Vec2::new(2.0, -4.0)], stroke);
    painter.line_segment([c + Vec2::new(6.0, 0.0), c + Vec2::new(2.0, 4.0)], stroke);
    response
}

/// `text` laid out on one line, cut with "…" past `width`.
fn one_line(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

fn kind_label(launch: &LaunchSpec) -> &'static str {
    match launch {
        LaunchSpec::Shortcut { .. }
        | LaunchSpec::Packaged { .. }
        | LaunchSpec::Executable { .. } => "Application",
        LaunchSpec::Command(_) => "Command",
        LaunchSpec::Script(_) => "Script",
        LaunchSpec::System(_) => "System",
        LaunchSpec::Quicklink { .. } => "Quicklink",
    }
}

/// Windows icon-font glyphs (Segoe Fluent Icons / MDL2 codepoints), as the
/// Start menu and Settings use them.
fn command_glyph(command: Command) -> Glyph {
    match command {
        Command::ClipboardHistory => Glyph::new('\u{E77F}', "📋"), // Paste
        Command::Emoji => Glyph::new('\u{E76E}', "😀"),            // Emoji2
        Command::Snippets => Glyph::new('\u{E70B}', "📝"),         // QuickNote
        Command::CreateSnippet => Glyph::new('\u{E70F}', "✏"),     // Edit
        Command::Settings => Glyph::new('\u{E713}', "⚙"),          // Settings
        Command::About => Glyph::new('\u{E946}', "ℹ"),             // Info
        Command::SearchFiles => GLYPH_SEARCH,
        Command::CheckForUpdates => Glyph::new('\u{E895}', "🔄"), // Sync
    }
}

fn system_glyph(action: SystemAction) -> Glyph {
    match action {
        SystemAction::LockScreen => Glyph::new('\u{E72E}', "🔒"), // Lock
        SystemAction::Sleep => Glyph::new('\u{E708}', "💤"),      // QuietHours (moon)
        SystemAction::Restart => Glyph::new('\u{E777}', "🔃"),    // UpdateRestore
        SystemAction::ShutDown => Glyph::new('\u{E7E8}', "🔌"),   // PowerButton
        SystemAction::SignOut => Glyph::new('\u{E748}', "🚪"),    // SwitchUser
        SystemAction::EmptyRecycleBin => Glyph::new('\u{E74D}', "🗑"), // Delete
        SystemAction::ToggleDarkMode => Glyph::new('\u{E706}', "🌗"), // Brightness
        SystemAction::ToggleMute => Glyph::new('\u{E74F}', "🔇"), // Mute
        SystemAction::TurnOffDisplay => Glyph::new('\u{E7F4}', "🖥"), // TVMonitor
    }
}
