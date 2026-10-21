//! Launcher state (plan §6): the current index, ranking, selection, bounded
//! icon textures, scan scheduling, and launch/usage bookkeeping.
//!
//! Backend-neutral like `model`: it never starts threads. It raises requests
//! (`take_*`) that the host executes and consumes the resulting `AppEvent`s.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::emath::GuiRounding as _;
use egui::{
    Color32, Key, Modifiers, Pos2, Rect, RichText, Sense, TextureHandle, TextureOptions, Vec2,
};

use std::collections::BTreeMap;

use crate::core::app_index::{
    AppId, Command, IndexedApp, LaunchSpec, builtin_commands, script_entries,
};
use crate::core::calc::{self, Answer};
use crate::core::config::{PaletteSize, ScriptConfig, SearchEngine};
use crate::core::favorites::{Favorites, MAX_FAVORITES};
use crate::core::matcher::{Ranked, rank};
use crate::core::usage::Usage;
use crate::core::web;
use crate::win::icon::IconRgba;
use crate::win::persistence::{Stored, unix_ms_now};
use crate::win::shell::{BrokenShortcut, LaunchError, ScanOutcome};

use super::notice::{Notice, NoticeStore, Severity, op};
use super::{list, theme};

/// A summon rescans only when the last completed scan is older than this (§1.4).
const RESCAN_AFTER: Duration = Duration::from_secs(5 * 60);
/// Application icon textures kept alive (§6.4).
const ICON_CACHE_CAP: usize = 128;
/// Result rows shown at once in a medium palette.
const MEDIUM_ROWS: usize = 7;
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
    /// The pixel edge the cached icons were extracted at.
    edge: u32,
}

impl IconCache {
    /// Icons are prescaled to exact pixels, so a monitor DPI change starts
    /// the cache over.
    fn set_edge(&mut self, edge: u32) {
        if edge != self.edge {
            *self = Self {
                edge,
                ..Self::default()
            };
        }
    }

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
    icons: IconCache,
    /// Result rows shown at once, from the palette size.
    rows: usize,
    /// Mouse-wheel distance not yet worth a whole row.
    wheel: f32,
    /// The calculator's answer for the query; when present it is row 0,
    /// above the ranked apps.
    answer: Option<Answer>,
    /// The answer the user chose; the model pastes it.
    paste_answer: Option<String>,
    /// The web search fallback's engine, and the template for a custom one.
    search_engine: SearchEngine,
    custom_search_url: String,
    /// The query offered as a web search: set only when nothing else
    /// matches, so its row is the only one.
    web_query: Option<String>,
    /// A search URL the user chose; the model opens it in the browser.
    open_url: Option<String>,
}

impl Launcher {
    /// Starts with the persisted usage and requests the startup scan.
    pub fn new(
        usage: Stored<Usage>,
        favorites: Stored<Favorites>,
        scripts: &BTreeMap<String, ScriptConfig>,
    ) -> Self {
        let scripts = script_entries(scripts);
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
            usage: usage.value,
            usage_generation: 0,
            ranked: Vec::new(),
            ranked_for: None,
            selected: 0,
            first_row: 0,
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
            icons: IconCache::default(),
            rows: MEDIUM_ROWS,
            wheel: 0.0,
            answer: None,
            paste_answer: None,
            search_engine: SearchEngine::default(),
            custom_search_url: web::DEFAULT_SEARCH_URL.to_owned(),
            web_query: None,
            open_url: None,
        };
        // No scan has run yet, so the list is just commands and scripts.
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
        icon_key: &str,
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
            self.icons.pending.remove(&app_id);
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

    /// Replace the configured scripts after a config reload (§10.3).
    pub fn set_search_engine(&mut self, engine: SearchEngine, custom_url: &str) {
        self.search_engine = engine;
        self.custom_search_url = custom_url.to_owned();
    }

    /// A web search the user chose, to open in the default browser.
    pub fn take_open_url(&mut self) -> Option<String> {
        self.open_url.take()
    }

