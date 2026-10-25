# System patterns: smolcast architecture contracts

The normative engineering spec, moved here from the former
`plans/IMPLEMENTATION_PLAN.md` (approved 2026-09-18, adversarially revised).
Section numbers are unchanged because code comments cite them as
"plan §N" / "§N". Later choices that refine or replace a section are recorded
in `decisions/`; where a decision and this file disagree, the decision wins
and this file should be updated in the same change. Current status, pending
Windows checks and next steps are in `activeContext.md`; visual design is in
`docs/DESIGN.md`.

---

## 0. Normative implementation rules

These rules apply to every milestone:

- Windows 11 x86-64 is the runtime target. Linux only host-runs the pure library
  tests and cross-links the Windows GNU binary. A Linux build is not runtime proof.
- The process has one eframe/winit event loop on the main thread. The root palette
  window remains alive and is hidden, not destroyed, while the process is resident.
- No polling loop keeps a hidden window alive. Every external producer sends a
  typed event and explicitly requests an egui repaint.
- The UI thread never performs app enumeration, icon extraction, clipboard
  contention retries, or process waiting. Startup config reads may happen before
  `eframe::run_native`; visible-runtime I/O runs on a worker.
- No async runtime. Use `std::sync::mpsc`, `std::thread`, Win32 messages/timers,
  and short owning wrappers. Add a crate only when the standard library or the
  already-selected Windows APIs cannot provide the behavior cleanly.
- No COM interface pointer crosses a thread. Every thread that uses COM calls
  `CoInitializeEx`, keeps COM objects thread-local, converts results to owned Rust
  values, releases the objects, and calls `CoUninitialize`.
- All `unsafe` Windows calls live under `src/win/`. Safe modules receive owned
  values and return typed errors. Handles, clipboard ownership, registered hotkeys,
  COM initialization, window classes, timers, and hooks use RAII guards.
- Clipboard content and script command text are never logged. Diagnostics may
  include operation, error code, sizes, IDs, and counts only.
- A feature is complete only after the Windows smoke path in its milestone is run.
  Core tests do not prove windowing, focus, clipboard, tray, hotkey, COM, graphics,
  or input-injection behavior.
- Clean cutover only: no obsolete aliases, deprecated paths, placeholder modules,
  fake fallbacks, or compatibility shims remain after a milestone.
- MSVC builds load the DLLs the exe imports from System32 only
  (`/DEPENDENTLOADFLAG:0x800`, `build.rs`), so a DLL downloaded next to
  smolcast.exe is never loaded. The `app-folder-dlls` feature restores the
  default search for CI's software OpenGL (decision 029) and nothing else.

---

## 1. Process, threads, events, and window lifetime

### 1.1 Main-thread ownership

`main` acquires the single-instance mutex before calling eframe. Eframe owns the
winit event loop on the main thread. The following objects are created on that
thread from the eframe app-creation callback and remain there until shutdown:

- `GlobalHotKeyManager` and every registered global hotkey;
- `TrayIcon`, its menu, and menu IDs;
- the `eframe::App` adapter and root palette viewport.

This satisfies `global-hotkey` and `tray-icon`: both require a Win32 event loop on
the same thread as object creation. Do not move either manager to a worker.

### 1.2 Application event contract

The UI owns one `Receiver<AppEvent>`. Producers clone its `Sender<AppEvent>` and
an `egui::Context`. Start with this semantic enum; exact error payloads may be
small project error types, but do not replace typed variants with strings:

```rust
enum AppEvent {
    SummonRequested,
    Tray(TrayCommand),
    ClipboardCaptured(ClipboardEntry),
    ClipboardWriteFinished {
        request_id: u64,
        result: Result<ClipboardWriteReceipt, ClipboardError>,
    },
    IndexReady {
        generation: u64,
        result: Result<Vec<IndexedApp>, LaunchError>,
    },
    IconReady {
        app_id: AppId,
        result: Result<IconRgba, LaunchError>,
    },
    LaunchFinished {
        app_id: AppId,
        result: Result<(), LaunchError>,
    },
    ScriptSpawnFinished {
        script_id: String,
        result: Result<(), ScriptError>,
    },
    Notice(Notice),
}

enum TrayCommand {
    OpenPalette,
    Refresh,
    Quit,
}
```

Producer invariant:

1. Send the event.
2. If the send succeeds, call `ctx.request_repaint()`.
3. Never wait for the UI to acknowledge the event.

`eframe::App::update` drains all currently queued events with `try_recv` before
rendering. It ignores stale `IndexReady` generations. It must not call blocking
`recv`.

The reverse direction is the effect outbox (decision 025). The model never
starts threads or calls a worker; it queues an `app::effect::Effect` (scan,
launch, icon, glyph, file search, clipboard command, store write, thumbnail,
paste finisher, update work, config save/reload/apply/open, trial hotkey).
After `AppModel::step`, the host takes them all with `take_effects` and
executes them in order. The model's own effects keep their push order; the
views' request slots follow in a fixed order, and keep coalescing (one scan,
one launch, the latest snapshot); the host applies only the latest config
reload and trial key of a drain. A shell job, file search, update or paste
finisher that cannot start reports back through a model `*_failed` method; a
request to a worker that never started (icons, glyphs, state writer) is
dropped, as that worker's startup already raised a notice. A config save waits for the refresh worker slot in the host.

For hotkeys, call `GlobalHotKeyEvent::set_event_handler(Some(...))`; filter to
registered IDs and the pressed state, send `SummonRequested` or the mapped script
action, then request repaint. For tray/menu events, use their event handlers, not
receiver-only polling. Ignore tray hover/move events; enqueue only left-button
release, menu Open, Refresh, and Quit. On shutdown set all global event handlers
to `None` before dropping senders or managers.

The clipboard thread follows the same send-then-repaint contract. This is the only
supported wake mechanism for a hidden root window.

### 1.3 Clipboard thread

One dedicated thread owns one message-only HWND and all clipboard access. It is
responsible for:

- `AddClipboardFormatListener` / `WM_CLIPBOARDUPDATE`;
- every `OpenClipboard` / `CloseClipboard` pair;
- capture contention retries;
- every smolcast clipboard write;
- self-write and exclusion markers;
- forwarding fully owned `ClipboardEntry` values to the UI.

The UI sends `ClipboardCommand` values over an mpsc channel. After enqueueing a
command it calls `PostMessageW(clipboard_hwnd, WM_APP + 1, 0, 0)` so the message
loop wakes and drains commands. In milestone 1 the thread sends its HWND through
a one-shot ready channel after class registration and HWND creation. From
milestone 3 onward it registers the clipboard listener before sending ready.
Startup fails visibly if the handshake required by the current milestone fails.

Use `GetMessageW` / `TranslateMessage` / `DispatchMessageW`. `WM_APP + 1` drains
commands; `WM_TIMER` advances a pending retry; `WM_CLIPBOARDUPDATE` coalesces to
the latest sequence number. Shutdown removes the listener, kills timers, destroys
the HWND, unregisters its class, posts quit, and joins the thread.

### 1.4 Shell work

App scans and launches run off the UI thread. Each bounded shell job initializes an
STA with `COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`, performs its work,
converts every result to owned data, releases COM objects, and exits. At most one
index scan runs at a time.

Scan at startup. There is no directory watcher and no periodic hidden wake in v1.
On summon, start a new scan only when the last completed scan is older than five
minutes and no scan is active. Tray Refresh also starts a scan. This deliberately
chooses rescan over `ReadDirectoryChangesW` until measurements prove a watcher is
needed.

### 1.5 Shutdown order

Escape hides immediately. In `app/eframe_host.rs`, a root
`close_requested()` event sends `ViewportCommand::CancelClose` and then hides;
it does not terminate the process. Only tray Quit or a fatal startup failure sends
root `ViewportCommand::Close`.

Shutdown order is fixed:

1. Set a shared `shutting_down` flag.
2. Clear global hotkey/tray/menu event handlers.
3. Unregister all script hotkeys, then the summon hotkey; drop the managers/tray.
4. Tell clipboard and persistence workers to flush/stop; wake and join them.
5. Ask scan/icon jobs to cancel between items and join every retained job handle.

Only the state writer is joined without a limit. The clipboard worker gets
2 s, and every thread in step 5 shares one deadline 2 s after the state
writer finishes (`win::join_by`); one still running then, such as a launch
stuck on an unreachable share, is left to end with the process.
6. Let eframe destroy the viewport and graphics resources.
7. Return from `run_native`; only then release and close the named mutex.

Do not call `process::exit` from an event callback.

---

## 2. Renderer, idle behavior, and footprint

### 2.1 Approved renderer

Use eframe with `Renderer::Glow`, not wgpu. On Windows this uses glutin's OpenGL
path (WGL, or EGL only when suitable EGL libraries are present). Windows does not
provide a general system ANGLE runtime and smolcast does not bundle
`libEGL.dll`/`libGLESv2.dll`.

Initial eframe features are exactly `glow` and `default_fonts`; disable default
features. The default fonts make milestone 1 text reliable and give the approved
monochrome emoji baseline. Measure their binary cost before considering system-font
replacement.

The root viewport is created hidden. The first frame shows it when the user
started smolcast (no `--login` from the Run entry, no startup-task activation
in the Store package (§15), no `--wait-pid` from the updater; decision 037) or
when startup/config/hotkey registration has an error
that requires user action; otherwise it stays in the tray. Summon sends `ViewportCommand::Visible(true)`
then `ViewportCommand::Focus`. Hide sends `Visible(false)`. Never send `Close` to
implement hide.

Summon fast path (2026-10-21): eframe 0.36 delays every repaint of a hidden
window by at least 100 ms (`INVISIBLE_WINDOW_REPAINT_INTERVAL`), which set the
measured ~102 ms summon median. The summon-hotkey and tray-open handlers already
run on the main thread. Before they queue their event, they call
`PaletteWindow::begin_reveal`, which works only on a hidden palette. It
DWM-cloaks the window (`DWMWA_CLOAK`), places it on the cursor's monitor and
shows it with `SW_SHOWNOACTIVATE`. Because the window is visible, the repaint
runs at once. Because it is not activated, the model still captures the paste
destination before its usual `Visible(true)` + `Focus`. That keeps winit's
visibility flag in sync. The host uncloaks after one frame has been presented at
the final size (at most three frames). If the model did not show the palette
after all, the host hides and uncloaks the window directly. If cloaking fails,
the handler leaves the window alone and the normal path applies.

Eframe owns `ControlFlow`; application code must not try to set it through a
second event loop. Demand-driven behavior comes from making no repaint request
while hidden. The only allowed hidden delayed repaints are the bounded paste-back
settle checks in §8. No hidden animation, cursor blink, or periodic timer may call
`request_repaint_after`.

### 2.2 Milestone-1 measurement gate

Build an uncompressed release with:

```toml
[profile.release]
opt-level = "s"
lto = "thin"
codegen-units = 1
panic = "abort"
strip = "symbols"
```

After one summon/hide cycle and ten seconds of settling, measure for 60 seconds on
Windows 11:

- release executable: hard target `<= 15 MiB` without UPX;
- hidden average CPU: `<= 0.1%` of one logical core;
- hidden private working set: record it; `<= 80 MiB` is the review threshold, not
  a product promise;
- hotkey-to-first-visible-frame: record 20 samples; review if median exceeds
  100 ms or any sample exceeds 250 ms.

Record tool, OS build, GPU/driver, binary bytes, working set, CPU delta, and latency
samples in the milestone result. Do not claim zero dependencies: record imported
DLLs and distinguish Windows system DLLs from bundled/runtime dependencies.

If size fails, first inspect features and duplicate crate versions; do not remove
fonts blindly. If size or idle CPU still fails, switch completely to
`egui + egui-winit + egui_glow` with one custom winit event loop. That fallback
adds those crates as direct dependencies and replaces only `app/eframe_host.rs`.
`app/model.rs`, `core/`, and `win/` contracts stay unchanged. Destroy/recreate of
the root eframe window is explicitly rejected because it would stop the event
loop that delivers hotkey/tray events.

If Glow cannot create a context on the clean Windows acceptance machine, stop and
reopen the renderer decision with measured evidence. Do not silently bundle ANGLE
or enable wgpu alongside Glow; either choice changes the dependency/size contract.

---

## 3. Toolchain and dependency contract

### 3.1 Rust and Cargo