    pub fn set_palette_size(&mut self, size: PaletteSize) {
        self.rows = theme::visible_rows(MEDIUM_ROWS, size);
    }

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
        self.icons.textures.retain(|id, _| !stale.contains(id));
        self.icons.recency.retain(|id| !stale.contains(id));
        self.icons.failed.retain(|id| !stale.contains(id));
        self.icons.pending.retain(|id| !stale.contains(id));
        self.scripts = scripts;
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

    /// The app on list row `row`, if that row is an app (not the calculator
    /// card or the web search).
    fn app_at(&self, row: usize) -> Option<&IndexedApp> {
        if self.web_query.is_some() {
            return None;
        }
        let ranked = self.ranked.get(row.checked_sub(self.answer_rows())?)?;
        self.apps.get(ranked.index)
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
            self.selected = if up { row.saturating_sub(1) } else { row + 1 };
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
        if self.answer.is_some() && self.selected == 0 {
            return "Paste Answer";
        }
        if self.web_query.is_some() {
            return "Search Web";
        }
        match self.app_at(self.selected).map(|a| &a.launch) {
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
        let count = self.row_count();
        list::step_selection(&mut self.selected, count, down, up);
        if enter {
            self.commit(self.selected);
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
            self.toggle_favorite(self.selected);
        }
        let on_favorite = self.selected >= self.answer_rows()
            && self.selected - self.answer_rows() < self.favorite_rows;
        if on_favorite && (move_up || move_down) {
            self.move_favorite(self.selected, move_up);
        }
    }

    /// 1 when a calculator answer occupies row 0.
    fn answer_rows(&self) -> usize {
        usize::from(self.answer.is_some())
    }

    fn row_count(&self) -> usize {
        self.answer_rows() + self.ranked.len() + usize::from(self.web_query.is_some())
    }

    /// Rank apps and evaluate the calculator for `query`, once per change.
    /// The model calls it before drawing the section header, which depends
    /// on whether there is an answer.
    pub fn refresh(&mut self, query: &str) {
        if self.ranked_for.as_deref() != Some(query) {
            if self.ranked_for.is_some() {
                // The query changed (not just the index): start from the top.
                self.selected = 0;
                self.first_row = 0;
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
        self.answer = calc::evaluate(query);
        // Nothing matched: offer the query to the web, like Raycast's
        // fallback commands. Not during the first scan, when the app may
        // simply not be indexed yet.
        let first_scan = !self.indexed && self.scan_active;
        self.web_query =
            (self.ranked.is_empty() && self.answer.is_none() && !trimmed.is_empty() && !first_scan)
                .then(|| trimmed.to_owned());
        self.ranked_for = Some(query.to_owned());
    }

    pub fn has_answer(&self) -> bool {
        self.answer.is_some()
    }

    pub fn results_ui(&mut self, ui: &mut egui::Ui, query: &str) {
        self.refresh(query);
        let total = self.row_count();
        self.selected = self.selected.min(total.saturating_sub(1));

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
        if let Some(query) = &self.web_query {
            let engine = self.search_engine;
            let leading = |ui: &mut egui::Ui| {
                theme::engine_badge(ui, engine, ICON_POINTS);
            };
            let trailing = |ui: &mut egui::Ui| {
                ui.label(RichText::new("Web Search").weak().small());
            };
            let name = match engine {
                SearchEngine::Custom => format!("Search the web for \u{201C}{query}\u{201D}"),
                _ => format!("Search {} for \u{201C}{query}\u{201D}", engine.name()),
            };
            if list::row(ui, true, &name, leading, trailing).clicked() {
                self.commit(0);
            }
            return;
        }

        // The answer card stays put above the apps, which scroll in the rows
        // the card and the "Results" header leave.
        let offset = self.answer_rows();
        // Favorites and the other suggestions are split by a "Suggestions"
        // header, which takes one row.
        let split = self.favorite_rows > 0 && self.ranked.len() > self.favorite_rows;
        let app_rows = if offset == 1 {
            self.rows.saturating_sub(CARD_ROWS).max(1)
        } else if split {
            self.rows.saturating_sub(1).max(1)
        } else {
            self.rows
        };
        let step = list::wheel_rows(ui, theme::ROW_STRIDE, &mut self.wheel);
        if self.selected >= offset {
            let app = list::scroll_window(
                &mut self.first_row,
                self.selected - offset,
                self.ranked.len(),
                app_rows,
                step,
            );
            self.selected = app + offset;
            list::keep_visible(self.selected - offset, &mut self.first_row, app_rows);
        } else {
            // The calculator card stays selected; only the apps scroll.
            list::scroll_window(
                &mut self.first_row,
                self.selected,
                self.ranked.len(),
                app_rows,
                step,
            );
        }
        let end = (self.first_row + app_rows).min(self.ranked.len());
        let icon_px = (ICON_POINTS * ui.ctx().pixels_per_point()).round() as u32;
        self.icons.set_edge(icon_px);

        let mut clicked = None;
        if let Some(answer) = &self.answer {
            if answer_card(ui, answer, self.selected == 0).clicked() {
                clicked = Some(0);
            }
            if !self.ranked.is_empty() {
                theme::section_header(ui, "Results", None);
            }
        }
        let mut menu = None;
        for row in self.first_row + offset..end + offset {
            if split && row - offset == self.favorite_rows {
                theme::section_header(ui, "Suggestions", None);
            }
            let app = &self.apps[self.ranked[row - offset].index];
            let selected = row == self.selected;
            let favorite = self.favorites.contains(&app.id);
            let in_favorites = row - offset < self.favorite_rows;
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

            let launching = self
                .launch_pending
                .as_ref()
                .is_some_and(|(id, _)| *id == app.id);
            let leading = |ui: &mut egui::Ui| {
                let size = Vec2::splat(ICON_POINTS);
                match (&icon, &app.launch) {
                    (Some(texture), _) => {
                        // Draw texel-for-pixel on whole device pixels; any
                        // resample here would blur the prescaled icon.
                        let (slot, _) = ui.allocate_exact_size(size, Sense::hover());
                        let ppp = ui.ctx().pixels_per_point();
                        let [w, h] = texture.size();
                        let mut drawn = Vec2::new(w as f32, h as f32) / ppp;
                        // One extracted before a DPI change: fit the slot.
                        if drawn.max_elem() > ICON_POINTS {
                            drawn *= ICON_POINTS / drawn.max_elem();
                        }
                        let min = (slot.center() - drawn / 2.0).round_to_pixels(ppp);
                        ui.painter().image(
                            texture.id(),
                            Rect::from_min_size(min, drawn),
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
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
            // The right column is the entry type (§5.4); while searching, a
            // favorite says so, since it is not in its own section then.
            let kind = if launching {
                "Launching…".to_owned()
            } else if favorite && self.favorite_rows == 0 {
                format!("Favorite · {}", kind_label(&app.launch))
            } else {
                kind_label(&app.launch).to_owned()
            };
            let trailing = |ui: &mut egui::Ui| {
                ui.label(RichText::new(kind).weak().small());
            };
            let response = list::row(ui, selected, &app.display_name, leading, trailing);
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
            self.selected = row;
            match action {
                FavoriteAction::Toggle => self.toggle_favorite(row),
                FavoriteAction::Up => self.move_favorite(row, true),
                FavoriteAction::Down => self.move_favorite(row, false),
            }
        } else if let Some(row) = clicked {
            self.selected = row;
            self.commit(row);
        }
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
        if let Some(query) = &self.web_query {
            let template = self.search_engine.template_or(&self.custom_search_url);
            self.open_url = Some(web::search_url(template, query));
            return;
        }
        let Some(app) = self.app_at(row) else {
            return;
        };
        let (app_id, spec) = (app.id.clone(), app.launch.clone());
        if let LaunchSpec::Command(command) = spec {
            // Opening a command view counts as a use for ranking.
            self.open_command = Some(command);
            self.record_usage(&app_id);
            return;
        }
        self.launch_pending = Some((app_id.clone(), Instant::now()));
        self.launch_request = Some(LaunchRequest { app_id, spec });
    }
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