Milestone 1 creates this exact `rust-toolchain.toml` and sets package
`edition = "2024"` and `rust-version = "1.95"` (the eframe candidate's MSRV):

```toml
[toolchain]
channel = "1.98.1"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

Update both setup scripts to install toolchain `1.98.1` rather than moving
`stable`, then add only the platform's build target to that named toolchain.
Remove every `+stable` override from `Taskfile.yml`; it bypasses the repository
pin. All build/check/test commands use the toolchain selected by
`rust-toolchain.toml` and `--locked` after `Cargo.lock` exists.

Create `.cargo/config.toml` with `-C target-feature=+crt-static` for
`x86_64-pc-windows-msvc` only. Do not apply MSVC flags to GNU. The clean-machine
smoke must start without a separately installed Visual C++ runtime; the import
report confirms the actual result rather than assuming the flag worked.

### 3.2 Candidate versions to validate and then lock

These are current candidate versions, not pre-claimed “known good” versions. Use
exact requirements during the milestone-1 compatibility spike and commit the
resulting lockfile. If a candidate fails MSVC or GNU, select the newest version
that passes both, record the reason in this section and `MEMORY.md`, and keep all
eframe/egui family versions identical.

| Crate | Candidate | Features / role |
| --- | --- | --- |
| `eframe` | `0.36.2` | `default-features = false`, `glow`, `default_fonts`. |
| `egui` | `0.36.2` | direct UI contract; `default-features = false`, `default_fonts`. |
| `global-hotkey` | `0.8.0` | summon and optional script hotkeys; handler wakeups. |
| `tray-icon` | `0.25.1` | tray icon/menu; handler wakeups. |
| `windows` | `0.62.2` | Win32/COM APIs, feature-gated. |
| `fuzzy-matcher` | `0.3.7` | `SkimMatcherV2`. |
| `serde` | `1.0.229` | `derive`. |
| `toml` | `1.1.6+spec-1.1.0` | config decode/encode. |
| `serde_json` | `1.0.151` | usage state. |
| `directories` | `6.0.0` | exact APPDATA/LOCALAPPDATA bases. |
| `image` | `0.25.10` | `default-features = false`, `bmp`; DIB decode only. |
| `unicode-segmentation` | `1.13.3` | generation/runtime validation of complete graphemes. |
| `winresource` | `0.1.34` | build dependency only: icon and VERSIONINFO resources (decision 026). |

Keep `fuzzy-matcher`, serde/TOML/JSON, `image`, and
`unicode-segmentation` host-compatible because `src/lib.rs` uses them. Put eframe,
egui, hotkey, tray, directories, and `windows` under
`[target.'cfg(windows)'.dependencies]` so `cargo test --lib` on Linux does not
compile a Linux GUI stack.

Start the `windows` feature list with only modules required by the concrete API
calls:

```text
Win32_Foundation
Win32_Graphics_Dwm
Win32_Graphics_Gdi
Win32_Security
Win32_Security_Authorization
Win32_Storage_FileSystem
Win32_System_Com
Win32_System_DataExchange
Win32_System_LibraryLoader
Win32_System_Memory
Win32_System_Registry
Win32_System_Threading
Win32_UI_Input_KeyboardAndMouse
Win32_UI_Shell
Win32_UI_WindowsAndMessaging
```

Add another Windows feature only when a named call requires it; record additions
with the milestone's import-table measurement. Do not add `notify`, `winreg`, an
async runtime, a logging framework, or a UUID/hash crate for v1.

---

## 4. Package and module layout

One Cargo package contains a real library target and one shipped binary. The
library is an implementation boundary for host-testable data behavior, not a
promise of a portable product.

```text
.cargo/
  config.toml             // MSVC static-CRT target flag only
src/
  lib.rs                  // pub mod core; required by cargo test --lib
  main.rs                 // Windows entry, mutex, config bootstrap, run_native
  app/
    mod.rs
    model.rs              // backend-neutral app state + ui(ctx), event reduction
    eframe_host.rs        // thin impl eframe::App; viewport lifecycle only
    effect.rs             // the model's outbox of host work (§1.2)
    notice.rs             // bounded, content-safe user notices
    file_view.rs          // Search Files view (§6.6)
  core/
    mod.rs
    app_index.rs          // AppId, IndexedApp, dedup selection
    installed_programs.rs // Uninstall-row filter for the launcher (decision 027)
    matcher.rs            // scoring and total deterministic ordering
    usage.rs              // schema v1, ranking bonuses, saturating updates
    favorites.rs          // pinned entries, at most 5 shown (decision 011)
    hidden.rs             // entries left out of the root list (decision 039)
    config.rs             // schema v1 + pure validation
    history.rs            // payloads, dedup, recency, byte/count eviction
    list_cursor.rs        // list selection, scroll window, wheel carry
    request_cache.rs      // LRU of worker-produced values at one pixel edge
    dib.rs                // checked DIB metadata + BMP-without-header decode
    emoji_data.rs         // generated static table
    emoji.rs              // keyword matching and grapheme validation
    quicklink.rs          // {query} expansion and keyword split (§10.7)
    file_search.rs        // Windows Search SQL and result order (§6.6)
    update.rs             // versions, release choice, check schedule (§13)
  win/
    mod.rs
    hotkey.rs             // main-thread manager + conflict mapping
    tray.rs               // main-thread tray/menu IDs and callbacks
    clipboard.rs          // HWND thread, read/write/retry/marker ownership
    paste.rs              // destination identity + bounded paste state machine
    installed.rs          // Uninstall registry reader
    shell.rs              // Start Menu/AppsFolder scan, icon, shell/AUMID launch
    single_instance.rs    // named mutex guard
    persistence.rs        // APPDATA paths, atomic replace, state writer
    process.rs            // detached script spawn
    autostart.rs          // opt-in Run key
    backdrop.rs           // DWM attributes
    icon.rs               // HICON -> owned RGBA
    system.rs             // lock, sleep, restart, shut down, etc. (§6.5)
    file_search.rs        // OLE DB session with Windows Search (§6.6)
    http.rs               // WinHTTP GET (§13)
    update.rs             // update worker, download, swap, relaunch (§13)
scripts/
  generate_emoji.py       // maintainer-only deterministic data generator
```

`main.rs` imports library code as `smolcast::core`; it must not declare a second
`mod core`. `app/model.rs` exposes a method shaped like
`fn ui(&mut self, ctx: &egui::Context)` and must not mention `eframe::Frame`.
Only `app/eframe_host.rs` implements `eframe::App`. This preserves the raw-winit
fallback boundary.

`lib.rs` and every `core` module must compile without `cfg(windows)` and without
importing `windows`, eframe, egui, tray, or hotkey crates.

---

## 5. Single instance, summon hotkey, tray, and notices

### 5.1 Single instance

Before eframe initialization:

1. Obtain the current user SID from the process token and format it with
   `ConvertSidToStringSidW`.
2. Call `CreateMutexW(NULL, TRUE, "Local\\smolcast-<sid>")`.
3. Null handle is fatal. Wrap a successful handle immediately.
4. If `GetLastError() == ERROR_ALREADY_EXISTS`, close the returned handle and exit
   code 0 before creating eframe, tray, hotkeys, clipboard HWND, or workers.
5. Otherwise retain ownership until the ordered shutdown in §1.5, then call
   `ReleaseMutex` and `CloseHandle` through the guard.

A second launch does **not** summon v1. Registered-message broadcast is removed:
eframe does not expose arbitrary registered Win32 messages, and message-only HWNDs
do not receive broadcasts. Add real IPC only in a separately approved feature.

### 5.2 Summon hotkey

Default: `control+alt+Space`. Config strings use `global-hotkey`'s `HotKey::from_str`
grammar: modifiers first, then exactly one `keyboard_types::Code`, case-insensitive;
for example `shift+alt+KeyQ` or `control+alt+Space`.

Summon is idempotent, not a toggle. If hidden, capture the current destination,
show, and focus the palette. If already visible, focus it without replacing the
captured destination. Escape or root-window close is the only normal hide action.

Register the summon key before script keys. A registration failure is never a
single-instance signal. On failure:

- force the palette visible in a `HotkeyConflict` state;
- show the rejected canonical key and OS/crate error without a panic;
- keep tray Open and Quit functional;
- accept another key string, parse it, and retry for the current session;
- show the exact `summon_hotkey = "..."` config edit needed to persist it;
- never rewrite a user-authored TOML file merely to save this fallback.

The tray menu exists in milestone 1 with `Open palette`, `Refresh`, and `Quit`.
The tray icon is the exe's icon resource 1 at the startup system DPI's small-icon size
(decision 026).
A left-button release on the icon also opens the palette. Refresh parses config
off the UI thread and starts an app-index scan. A valid new summon key is
registered before the old one is unregistered. On conflict, retain the old
registration and report the error; the index scan still proceeds independently.

### 5.3 Notice behavior

`Notice` has severity, stable operation code, user message, and optional Win32 or
HRESULT numeric code. Keep at most 32 notices in memory. Never include clipboard
text/image bytes or full script arguments. Startup errors that prevent summon
must force the palette visible. Errors detected after a successful hide are stored
and shown on the next summon; do not steal focus again solely to display an error.

### 5.4 Palette navigation (approved 2026-09-24)

The root view is one flat, ranked list, as in Raycast. Applications and built-in
commands are ranked together, and each row's right-hand column names its type
(`Application`, `Command`). Built-in commands are identified by
`AppId::Command { name }`, so they share the ranking formula and usage history
with applications. Selecting a command opens its view (Clipboard History in
milestone 3, Emoji in milestone 4); scripts join the root list in milestone 5
as the `Script` type. In a sub-view, Escape or Backspace on an empty search
returns to the root. A form with unsaved changes (Settings, the snippet and
script editors) takes the first Escape and says "Press Esc again to discard";
the second leaves. Escape at the root hides the palette. Every summon opens
at the root with an empty search.

System actions (`System`, §6.5) and quicklinks (`Quicklink`, §10.7) join the
same ranked list (2026-10-22). After the ranked entries of any query that is
not a calculation or a quicklink query come two fallback rows, the web search
and "Search Files" (§6.6), under a `Search for "query"` header that takes one
list row (left out when there are no results). Fallbacks scroll with the list
(decision 022).

---

## 6. App launcher contract

### 6.1 Owned models

```rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AppId {
    Packaged { aumid_key: String },
    Classic {
        target_key: String,
        arguments: String,
        cwd_key: String,
    },
    UnresolvedShortcut { path_key: String },
}

enum LaunchSpec {
    Shortcut { lnk_path: PathBuf },
    Packaged { aumid: String },
}

struct IndexedApp {
    id: AppId,
    display_name: String,
    sort_name: String,    // Unicode-lowercased once during indexing
    aliases: Vec<String>,
    source: AppSource,
    launch: LaunchSpec,
    icon_key: String,
}
```

Windows shell code generates comparison keys before returning owned models. Use
absolute paths, normalize `/` to `\\`, remove redundant `.` components, and apply
invariant Windows lowercase mapping. Preserve original-case paths/AUMIDs in
`LaunchSpec`; comparison keys are identity only.

Enumerate:

- current-user Start Menu known folder recursively for `.lnk`;
- common Start Menu known folder recursively for `.lnk`;
- shell `AppsFolder` for entries with package identity and an AUMID; skip
  non-packaged AppsFolder rows because classic apps are sourced from shortcuts;
- the Uninstall registry keys (HKLM 64-bit, HKLM `WOW6432Node`, HKCU) last,
  per decision 027: skip `SystemComponent` = 1 and rows with a non-empty
  `ParentKeyName` or `ReleaseType`; keep a row whose `DisplayIcon` (quotes
  and `,<index>` stripped) is an absolute `.exe` outside `Package Cache`, the
  Windows folder and `WindowsApps`, not `OfficeC2RClient.exe`, whose file stem
  contains none of setup, unins, redist, maintenance, installer, update,
  dpinst, and which exists on a fixed drive (other drives are not probed, so
  a sleeping disk cannot stall the scan). Compare and launch its long path
  (`GetLongPathNameW`). Drop it when the exe's folder equals a Start Menu
  shortcut target's folder, or lies under it unless that folder is a drive
  root, a profile (`C:\Users\<name>`) or a shared parent (Program Files,
  ProgramData, Users, AppData, Local, Roaming, Programs, Games, Desktop,
  Documents, Downloads). A trailing space-separated copy of `DisplayVersion`
  is removed from the name.

Resolve shortcut target, arguments, working directory, display name, icon location,
and AppUserModelID when available, but launch classic entries by passing the
original `.lnk` path to `ShellExecuteExW`. That preserves shortcut arguments and
working directory. Launch packaged entries through
`IApplicationActivationManager::ActivateApplication`. Launch installed
programs (`LaunchSpec::Executable { path }`) by passing the exe to
`ShellExecuteExW` with its folder as the working directory; their identity is
`AppId::Classic` with no arguments and that folder as `cwd_key`. Two
Uninstall rows naming the same exe merge in §6.2; a shortcut never meets
one, since the folder rule drops the row first.

### 6.2 Deduplication

Compute identity in this order:

1. A shortcut with AppUserModelID and an AppsFolder item with the same normalized
   AUMID share `AppId::Packaged`.
2. Resolved classic shortcuts share `AppId::Classic` only when normalized target,
   raw argument string, and normalized effective working directory all match.
   Different arguments or working directories are different launch actions.
3. An unresolved shortcut uses its normalized `.lnk` path and cannot merge with a
   resolved entry.

When duplicates share an ID, choose the launch/display source by priority:
current-user shortcut, common shortcut, AppsFolder, installed program. Union the discarded display
names into `aliases`, sort/dedup aliases, and retain only one launch action. This
keeps alternative names searchable without displaying duplicate rows.

### 6.3 Ranking

For each app, fuzzy-match the query against `display_name` and every alias; retain
the highest `SkimMatcherV2` score. A non-empty query excludes entries with no
match. Empty query uses fuzzy score 0 for every app.

Usage bonus is integer-only:

```text
frequency_bonus = min(launch_count, 100) * 4
recency_bonus = 20 if age <= 1 day
                10 if age <= 7 days
                 5 if age <= 30 days
                 0 otherwise
favorite_bonus = 2000 for a favorite, 0 otherwise (decision 039)
combined_score = fuzzy_score * 100 + frequency_bonus + recency_bonus
                 + favorite_bonus
```

Treat a future or invalid last-launch timestamp as no recency bonus. Use saturating
arithmetic. Sort by this complete tuple:

1. `combined_score` descending;
2. raw `fuzzy_score` descending;
3. `launch_count` descending;
4. `last_launch_unix_ms` descending;
5. precomputed `sort_name` ascending;
6. derived `AppId::cmp` ascending.

This is the only ranking order; do not depend on filesystem, hash-map, or channel
arrival order. An empty query then moves the shown favorites to the top in the
user's order.

Hidden entries (`core::hidden`, decision 039) are dropped after ranking, for
every query; a hidden quicklink's keyword does not match. Ctrl+Shift+H or a
row's "Hide" menu item hides any entry except `Command::HiddenEntries`, and
hiding a favorite removes it from favorites. "Manage Hidden Entries"
(`View::Hidden`, drawn by the launcher) lists them in the order hidden,
filtered by the search; Enter or a click unhides. An entry not in the index
shows the name it had when hidden and "Not installed".

Increment usage only after `ShellExecuteExW` returns success or
`ActivateApplication` returns `S_OK`. Spawn success is the observable boundary;
smolcast does not wait for the launched app to create a window. Persist usage on
the state writer after each successful increment, coalescing queued snapshots so
an older snapshot cannot overwrite a newer one.

### 6.4 Icons and refresh

Do not move `HICON` across threads. Extract on a shell job, convert immediately to
owned RGBA, call `DestroyIcon` when ownership requires it, and send `IconReady`,
addressed by `IconTarget` (a launcher `AppId` or a Search Files path) and
carrying the pixel edge it was requested at. Request icons only for visible
rows. Cache at most 128 application icons in a least-recently-used
`core::request_cache::RequestCache`, which refuses a result for any edge but
the current one; dropping an entry drops its egui texture.

A broken/removed shortcut is skipped on the next scan and produces at most one
content-free notice per scan. A launch race after indexing reports the launch
error and does not increment usage.

### 6.5 System actions (2026-10-22)

`SystemAction` entries are built in (`AppId::Command { name: "system-…" }`,
so they share ranking and usage) and run on the launch thread through
`win/system.rs`; success hides the palette like a launch.

| Action | Windows call |
| --- | --- |
| Lock Screen | `LockWorkStation` |
| Sleep | `SetSuspendState(false, false, false)` |
| Restart | `ExitWindowsEx(EWX_REBOOT)` |
| Shut Down | `ExitWindowsEx(EWX_POWEROFF \| EWX_HYBRID_SHUTDOWN)` |
| Sign Out | `ExitWindowsEx(EWX_LOGOFF)` |
| Empty Recycle Bin | `SHEmptyRecycleBinW(SHERB_NOCONFIRMATION)`; an empty bin is success |
| Toggle Dark Mode | flips `AppsUseLightTheme` and `SystemUsesLightTheme`, then broadcasts `WM_SETTINGCHANGE("ImmersiveColorSet")` |
| Toggle Mute | `SendInput(VK_VOLUME_MUTE)`, which shows Windows' volume flyout |
| Turn Off Display | posts `WM_SYSCOMMAND(SC_MONITORPOWER, 2)` to `HWND_BROADCAST` |

Restart, Shut Down and Sign Out enable `SeShutdownPrivilege` and never force:
apps with unsaved work can still object. They and Empty Recycle Bin need a
second Enter (or click): the first marks the row "Press Enter again to
confirm" and the footer reads "Confirm"; moving the selection, changing the
query or re-showing the palette clears it.

### 6.6 File search (2026-10-22, decision 020)

smolcast keeps no file index. `win/file_search.rs` runs one STA thread that
holds an OLE DB session with the Windows Search provider
(`CLSID_CollatorDataSource`, initialized on first use and again after a
failure) and executes `core::file_search::sql`:

```sql
SELECT TOP 40 System.ItemPathDisplay, System.ItemType FROM SystemIndex
WHERE SCOPE='file:' AND CONTAINS(System.FileName, '"word1*" AND "word2*"')
ORDER BY System.Search.Rank DESC
```

Words are name-word prefixes; `"`, `*` and control characters are dropped and
`'` is doubled, so typed text cannot leave the literal. Rows are read one at a
time through `IGetRow::GetRowFromHROW` as `IPropertyStore`
(`System.ItemPathDisplay`); paths that no longer exist are skipped, and names
starting with the whole query move to the front. The worker drains its queue
and runs only the newest query; the view sends a query only after the text has
been unchanged for 120 ms and drops answers for older generations
(`AppEvent::FilesFound`). Enter opens the item with the shell
(`shell::open_file`, without `SEE_MASK_FLAG_NO_UI`, so Windows keeps its own
prompts such as the warning for a downloaded file); Ctrl+Enter runs
`explorer.exe /select,"<path>"`. Both are completed actions. Visible rows ask
the icon worker for Explorer's icon by path (`IconTarget::File`, the same
`IShellItemImageFactory` path as app icons, §6.4); textures are kept only for
listed items.

---

## 7. Clipboard capture, image normalization, and history

### 7.0 History source (approved 2026-09-24)

Clipboard History has two sources, chosen by `settings.clipboard_source`:

- `auto` (the default) uses Windows clipboard history when it is enabled
  (`Clipboard.IsHistoryEnabled`) and smolcast's own session history otherwise.
  The choice is re-evaluated each time the view opens.
- `windows` always uses Windows clipboard history. When Windows history is
  off, the view says so and offers a button that opens `ms-settings:clipboard`.
- `smolcast` always uses the session history specified in §7.1–§7.5.

The Windows source shares data with Win+V instead of keeping a copy. It uses
`Clipboard.GetHistoryItemsAsync`, `SetHistoryItemAsContent` (for paste-back),
`DeleteItemFromHistory` and `ClearHistory`; ClearHistory keeps pinned items.
All of these calls run on the clipboard thread, which is an STA with a message
pump and keeps every WinRT object on that thread. Blocking `.join()` waits are
acceptable there because completions run on WinRT's thread pool. The UI gets
owned snapshots only: item id, timestamp, a text preview (at most 64 KiB), or
an RGBA thumbnail (at most 256 px, decoded with `Windows.Graphics.Imaging`).
The snapshot is refreshed when the view opens, after a delete or clear, and on
`WM_CLIPBOARDUPDATE` while the view is open. Windows owns item limits,
retention, privacy exclusions and pinning, so smolcast adds no tests for them.
Capture into smolcast's session history runs only while the effective source
is `smolcast`. Neither source has Pause: Windows offers no equivalent, so it
was dropped for both. Delete and Clear all remain. Paste-back (§8) is the same
for both sources; with Windows items, step 1's clipboard write is
`SetHistoryItemAsContent`.

### 7.1 Clipboard protocol and retries

Commands are:

```rust
enum ClipboardCommand {
    SetText { request_id: u64, text: String },
    SetImage { request_id: u64, image: ImagePayload },
    SetCaptureEnabled(bool),
    Shutdown,
}
```

The worker serializes commands and capture. Writes have priority; external update
notifications coalesce to the newest `GetClipboardSequenceNumber` while a write is
pending.

Never sleep inside the window procedure. If `OpenClipboard(hwnd)` fails, arm a
one-shot timer and retry after `5, 10, 25, 50, 100` milliseconds. After the fifth
retry:

- capture: skip that sequence and increment a content-free contention counter;
- write: send `ClipboardWriteFinished` with the error and leave the palette visible.

Every successful `OpenClipboard` has one guaranteed `CloseClipboard`. Handles from
`GetClipboardData` are borrowed: use `GlobalLock`/`GlobalUnlock`; never close or
free them.

### 7.2 Privacy and self-write filtering

Register these format names once on the clipboard thread:

- `smolcast.self-write.v1`;
- `ExcludeClipboardContentFromMonitorProcessing`;
- `Clipboard Viewer Ignore`;
- `CanIncludeInClipboardHistory`.

Skip capture when:

- `smolcast.self-write.v1` is present;
- `ExcludeClipboardContentFromMonitorProcessing` or `Clipboard Viewer Ignore` is
  present; or
- `CanIncludeInClipboardHistory` is present with DWORD value 0.

Check these while the clipboard is open, with the sequence number read under the
same open, so a change between the check and the read cannot slip through. A
check before opening is only a fast path.

`CanUploadToCloudClipboard = 0` alone does not prohibit local history capture.

Every smolcast write empties the clipboard, sets the selected content, and sets a
DWORD-1 `smolcast.self-write.v1` marker in the same open transaction. Allocate each
published memory object with `GMEM_MOVEABLE`; after successful `SetClipboardData`,
Windows owns it and the local guard must relinquish without freeing. Record the
exact post-close sequence as a secondary self-write guard, including the case where
setting the marker failed after content succeeded.

**Persistence (revised 2026-09-24, user decision):** smolcast's own history
survives restarts. This reverses the earlier session-only rule. It is on by
default; setting `clipboard_persist = false` returns to session-only and
deletes the stored files at the next start. Each entry is one file,
`%LOCALAPPDATA%\\smolcast\\clipboard\\<token>.clip`. The file holds a record
(`core::history::encode_record`: magic, kind, time, body length, body)
encrypted with DPAPI `CryptProtectData`, current-user scope, secondary entropy
`smolcast.clipboard.v1`. The record is written through §10.5's atomic write on
the state-writer thread, never on the UI thread. An entry is written once when
captured, rewritten when a duplicate promotes it (its time changed), and deleted
when it is evicted, deleted or cleared. At startup the stored entries load in
the background, oldest first, unless the source is always `windows`; captures
that arrive before the load finishes are applied after it. The history limits
are re-applied as entries load. A file is deleted only when it is definitely
bad: it decrypts but does not decode, or `CryptUnprotectData` fails with
`ERROR_INVALID_DATA`, `NTE_BAD_DATA` or `NTE_BAD_KEY_STATE` (a password reset
invalidated the key). Any other decryption failure may pass (the profile still
loading), so that file is skipped but kept, as are a file that cannot be read
right now (locked by an indexer) and a record over the current item cap.
Writing a history item to the clipboard allocates every format's memory
before `EmptyClipboard`, so a failed allocation leaves the clipboard as it
was.
Exclusion hints still prevent capture, so excluded content never reaches
disk. Pause was dropped (§7.0). Clear and delete remove the entries from
memory and disk and drop their preview textures.

### 7.3 Text capture

For `CF_UNICODETEXT`:

1. Reject odd byte size or fewer than two bytes.
   Before copying/decoding, also reject `GlobalSize` greater than
   `2 * item_cap + 2` using checked arithmetic; this bounds transient UTF-16 input.
2. Find a UTF-16 NUL terminator within `GlobalSize`; reject unterminated data.
3. Decode preceding units with `String::from_utf16`; reject unpaired surrogates
   rather than silently changing paste-back content.
4. Preserve CRLF and all other valid characters exactly; do not trim whitespace.
5. Reject empty text and text whose UTF-8 length exceeds the item cap.

Store the decoded `String`. Paste-back re-encodes it to UTF-16 plus one NUL.

### 7.4 DIB capture and normalized image payload

Prefer `CF_DIBV5`; fall back to `CF_DIB`. Copy the borrowed global-memory bytes
while the clipboard is open, then close the clipboard before decoding. Reject a raw
DIB larger than the configured per-item cap before copying.

Validate before allocating decoded pixels:

- header size fits the copied bytes and is exactly 40, 52, 56, 108, or 124 bytes
  (`BITMAPCOREHEADER` size 12 is rejected in v1);
- width is positive; height is nonzero and not `i32::MIN`;
- planes equals 1;
- checked `abs(width) * abs(height) * 4` does not overflow and does not exceed the
  per-item cap;
- palette/mask offsets and declared image bytes remain inside the copied buffer.

Decode with `image::codecs::bmp::BmpDecoder::new_without_file_header`, not the BMP
file-header constructor. Convert the result once to:

```rust
struct ImagePayload {
    width: u32,
    height: u32,
    rgba: Vec<u8>, // top-down, 8-bit RGBA
}
```

Require `rgba.len() == width * height * 4`. Drop the copied DIB immediately after
normalization. Unsupported compression/header combinations are skipped with a
content-free format/error counter, not a panic.

For paste-back, encode a 124-byte `BITMAPV5HEADER` followed by top-down BGRA pixels:
negative height, 32 bits/pixel, `BI_BITFIELDS`, red/green/blue/alpha masks
`00FF0000/0000FF00/000000FF/FF000000`, and sRGB color space. Publish as
`CF_DIBV5`, and alongside it an explicit 24-bit `BI_RGB` bottom-up `CF_DIB`
composited over white. The first Windows test (2026-09-24) found that relying
on Windows to synthesize `CF_DIB` from a `BI_BITFIELDS` `CF_DIBV5` failed to
paste. Paint is the required
compatibility smoke target.

### 7.5 History and memory bounds

Defaults, configurable within validated limits:

- 100 entries (`1..=500`);
- 8 MiB retained payload per item (`64 KiB..=64 MiB`);
- 64 MiB retained payload total (`item cap..=512 MiB`).

`clipboard_max_age_days` (`0..=365`, default 0 = no age limit) also drops
smolcast's own entries captured longer ago than that, with their stored files,
when the history loads, on each capture, when the clipboard view opens and
when settings change. A promoted duplicate counts from its new capture time.

Newest is front. Payload size is computed once: `text.len()` or `rgba.len()`.
Struct/string metadata is bounded by entry count and is not included in the payload
counter.

Use `DefaultHasher` as a session-local candidate index and always compare complete
payloads after a hash match. Text equality is exact UTF-8 bytes. Image equality is
width, height, and exact RGBA bytes. No collision can merge unequal content.
A duplicate updates capture time and moves the existing allocation to front; the
new candidate allocation is dropped. It does not retain a second copy.

Reject an oversize item without evicting existing entries. After inserting a valid
item, evict oldest-first until both count and total-byte limits hold. Because the
item cap is no greater than the total cap, the new valid item cannot evict itself.

Generate preview thumbnails lazily at no more than 256x256. Cache at most 32
thumbnail textures (about 8 MiB RGBA worst case); evict the least recently used and
drop a texture when its history item is deleted/evicted. Never upload every full
history image as a GPU texture.

---

## 8. Paste-back state machine

Clipboard history and emoji use the same service. `AllowSetForegroundWindow` and
`AttachThreadInput` are not part of the v1 normal path. Do not synthesize key-up
events for physically held keys.

### 8.1 Capture destination

Immediately before showing the palette:

1. Read `GetForegroundWindow`.
2. Reject null, the palette HWND, and windows owned by the smolcast process.
3. Record HWND, PID, and thread ID from `GetWindowThreadProcessId`.

This `Destination` belongs to one summon session and is cleared when the palette
hides without a paste.

### 8.2 Commit and restore

Paste is an asynchronous state machine stored in `AppModel`:

1. Send `SetText` or `SetImage` to the clipboard thread with a monotonic request ID.
   Keep the palette visible and disable duplicate commit until the receipt arrives.
2. On write failure, keep the palette visible and show the error. Do not inject.
3. On success, call `IsWindow` and re-read PID/thread ID. If either differs from
   the captured identity, keep the palette visible, report “Copied; destination
   closed or changed — paste manually,” and do not inject.
4. Require `GetForegroundWindow()` to still equal the palette HWND. If the user
   already focused another window, do not steal focus; keep the palette state and
   report manual paste. Otherwise call `SetForegroundWindow(destination.hwnd)`
   while smolcast is still the foreground process. A zero result means
   manual-paste fallback; keep the palette visible so the message is observable.
5. On a nonzero result, hide the palette and enter `WaitingForDestination`.
6. Request a repaint after 10 ms. For at most 150 ms, require
   `GetForegroundWindow() == destination.hwnd`. If another unrelated HWND becomes
   foreground, abort immediately. If the deadline expires, store a manual-paste
   notice for the next summon and do not inject.
7. Once the destination is foreground, poll all left/right Control, Shift, Alt,
   and Windows virtual keys with `GetAsyncKeyState`. Wait up to 750 ms, repainting
   every 10 ms, for every high bit to clear; do not release keys on the user's
   behalf. Continue revalidating HWND/PID/thread and foreground on every tick.
   Timeout means clipboard-only fallback.
8. Send one four-element `SendInput` array: Control down, V down, V up, Control up.
   Require return count 4. A smaller count stores a manual-paste notice. Never
   retry injection automatically.

**Timing revision (2026-09-24, measured):** eframe 0.36 delays any repaint of
an invisible window to at least 100 ms (`INVISIBLE_WINDOW_REPAINT_INTERVAL`), so
repaint-driven 10 ms ticks after the hide in step 5 cannot meet steps 6–7. Steps
1–5 stay on the UI thread. Once the palette hides, one bounded paste-finisher
thread runs steps 6–8: it polls every 10 ms with `GetForegroundWindow`,
`IsWindow`, `GetWindowThreadProcessId` and `GetAsyncKeyState`, then calls
`SendInput`, all with the same 150 ms and 750 ms deadlines and the same abort
rules. It reports one `PasteFinished` event, which carries any manual-paste
notice to show on the next summon. After the destination becomes foreground, the finisher also waits at least
75 ms (overlapping the modifier wait) before injecting, so apps that restore
keyboard focus asynchronously, such as Chromium/Electron and XAML-island
terminals, have focused their input first. In the first Windows test, pastes
into a Claude prompt were lost while a manual Ctrl+V worked (2026-09-24).
A new summon sets the finisher's cancel flag
before it captures a new destination. Step 4's foreground check tests that the
foreground window belongs to the smolcast process; the visible palette is that
process's only top-level window, and this avoids a raw-window-handle dependency.

The clipboard-set receipt is the guaranteed outcome. `SendInput` cannot prove the
destination consumed Ctrl+V, and UIPI does not identify itself distinctly. An
elevated destination therefore receives clipboard content but may receive no
injected keys; smolcast never elevates automatically.

Escape during steps 1–3 cancels injection but cannot undo clipboard content if the
write already completed. A new summon cancels any stale paste state before
capturing a new destination.

---

## 9. Emoji data and rendering

Pin generated data to Unicode Emoji 17.0 and CLDR 48.2 English annotations. Fetch
only these immutable sources:

- `https://www.unicode.org/Public/17.0.0/emoji/emoji-test.txt`;
- `https://raw.githubusercontent.com/unicode-org/cldr/release-48-2/common/annotations/en.xml`;
- `https://raw.githubusercontent.com/unicode-org/cldr/release-48-2/common/annotationsDerived/en.xml`.

The generator emits one entry per fully-qualified emoji sequence and records all
source URLs, versions, SHA-256 values, generator version, and the Unicode License
notice in the generated header.

`scripts/generate_emoji.py` is maintainer-only; it is not required to build or run
the app. Generated `src/core/emoji_data.rs` is committed. Sort entries by Unicode
emoji-test order and sort/dedup lowercased keywords so regeneration is byte-for-byte
deterministic.

Each entry stores the complete sequence string, English short name, and keywords.
At generation time and in one core regression test, require exactly one extended
grapheme cluster via `unicode-segmentation`. Runtime paste writes the stored string
verbatim; it never reconstructs ZWJ, modifier, variation-selector, or flag
sequences.

**Implementation notes (2026-09-24):** The generator uses only the standard
library, so it has no extended-grapheme segmenter. The one-grapheme check
therefore runs over the entire table, all 3,944 entries, in the core test
`core::emoji::tests::every_entry_is_one_grapheme`, which maintainers run after
regenerating; it is not limited to the four sample sequences. The generated
module is `#[rustfmt::skip]` so formatting cannot break byte-for-byte
regeneration. Search is deterministic, not fuzzy: every query word must match
the name or a keyword, and scores are ranked (the whole-phrase match against
the punctuation-normalized name ranks first). Skin-tone variants are listed
only when the query mentions "tone" or "skin"; otherwise the base emoji stands
for them. "Search Emoji" is a root Command (§5.4). Pasted emoji use the shared
paste service and are not recorded in smolcast's clipboard history.

**Rendering revision (2026-09-24, user decision, replaces the monochrome
preview below):** In the first Windows test, egui's bundled emoji font lacked
many common emoji, which rendered as tofu boxes. egui also has no text shaping,
so ZWJ, flag and keycap sequences broke into their parts. The picker now draws
glyphs with Windows' own text stack. One `smolcast-emoji` worker thread (MTA,
created once) holds the Direct2D, DirectWrite and WIC factories. It lays out
the exact sequence in Segoe UI Emoji (centered, 0.78 em per cell), draws it
into a software WIC bitmap with `D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT`, and
returns owned premultiplied RGBA as `AppEvent::EmojiGlyph`. The UI requests
only visible cells and the selected preview, caches at most 256 textures, and
resets the cache when the display scale changes. No font or atlas is bundled.
Windows features added: Graphics_Direct2D, Direct2D_Common, DirectWrite,
Dxgi_Common and Imaging.

Superseded original text: Egui's approved v1 preview is monochrome. No color atlas is bundled. Destination
color is not guaranteed because it depends on the destination renderer/font. The
behavioral contract is unchanged codepoints. The Windows smoke also records the
cosmetic result in current Notepad, but a monochrome destination is not a smolcast
failure.

---

## 10. Configuration and persistence schemas

### 10.1 Paths

Use `directories::BaseDirs`, then join exact product paths:

- `%APPDATA%\\smolcast\\config.toml`;
- `%LOCALAPPDATA%\\smolcast\\usage.json`;
- `%APPDATA%\\smolcast\\favorites.json` and `hidden.json` (user-curated,
  roaming; decisions 011, 039).

- `%LOCALAPPDATA%\\smolcast\\clipboard\\` (DPAPI-encrypted history entries, §7.2).
- `%LOCALAPPDATA%\\smolcast\\crash.log`, `crash.old.log` and the empty
  `crash.unseen` flag, written only by the panic hook (§14).

There is no `index.json` in v1: scan in the
background on startup and measure before adding a cache/schema.

### 10.2 Config schema v1

```toml
schema_version = 1

[settings]
summon_hotkey = "control+alt+Space"
clipboard_capture = true
clipboard_max_entries = 100
clipboard_max_item_bytes = 8388608
clipboard_max_total_bytes = 67108864
clipboard_source = "auto" # optional: "auto" | "windows" | "smolcast"
start_at_login = false     # optional (§12.7)
palette_size = "medium"    # optional: window size "small" | "medium" | "large" (§12.7)
text_size = "medium"       # optional: font sizes "small" | "medium" | "large" (§12.7)
reduce_motion = "system"   # optional: "system" (Windows Animation effects) | "on" | "off" (§12.7)
clipboard_max_age_days = 0 # optional: 0..=365; 0 keeps items until the limits evict them (§7.5)
keep_state_secs = 30       # optional: 0..=3600; 0 always starts fresh (§12.7)
search_engine = "google"   # optional: google | bing | duckduckgo | brave | ecosia | startpage | kagi | custom
web_search_url = "https://www.google.com/search?q=%s" # optional; used by "custom"
check_for_updates = true   # optional: daily GitHub release check (§13)

[scripts.deploy]           # a tracked file (§10.3)
name = "Deploy"
path = "C:\\Tools\\deploy.ps1"
args = ["-Env", "prod"]    # optional; after the path
hotkey = "control+alt+KeyD" # optional

[scripts.open-project]     # a command: no path
name = "Open project"
interpreter = "pwsh.exe"
args = ["-NoLogo", "-NoProfile", "-File", "C:\\Tools\\open-project.ps1"]
cwd = "C:\\Users\\me\\Projects"
hotkey = "control+alt+KeyP" # optional

[snippets.signature]       # optional table (§10.6)
name = "Email signature"
keyword = ";sig"
text = "Best,\nMe"

[quicklinks.gh]            # optional table (§10.7)
name = "Search GitHub"
url = "https://github.com/search?q={query}"
```

Rust shape:

```rust
struct ConfigFile {
    schema_version: u32,
    settings: Settings,
    scripts: BTreeMap<String, ScriptConfig>,
    snippets: BTreeMap<String, SnippetConfig>, // §10.6
    quicklinks: BTreeMap<String, QuicklinkConfig>, // §10.7
}

struct Settings {
    summon_hotkey: String,
    clipboard_capture: bool,
    clipboard_max_entries: usize,
    clipboard_max_item_bytes: usize,
    clipboard_max_total_bytes: usize,
    clipboard_source: ClipboardSource, // #[serde(default)] = Auto (§7.0)
    start_at_login: bool,              // #[serde(default)] = false (§12.7)
    palette_size: PaletteSize,         // #[serde(default)] = Medium (§12.7)
    text_size: TextSize,               // #[serde(default)] = Medium (§12.7)
    reduce_motion: ReduceMotion,       // #[serde(default)] = System (§12.7)
    clipboard_max_age_days: u32,       // #[serde(default)] = 0, max 365 (§7.5)
    keep_state_secs: u32,              // #[serde(default)] = 30, max 3600 (§12.7)
    search_engine: SearchEngine,       // #[serde(default)] = Google
    web_search_url: String,            // #[serde(default)]; http(s), contains %s
    check_for_updates: bool,           // #[serde(default)] = true (§13)
}

struct ScriptConfig {
    name: String,
    path: Option<String>,   // a tracked file
    interpreter: String,    // default ""; required without path, override with it
    args: Vec<String>,      // default []
    cwd: String,            // default ""; required without path
    hotkey: Option<String>,
}
```

Use `#[serde(deny_unknown_fields)]` on all config structs. `scripts` defaults to an
empty map, but every settings field is materialized by `Default` when the file is
absent. Validation:

- `schema_version` must equal 1; never overwrite a newer unknown version;
- script ID matches ASCII `[A-Za-z0-9_-]{1,64}`;
- name is nonempty and at most 128 Unicode scalar values; without `path`,
  interpreter and cwd are nonempty; a `path` is absolute (drive or UNC) and
  has an extension §10.3 runs, unless `interpreter` is set;
- at most 64 arguments per script and each argument is at most 32 KiB UTF-8;
- no NUL in any string;
- clipboard limits obey §7.5;
- Windows runtime validation parses hotkeys and checks `cwd` is an absolute,
  existing directory when the script is invoked.

If config is absent, use defaults without creating a file. If startup config is
malformed, use safe settings defaults, load no scripts, force the palette visible,
and show the parse path/line error. Never overwrite the invalid file. On runtime
reload failure, retain the last-known-good config and registrations.

### 10.3 Script execution

Build `std::process::Command` directly from `interpreter`, `.args(args)`, and
`.current_dir(cwd)` (for a tracked file, as resolved below). Fuzzy query text is
never appended, expanded, substituted, or exposed as an environment variable; the
only run-time input is the "Run with Arguments" line, appended as separate
elements. Shell syntax works only when the interpreter implements it.

On Windows set `CREATE_NO_WINDOW`, null stdin/stdout/stderr, spawn, and immediately
drop the child handle. Spawn success is success; v1 does not monitor exit status.
Missing interpreter, invalid cwd, quoting/Unicode launch failure, and permission
failure become notices.

**Implementation notes (2026-09-24):** Scripts are root-list entries of type
`Script` (§5.4), identified by `AppId::Script { id }`, so they share usage
ranking; the script id is a searchable alias. `LaunchSpec::Script(ScriptSpec)`
carries the verbatim interpreter, argument vector and working directory. A
spawn runs on a launch thread and reports through `AppEvent::LaunchFinished`,
the same event as applications, instead of a separate `ScriptSpawnFinished`.
A script hotkey sends `AppEvent::RunScript(id)` and spawns without showing the
palette; a failure notice waits for the next summon. On reload, unchanged
script keys stay registered, removed or changed keys are unregistered, and new
keys register one at a time. If a changed key fails to register, that script
has no hotkey until the conflict is fixed. The summon key is never touched.

**Tracked script files (2026-10-23, decision 032):** a script is usually a
tracked file, `[scripts.<id>]` with `path`, and is tracked, edited and
untracked in the palette through `config_edit::write_script` with the
snippet editor's rules (§10.6): only changed keys are rewritten, comments
survive, and a save completes when the normal reload accepts the file.
`core::script_file::resolve` builds the process at index time: `.ps1` →
`powershell.exe -NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File
<path>` (spawned by its System32 path, never via PATH; RemoteSigned runs local
scripts and still refuses downloaded ones), `.py` →
`python.exe <path>`, `.pyw` → `pythonw.exe`, `.js`/`.mjs` → `node.exe`;
`.exe`/`.com` and `.cmd`/`.bat` are the program themselves. A batch file
must never go through `cmd /c <path>`: cmd would re-parse the arguments
(`a&b` starts `b`), while the standard library escapes arguments for a
batch program and refuses ones it cannot escape. A set `interpreter`
replaces the extension's program (`interpreter <path> args…`); `cwd`
defaults to the file's folder. The path is one verbatim argument, then
`args`. Hand-written commands without `path` run as before. A `path` is a
drive or UNC path, never `\\?\` or `\\.\`; the spawn checks the file exists
(an interpreter given a missing file would exit silently and read as a
run), and a row on a network share draws no file icon (no SMB lookups on
indexing). A bare interpreter name (`python.exe`, `node`) is resolved by
`win/process.rs` in System32, the Windows folder, then each absolute `PATH`
folder (a name without an extension tries each `PATHEXT` one), never in
smolcast's own folder, which the standard library would search first; a
name found nowhere fails the run with "interpreter not found".

"Manage Scripts" (`View::Scripts`, `app/script_view.rs`): Enter runs with
the saved arguments; Tab or Ctrl+Enter opens "Run with Arguments", whose
line is split by `script_file::split_args` (whitespace separates, double
quotes group, `""` is a literal quote, backslashes are plain) and appended
after the saved arguments, one element each; nothing is expanded and no
shell sees it. Both go through `Launcher::run_script(id, …, from_palette,
extra)`, which sets the pending launch so a successful spawn hides the
palette like a root launch; a hotkey runs with no extra arguments. Ctrl+N
tracks a file, Ctrl+E edits, Shift+Delete untracks (removes the table,
never the file). The form holds the file (Browse or typed), name, saved
arguments on one line (`join_args`; an unchanged line keeps the saved
vector exactly) and a hotkey, typed or recorded (`RecordTarget::Script`);
a hotkey that does not parse, or that another script uses, blocks the
save. A command entry shows program and working folder instead of the
file. Browse shows the `IFileOpenDialog` on its own STA thread owned by
the palette (`win/file_dialog.rs`, `Effect::PickScriptFile` →
`AppEvent::ScriptFilePicked`); while it is up, losing focus does not
dismiss the palette.

Register script hotkeys after the main summon key. Map crate hotkey IDs to script
IDs. A parse error or registration collision disables only that script hotkey; the
script remains searchable and the main key remains registered. Config reload is
transactional: parse/validate, attempt new registrations, then replace old script
registrations; never drop a working main key because one script key failed.

### 10.4 Usage schema v1

```json
{
  "schema_version": 1,
  "entries": [
    {
      "id": { "kind": "packaged", "aumid_key": "microsoft.windowsnotepad_...!app" },
      "launch_count": 12,
      "last_launch_unix_ms": 1789728000000
    }
  ]
}
```

`entries` is a vector sorted by `AppId` before writing; duplicate IDs on read are
merged with saturating count addition and the greatest timestamp. `launch_count`
is `u64`; timestamp is `i64` UTC Unix milliseconds. Unknown fields are ignored in
usage state, but schema version must equal 1.

If usage JSON is malformed or has an unsupported version, atomically rename it to
`usage.corrupt-<unix_ms>.json`, start empty, and show one notice. Usage is a cache;
config is not and is never quarantined automatically. Favorites and hidden
entries (`{"schema_version": 1, "hidden": [{"id": AppId, "name": "…"}]}`) follow
the same rule. A file that exists but cannot be read, or cannot be moved aside, is never
overwritten: the session starts empty, keeps changes in memory only, and says so.

### 10.5 Atomic writes

One persistence worker serializes state writes and coalesces queued usage snapshots
to the newest generation. For each write:

1. Create the destination directory.
2. Serialize deterministically to `<name>.<pid>.<counter>.tmp` in the same directory.
3. Write all bytes and call `File::sync_all`.
4. Call `MoveFileExW(temp, destination,
   MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`.
5. On failure, preserve the previous destination, delete the temp when safe, and
   return a noticeable error. Never truncate the destination in place.

On startup, delete only matching smolcast temp files older than 24 hours. A temp
file is never treated as valid state.

### 10.6 Snippets (palette tier, approved 2026-09-27)

Snippets are saved text in config.toml, pasted from the palette. They are the
first tier of M6 (§12.6). As-you-type expansion is a separate later tier and
reads the same table.

```toml
[snippets.signature]
name = "Email signature"
keyword = ";sig"          # optional
text = """
Best,
Me"""
```

`SnippetConfig { name: String, text: String, keyword: Option<String> }` with
`deny_unknown_fields`; the `[snippets]` table defaults to an empty map, so it
needs no schema bump. Validation:

- snippet ID matches ASCII `[A-Za-z0-9_-]{1,64}` (same rule as scripts);
- name is nonempty and at most 128 Unicode scalar values;
- text is nonempty and at most 64 KiB UTF-8;
- keyword, when present, is 1–32 Unicode scalar values with no whitespace or
  control characters, and is unique across snippets ignoring case;
- no NUL in any string.

Palette: "Search Snippets" is a root Command (`Command::Snippets`, usage key
`snippets`) that opens a list-and-preview view like Clipboard History. Search
(`core::snippet::search`) ranks an exact keyword match (ignoring case) first,
then keyword prefixes, then fuzzy name/keyword matches by score, then
case-insensitive substring matches in the text; ties sort by lowercase name,
then id. Enter or a click pastes via `PasteRequest::Text`; nothing is recorded
in clipboard history. Text is literal. The only transform is line-break
normalization to CRLF (`paste_text`). Reload replaces the list. The settings
form never touches `[snippets]`.

In-palette editing (approved 2026-09-27) is the main way to manage snippets.
Hand-editing config.toml still works, and both go through the same reload.

- Keys in the list: Ctrl+N new, Ctrl+E edit, Shift+Delete asks to delete
  (Enter confirms, Esc cancels). The root command "Create Snippet"
  (`Command::CreateSnippet`) opens the view straight into a new-snippet form,
  and an empty table offers "New snippet".
- The form has Name, an optional Keyword, and a multi-line Text field. Ctrl+S
  or Ctrl+Enter saves; Enter inserts a line break. Name and keyword are
  trimmed, and an empty keyword means none. Problems (`draft_problem`: field
  rules plus a keyword already used by another snippet) show inline and
  disable Save. Esc with unsaved changes first warns, and a second Esc
  discards. The search field is hidden while the form is open, and Backspace
  belongs to the fields. A dirty draft survives click-away, and reopening the
  view resumes it.
- A new snippet's id comes from its name (`new_id`: lowercase ASCII slug of
  at most 48 bytes, `snippet` if empty, `-2`, `-3`, … for uniqueness ignoring
  case). A rename keeps the id.
- Saving: the view raises `SnippetEdit::{Put{id, snippet, create}, Delete}`,
  which the model wraps in `ConfigEdit::Snippet` for the same host save slot
  as the settings form (`config_edit::apply` → atomic write → `ConfigReload`).
  `write_snippet` refuses malformed or newer files, refuses a create over an
  existing id and an edit or delete of a missing one, and rewrites only the
  changed keys of that table (each replaced value keeps its comment). A new
  table appends as `[snippets.<id>]` without a bare `[snippets]` header, or
  inline when the file uses an inline `snippets = {…}`. Multi-line text is
  written as a `'''` literal when it can be, and as an escaped string
  otherwise. The result must parse back to exactly the expected config, or
  nothing is written. The save completes only when the reload accepts the
  file, and a failure keeps the form open with the draft.

Room for the expansion tier (not implemented): `keyword` is the future trigger.
It is validated strictly now because loosening a rule later keeps old configs
valid, while tightening one would break them. Case-insensitive uniqueness
therefore leaves the future case rule open. Prefix overlaps (`;s` and `;sig`) are
allowed; the expansion spec decides them together with delimiter semantics.
Expansion would add an optional `snippet_expansion` setting (off by default),
and possibly an optional per-snippet opt-out, both as `serde(default)` fields.
Placeholders (`{date}`, `{clipboard}`) would need an explicit opt-in field so
existing literal braces never change meaning.

---

### 10.7 Quicklinks (2026-10-22, decision 021)

```rust
struct QuicklinkConfig {
    name: String, // nonempty, at most 128 Unicode scalar values
    url: String,  // nonempty, at most 2048; any scheme, file or folder path
}
```

The table id follows the script-id rule and is the quicklink's keyword and an
alias. Neither field may contain a control character. Quicklinks are
`AppId::Quicklink { id }` with `LaunchSpec::Quicklink { target }` and open
through `ShellExecuteExW` on the launch thread.

A `url` containing `{query}` takes a query. A root query of the form
`<id> <text>` (id matched ignoring ASCII case, text nonempty) puts that
quicklink at row 0, shown as `Name "text"`; Enter opens `url` with every
`{query}` replaced by the trimmed text, percent-encoded as in the web search
(decision 014), or inserted as typed when `url` starts with a drive (`C:\`)
or UNC (`\\`) path or a `%NAME%\` path. A leading `%NAME%` followed by a
separator is expanded from the environment at launch (`quicklink::expand_variable`),
as Explorer would; a URL's `%20` never matches. Selecting a query-taking quicklink without text (Enter,
click, or Tab) replaces the search with `<id> ` and keeps the palette open.
Nothing else in `url` is expanded.

---

## 11. CI and verification

Create `.github/workflows/ci.yml` in milestone 1.

Windows job (`windows-latest`):

```text
rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy
rustup target add x86_64-pc-windows-msvc --toolchain 1.98.1
cargo fmt --all -- --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo build --locked --release --target x86_64-pc-windows-msvc
```

Ubuntu job (`ubuntu-latest`): install MinGW x86-64, install the pinned Rust
GNU target, then run:

```text
cargo test --locked --lib
cargo build --locked --release --target x86_64-pc-windows-gnu
```

Both jobs cache the dependency build (`Swatinem/rust-cache`), and a newer push
to the same ref cancels the run it supersedes. CI builds no debug binary:
clippy type-checks every target and the release build proves linking; debug
builds stay a local command (2026-10-21).

Use an MSVC developer-command environment or `llvm-objdump -p` to capture the
release import table. Upload both release executables, byte counts, and import
reports as artifacts. Debug and release are separate commands; a debug build must
never be described as both.

The Windows job also runs the desktop smoke test (`tests/smoke.rs`, decision
029): the release exe on the runner's desktop, driven with real `SendInput`
input, checking only user-visible outcomes. It is a regression signal, not
runtime acceptance. The release gate is an interactive Windows 11 desktop smoke
on a clean standard-user account without Rust, Visual Studio, PowerShell 7, or
other developer tools.

Releases (2026-10-22, §13): `.github/workflows/release.yml` runs on a pushed
`v*` tag. It refuses a tag that differs from `v` + the Cargo.toml version,
builds the MSVC release, and publishes `smolcast-<version>-x86_64.exe` with a
`.sha256` (`<hex>  <name>`, LF) through `gh release create --generate-notes`;
a tag with `-` becomes a prerelease, which the updater never offers. The
build job has a read-only token; a separate `publish` job, which runs no
repository code, holds `contents: write` and the attestation OIDC token.
Workflows follow the hardening rules in `techContext.md` (CI).

Action `uses:` references are full commit SHAs. A trailing `# vX.Y.Z` comment
records the release so Dependabot can move the pin. `.github/dependabot.yml`
checks Cargo and GitHub Actions weekly and opens one pull request for every
version update; security updates are not delayed into that pull request.

---

## 12. Milestones and observable gates

### 1. Build foundation, resident lifetime, and palette

Implement:

- Cargo package, `src/lib.rs`, binary, exact candidate dependencies, lockfile,
  pinned toolchain, setup scripts targeting that pin, and Task commands with no
  `+stable` override;
- eframe Glow root viewport and `app/model.rs` / `app/eframe_host.rs` split;
- named-mutex guard;
- explicit hotkey/tray handler wakeups and tray Open/Refresh/Quit;
- summon/hide/Escape/window-close behavior and destination capture;
- config v1 defaults/read/validation sufficient for summon key;
- notice UI and hotkey-conflict recovery;
- clipboard HWND thread creation, command wake, ready handshake, and ordered
  shutdown path; `AddClipboardFormatListener` begins in milestone 3, when capture
  filtering and normalization are implemented together;
- ordered cleanup.

Gate on Windows:

- hotkey and tray click wake and show the palette after at least 60 seconds hidden;
- 100 repeated summon/Escape cycles leave one tray icon and one process;
- window `X` hides; tray Quit exits and releases mutex/hotkey/tray/HWND;
- second launch exits without any visible window or competing resource;
- forced hotkey conflict opens the visible recovery state, and a session fallback
  key works without breaking tray access;
- run the §2.2 size/CPU/RAM/latency measurement and record imports;
- debug and release link for MSVC and GNU; `cargo test --lib`, fmt, and clippy pass.

Do not proceed with feature work until the wake path and idle gate pass.

### 2. App launcher

Implement §6 exactly: startup/summon scans, Start Menu + AppsFolder enumeration,
owned app models, dedup/aliases, deterministic ranking, lazy bounded icons,
ShellExecuteEx shortcut launch, AUMID activation, usage schema/state writer, keyboard
selection, and Enter commit.

Core regression tests earn permanence for:

- duplicate selection priority and alias union;
- different shortcut args/cwd remain distinct;
- full ranking tuple including equal-score deterministic ties;
- frequency/recency caps and saturating usage updates;
- usage decode, duplicate merge, and deterministic encode.

Windows gate:

- launch a classic `.lnk` whose target requires shortcut arguments and cwd;
- launch a packaged app by AUMID;
- remove an indexed shortcut before commit and observe a nonfatal error/no usage
  increment;
- restart and observe usage-ranked empty-query ordering;
- trigger two rescans and prove stale generation results cannot replace newer data.

### 3. Clipboard history and shared paste service

Implement all of §§7–8: privacy markers, exact self-write filtering, bounded timer
retries, text/DIB capture, normalized images, history bounds/dedup, pause/clear/delete,
lazy thumbnails, DIBV5 write, destination validation, modifier wait, and SendInput.

Core regression tests:

- count and byte eviction boundaries and oldest victim;
- oversize rejection does not evict;
- duplicate promotion retains one equal payload and updates recency;
- hasher collision still compares payload equality;
- valid text, invalid UTF-16, missing NUL, and exact CRLF preservation;
- valid 24/32-bit DIB, truncated header/data, bad planes/dimensions, arithmetic
  overflow, and decoded-size cap;
- DIBV5 output header masks, dimensions, and BGRA order.

Windows gate:

- copy/paste text into current Notepad and image into Paint;
- repeated identical copies promote one entry without growth;
- excluded/history-disabled formats are not captured;
- an 8 MiB boundary item is accepted and a larger item rejected without eviction;
- forced clipboard contention reaches success and terminal retry paths without
  blocking the palette;
- smolcast writes do not recapture;
- closed/reused destination, user focus change, and held summon modifiers never
  inject into the wrong window;
- elevated destination keeps clipboard data and fails injection safely;
- pause, delete, clear, and thumbnail eviction release memory/textures.

### 4. Emoji picker

Generate and commit the pinned §9 table, add keyword search and a keyboard grid,
and commit through the shared paste service.

Core tests cover deterministic lookup and one-grapheme validation for a common
emoji, skin tone, flag, and ZWJ family. Windows gate pastes those sequences into
Notepad and compares resulting codepoints exactly. Record, but do not require,
color rendering. Test keyboard-only navigation at 100%, 150%, and 200% scale.

### 5. Script runner

Implement the full config/script contract in §10. Add fuzzy script results, direct
argument-vector process spawn, runtime reload, and isolated optional hotkeys.

Core tests cover schema/defaults, unknown/malformed fields, ID/limit validation,
and the exact interpreter/argument/cwd model. Do not write tests that merely assert
field forwarding.

Windows gate runs a configured script with spaces, quotes, and Unicode in paths and
arguments; reports missing interpreter/cwd and malformed reload without losing the
last-known-good config; and proves one script hotkey collision does not disrupt the
main summon key or searchable script action.

### 6. Optional snippets — separate approval gate

**Tier 1, palette snippets (approved and implemented 2026-09-27):** saved text
searched and pasted from the palette, with no input hook (§10.6). Windows gate:
a multi-line snippet pastes with CRLF line breaks into Notepad and a terminal;
keyword search ranks the exact keyword first; a reload adds and removes
snippets; an invalid snippet table keeps the last-known-good config with a
notice; a settings-form save leaves `[snippets]` byte-identical. Editor:
create (Ctrl+N and "Create Snippet"), edit, rename, keyword removal, and
delete all update config.toml while keeping hand-written comments elsewhere;
a duplicate keyword is refused inline; Esc on a dirty draft warns once; a
multi-line edit round-trips exactly.

**Tier 2, as-you-type expansion:** only if still wanted after core release
quality. It reuses the §10.6 table and `keyword` as the trigger. Before coding, extend this spec
with delimiter/case semantics and a typed input-state machine. Then implement
`WH_KEYBOARD_LL`, excluding injected input, suspending on unsupported IME
composition, keeping the callback short, and removing the hook reliably on disable
and quit. The hook cannot observe higher-integrity windows.

Gate: backspace, dead keys, layout switches, IME composition, focus changes, and
injected replacement cannot recurse or corrupt unrelated typing. No partial
expansion is preferable to unsafe partial behavior.

### 7. Windows polish and release

Implement opt-in Run-key registration/removal, minimal settings, theme/system-font
measurement, DPI/multi-monitor positioning, rounded corners, conditional Mica with
solid fallback, first-run/upgrade behavior, license notices, and checksum. UPX stays
out unless measured antivirus/startup tradeoffs are separately approved.

Positioning (implemented 2026-09-25): every show moves the still-hidden palette
onto the monitor under the cursor, centered horizontally in its work area with
the free vertical space split 1:2 above/below (`core::placement`). The move uses
`SetWindowPos` in physical pixels on the HWND from eframe's creation context, not
`ViewportCommand::OuterPosition`, which converts with the departing monitor's
scale. The size is predicted at the target monitor's effective DPI; if winit's
`WM_DPICHANGED` handling yields a different size, the move is re-centered once.

Dismissal and height (2026-09-25): losing focus hides the palette once it has
held focus since the show, except while a paste awaits its write receipt (the
receipt then reports "focus moved" and the next frame hides). The window height
grows to fit its content (notices, hotkey recovery UI), never below the 640×460
base and never above 85% of the monitor height.

Login registration (2026-09-25, `win/autostart.rs`): `start_at_login` is the
source of truth for the value `smolcast` under `HKCU\...\CurrentVersion\Run`.
Startup with a valid config and every accepted reload reconcile it: enabled
writes the quoted running-exe path plus `--login` when missing or different
(a moved exe or a flagless older entry re-registers), disabled deletes the
value; both are idempotent and quit never
touches it. A malformed startup config skips reconciliation so falling back to
defaults cannot unregister. Task Manager's `StartupApproved` state is left alone.
Failures are a content-free notice (op `autostart`). The Store package uses
its startup task instead of the Run value (§15).

Appearance (2026-09-25, `win/appearance.rs`): DWM rounded corners
(`DWMWCP_ROUND`) and a dark/light frame (`DWMWA_USE_IMMERSIVE_DARK_MODE`) that
follows egui's system-driven theme; egui draws no border or corner radius of
its own. Mica (`DWMSBT_MAINWINDOW`, showing through the transparent GL surface) is
enabled only on build ≥ 22621 with Transparency effects on and high contrast
off, decided at startup because the GL surface must be created transparent; the
panel is then transparent and the clear color is zero. Otherwise the panel is
solid `panel_fill`. Segoe UI (`%WINDIR%\Fonts\segoeui.ttf`) is placed first in
the proportional family, with egui's bundled fonts kept as symbol/emoji
fallbacks; a missing file keeps the defaults. Adds `dwmapi.dll` to imports.
`DwmExtendFrameIntoClientArea` is not used: on this undecorated winit window it
made DWM draw minimize/maximize/close buttons over the palette.

Look (2026-10-20, `app/theme.rs`; decision 028 on 2026-10-22): both egui
themes are restyled once at startup, on Raycast's structure with smolcast's
own details (search glyph, selection pip, dissolving edges).
The palette has a frameless search field, the
view, and a footer action bar pinned to the bottom. The base window is 640×460.
[`docs/DESIGN.md`](DESIGN.md) holds the layout, tokens, and rules.

Motion (2026-10-21, `core/motion.rs`, `app/motion.rs`, decision 018): the
palette animates its content in paint only, never the window or its input.
A show fades and scales the content in (160 ms; a fast-path summon starts the
clock once it is uncloaked); a dismiss (Escape at the root, click-away, the
window close) fades it out (90 ms) with input disabled, then hides the window.
A summon during that fade brings the palette back in place with its captured
destination. Completed actions (launch, paste, web search) hide at once, so
paste-back timing (§8.2) is unchanged. Moving between views or into a form
slides the body in (150 ms); selections glide between rows. `reduce_motion`
resolves against `SPI_GETCLIENTAREAANIMATION`, re-read on each show: reduced
motion keeps fades and drops movement and scaling, and sets egui's own
`animation_time` to 0. A Windows high-contrast theme (re-read on each show)
strengthens the fills and gray text. Details in [`docs/DESIGN.md`](DESIGN.md).

Settings form (2026-09-25, `app/settings_view.rs`, `core/config_edit.rs`): a
root command "smolcast settings" (named to differ from Windows Settings) opens a structured form for every `[settings]` field
(summon hotkey with Record, start at login, clipboard capture/source/persist,
entry and MiB size limits). It validates inline (hotkey parse + §7.5 limits).
Save (Ctrl+S) runs on the refresh worker: it re-reads config.toml, refuses a
file that does not parse or validate (never overwrites, §10.2), rewrites only
changed keys with `toml_edit` (comments, order, and `[scripts]` preserved;
optional keys are added only when they differ from their default), verifies the
round trip, writes atomically, then reloads through the normal `ConfigReload`
path. An absent file is created with schema_version and the settings in
declaration order. A registration conflict on the new hotkey keeps the old one
active and says so in the form. Scripts stay file-edited ("Edit config file…"
opens Notepad). `toml_edit =0.25.15` (parse+display) is a new host dependency,
about +180 KB.

Gate on a clean Windows 11 standard-user account:

- executable starts without developer runtimes or PowerShell 7;
- login enable/disable is idempotent and normal exit leaves no registration drift;
- hotkey, clipboard, launcher, emoji, scripts, keyboard navigation, basic IME,
  theme, 100/150/200% DPI, and multi-monitor movement smoke successfully;
- release size/imports and OS/build-specific Mica limits are recorded;
- no clipboard or script content leaves the device.

---

## 13. Self-update (2026-10-22, decision 019)

`win/update.rs` owns one worker thread, started by the host with
`StatePaths::update_state` (`%LOCALAPPDATA%\smolcast\update.json`) and the
`check_for_updates` setting; the host forwards setting changes.

Checking. `GET https://api.github.com/repos/Archetypically/smolcast/releases/latest`
over WinHTTP (`win/http.rs`: system proxy and certificates, https only,
redirects followed, 10 s connect / 30 s receive timeouts, user agent
`smolcast/<version>`, body capped at 1 MiB). `core::update::newer_release`
offers the release when its tag parses as semver, is greater than the running
version (semver precedence), is not a draft or prerelease, and carries
`smolcast-<version>-x86_64.exe` (size 1 B..=64 MiB) and
`smolcast-<version>-x86_64.exe.sha256`, both served under
`https://github.com/Archetypically/smolcast/releases/download/` (GitHub's
redirect to its download host is followed). The notes link opens only if it
is under the repository's `/releases/` pages, else the releases page opens.
A 404 means no published release and reads as up to date.

Schedule. Automatic checks run when due: 24 h after the last automatic check,
2 h after a failed one, and never sooner than 30 s after startup. The worker
waits in `recv_timeout` between checks (no wakeups while idle, §1.4); with the
setting off, or in a debug build, it waits on requests only. `update.json`
holds `last_check_ms`, `last_check_failed` and `skipped`; unreadable state
starts over. An automatic check does not offer a skipped version and fails
silently; "Check for Updates" (a root command) ignores the skip and reports
"latest version" or the failure as a notice.

Offer. The model keeps the newest offer and shows it under the root list when
the query is empty: "smolcast X is available (you have Y)." with Install and
Restart, What's New (the release page) and Skip This Version.

Install. Download the `.sha256` (≤ 4 KiB) and the exe (≤ its listed size),
require the exact size and a matching SHA-256 (`BCryptHash`), write
`smolcast.exe.new` beside the running exe, rename the running exe to
`smolcast.exe.old`, and rename `.new` into place; if the second rename fails,
the first is undone. Any failure leaves the running build in place and shows
an error notice (an unwritable folder points at the release page). Success
sets `quit_requested`; after `shutdown_resources` the host starts the new exe
with `--wait-pid <pid>` (a failed start shows a message box, the one UI left). `main` waits (≤ 20 s) for that process to exit before
taking the single-instance mutex (§5.1). The update worker removes `.old` and
`.new` leftovers when it starts.

Trust. The checksum detects corruption only; builds are unsigned. Before
signing ships, an attacker who can publish a release can ship code. Signing
(decision 034, SignPath Foundation) will add a check before the swap:
`WinVerifyTrust` on the download, the SignPath Foundation subject, and
smolcast's `ProductName` and `OriginalFilename` in its signed version
resource, since the certificate is shared with other projects.

Publishing. `.github/workflows/release.yml` runs only on tags that are
exactly `v<semver>` and match Cargo.toml's version; a tag with a hyphen
publishes a prerelease. While the repository is public, the job also attests
the exe's build provenance (`actions/attest-build-provenance`, Sigstore), so
`gh attestation verify <exe> --repo Archetypically/smolcast` ties a download
to the workflow and tag. The app does not check attestations.

---

## 14. Crash log and bug reports (2026-10-23, decision 035)

Crash log. `main` installs a panic hook (`win/crash_log.rs`) right after
resolving the state paths. On a panic, on any thread, it appends one entry to
`%LOCALAPPDATA%\smolcast\crash.log` and creates the empty
`crash.unseen`, then runs the default hook; the release profile then aborts.
An entry (`core::crash_log::entry`) is a `=== smolcast crash, <UTC time>`
line, the version and Windows build, the thread and source location, and the
panic message cut to 2000 characters. A write that would take the log past
64 KiB first renames it to `crash.old.log`, replacing the older one. Every
failure in the hook is ignored. Native crashes (access violations, stack
overflows) do not run the hook and are not logged.

Next start. If `crash.unseen` exists, `main` deletes it and queues one
warning notice, shown on the next summon, that names Report a Bug. The last
entry (from `crash.log`, else `crash.old.log`) is read once at startup.

Report a Bug (`Command::ReportBug`, `View::ReportBug`,
`app/report_view.rs`). The view says what the issue will carry (version and
Windows build) and, when a crash entry exists, offers an unticked "Include
the crash report from <time>" with a preview of exactly that text. Enter or
the button opens `<repository>/issues/new?body=…` (`core::crash_log::issue_url`:
an outline, the environment, and `issue_excerpt`, the crash entry cut to
1200 percent-encoded bytes so the whole URL stays near 2 KB; the preview
shows exactly that excerpt) through `shell::open_uri` and hides the
palette. Nothing is sent by smolcast: the user reviews and submits the issue
on GitHub.

---

## 15. Microsoft Store package (2026-10-25, decision 040)

One exe, two forms. `win/package::is_packaged` (`GetCurrentPackageFullName`,
cached) tells the GitHub exe from the same exe running inside the Store's
MSIX. When packaged:

- No self-update: the host starts no `UpdateWorker`, the root list has no
  "Check for Updates" (`builtin_commands(false)`), and Settings shows "The
  Microsoft Store keeps smolcast up to date" in place of the
  `check_for_updates` checkbox. The setting is kept and ignored.
- Login registration (§12.7) goes through the manifest's `windows.startupTask`
  extension, `TaskId="smolcast"`, declared `Enabled="false"`.
  `autostart::reconcile` reads `StartupTask::State`: enabled calls
  `RequestEnableAsync` (no prompt for a desktop app); disabled calls
  `Disable`. It runs only when `start_at_login` differs from the value last
  reconciled in this process, since the calls block the UI thread.
  `DisabledByUser` (Task Manager) and `DisabledByPolicy` cannot be undone by
  the app: silent at startup (Task Manager's switch is the user's), and one
  notice naming the reason when the user turns the setting on; on by policy
  is left alone. A `smolcast` Run value (the portable exe's) is deleted, so
  only one copy starts at sign-in.
- A start by the startup task (`AppInstance::GetActivatedEventArgs` kind
  `StartupTask`) is a quiet start, like `--login` (decision 037).

Manifest (`packaging/msix/AppxManifest.xml`). Identity `Name`, `Publisher`
and `PublisherDisplayName` are Partner Center's (Product identity); `Version`
is `$(Version)`, filled with `<major.minor.patch>.0`. One x64 full-trust
application (`Windows.FullTrustApplication`, `runFullTrust`), Windows
Desktop from 10.0.22000. `desktop6:FileSystemWriteVirtualization` and
`RegistryWriteVirtualization` are `disabled` (capability
`unvirtualizedResources`), so smolcast and every process it starts write to
the real AppData and HKCU: the same `%APPDATA%\smolcast` and
`%LOCALAPPDATA%\smolcast` as the portable exe, and uninstall leaves them.
Never add `virtualization:ExcludedDirectories`: on Windows 11 it replaces
the full opt-out.

Logos (`packaging/msix/Assets`) come from `scripts/generate_icon.py`:
Square44x44Logo (scales 100–400, target sizes 16–256 plated and unplated),
Square150x150Logo (the squircle at two thirds of the tile) and StoreLogo.

Packaging (`scripts/package_msix.ps1`, `task msix`). Stages the exe,
`LICENSE`, the logos and the manifest; refuses a prerelease version, a part
above 65535, or a `REPLACE-ME` identity (unless `-AllowPlaceholders`, which
CI uses); runs `makepri` (default config without its resource-pack split) and
`makeappx pack` from the newest Windows SDK. The MSIX is unsigned: the Store
signs it. `Add-AppxPackage -Register <layout>\AppxManifest.xml` with
Developer Mode installs the staged layout for testing.

Pipeline. `ci.yml` packages every Windows build with placeholders to catch
manifest and asset errors. On a release (not prerelease) tag, once the
repository variable `MSSTORE_PRODUCT_ID` exists, `release.yml`'s `msix` job
(read-only, beside `publish`, so it never holds up the GitHub release)
packages the built exe and uploads the MSIX as an artifact; after both, the
`store` job, which runs no repository code, in the `microsoft-store`
environment (limited to `v*` tags, with a required reviewer; secrets
`MSSTORE_TENANT_ID`, `MSSTORE_SELLER_ID`, `MSSTORE_CLIENT_ID`,
`MSSTORE_CLIENT_SECRET`), installs the Store Developer CLI (`msstore`, pinned
version), signs in with `msstore reconfigure`, and runs
`msstore publish <msix> --appId <id>`, which replaces any draft with one
carrying the package and commits it to certification. The CLI updates only a
product with a published submission, so the first one is manual.

