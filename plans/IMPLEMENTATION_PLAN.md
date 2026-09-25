# smolcast engineering implementation plan — approved

Status: implementation-ready plan; no `Cargo.toml`, `Cargo.lock`, or application
source exists yet. `task build` must continue to fail until milestone 1 creates the
package. This document is the plan of record. `BRIEF.md` is the product source and
`README.md` is the human-facing status document.

The original architecture decisions were approved on 2026-09-18. This revision
incorporates the subsequent adversarial review: wakeups are explicit, the root
window is never destroyed while it owns hotkey/tray delivery, the package has a
real library target, clipboard access is serialized on one thread, paste-back no
longer relies on `AllowSetForegroundWindow` or synthetic modifier releases, and
persistence/ranking schemas are concrete.

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

The root viewport is created hidden unless startup/config/hotkey registration has
an error that requires user action. Summon sends `ViewportCommand::Visible(true)`
then `ViewportCommand::Focus`. Hide sends `Visible(false)`. Never send `Close` to
implement hide.

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
    notice.rs             // bounded, content-safe user notices
  core/
    mod.rs
    app_index.rs          // AppId, IndexedApp, dedup selection
    matcher.rs            // scoring and total deterministic ordering
    usage.rs              // schema v1, ranking bonuses, saturating updates
    config.rs             // schema v1 + pure validation
    history.rs            // payloads, dedup, recency, byte/count eviction
    dib.rs                // checked DIB metadata + BMP-without-header decode
    emoji_data.rs         // generated static table
    emoji.rs              // keyword matching and grapheme validation
  win/
    mod.rs
    hotkey.rs             // main-thread manager + conflict mapping
    tray.rs               // main-thread tray/menu IDs and callbacks
    clipboard.rs          // HWND thread, read/write/retry/marker ownership
    paste.rs              // destination identity + bounded paste state machine
    shell.rs              // Start Menu/AppsFolder scan, icon, shell/AUMID launch
    single_instance.rs    // named mutex guard
    persistence.rs        // APPDATA paths, atomic replace, state writer
    process.rs            // detached script spawn
    autostart.rs          // opt-in Run key
    backdrop.rs           // DWM attributes
    icon.rs               // HICON -> owned RGBA
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
returns to the root. Escape at the root hides the palette. Every summon opens
at the root with an empty search.

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
  non-packaged AppsFolder rows because classic apps are sourced from shortcuts.

Resolve shortcut target, arguments, working directory, display name, icon location,
and AppUserModelID when available, but launch classic entries by passing the
original `.lnk` path to `ShellExecuteExW`. That preserves shortcut arguments and
working directory. Launch packaged entries through
`IApplicationActivationManager::ActivateApplication`.

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
current-user shortcut, common shortcut, AppsFolder. Union the discarded display
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
combined_score = fuzzy_score * 100 + frequency_bonus + recency_bonus
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
arrival order.

Increment usage only after `ShellExecuteExW` returns success or
`ActivateApplication` returns `S_OK`. Spawn success is the observable boundary;
smolcast does not wait for the launched app to create a window. Persist usage on
the state writer after each successful increment, coalescing queued snapshots so
an older snapshot cannot overwrite a newer one.

### 6.4 Icons and refresh

Do not move `HICON` across threads. Extract on a shell job, convert immediately to
owned RGBA, call `DestroyIcon` when ownership requires it, and send `IconReady`.
Request icons only for visible rows. Cache at most 128 application icons with a
simple recency queue; dropping an entry drops its egui texture.

A broken/removed shortcut is skipped on the next scan and produces at most one
content-free notice per scan. A launch race after indexing reports the launch
error and does not increment usage.

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
- `CanIncludeInClipboardHistory`.

Skip capture when:

- `smolcast.self-write.v1` is present;
- `ExcludeClipboardContentFromMonitorProcessing` is present; or
- `CanIncludeInClipboardHistory` is present with DWORD value 0.

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
are re-applied as entries load. Files that fail to decrypt or decode are
deleted, for example after a password reset invalidates the DPAPI key.
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
thumbnail textures (about 8 MiB RGBA worst case); use a simple recency queue and
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
- `%LOCALAPPDATA%\\smolcast\\usage.json`.

- `%LOCALAPPDATA%\\smolcast\\clipboard\\` (DPAPI-encrypted history entries, §7.2).

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

[scripts.open-project]
name = "Open project"
interpreter = "pwsh.exe"
args = ["-NoLogo", "-NoProfile", "-File", "C:\\Tools\\open-project.ps1"]
cwd = "C:\\Users\\me\\Projects"
hotkey = "control+alt+KeyP" # optional
```

Rust shape:

```rust
struct ConfigFile {
    schema_version: u32,
    settings: Settings,
    scripts: BTreeMap<String, ScriptConfig>,
}

struct Settings {
    summon_hotkey: String,
    clipboard_capture: bool,
    clipboard_max_entries: usize,
    clipboard_max_item_bytes: usize,
    clipboard_max_total_bytes: usize,
    clipboard_source: ClipboardSource, // #[serde(default)] = Auto (§7.0)
}

struct ScriptConfig {
    name: String,
    interpreter: String,
    args: Vec<String>,
    cwd: String,
    hotkey: Option<String>,
}
```

Use `#[serde(deny_unknown_fields)]` on all config structs. `scripts` defaults to an
empty map, but every settings field is materialized by `Default` when the file is
absent. Validation:

- `schema_version` must equal 1; never overwrite a newer unknown version;
- script ID matches ASCII `[A-Za-z0-9_-]{1,64}`;
- name/interpreter/cwd are nonempty; name is at most 128 Unicode scalar values;
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
`.current_dir(cwd)`. Fuzzy query text is never appended, expanded, substituted, or
exposed as an environment variable. Shell syntax works only when the configured
interpreter implements it; `.cmd`/`.bat` users must explicitly configure
`cmd.exe` and its arguments.

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
config is not and is never quarantined automatically.

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

---

## 11. CI and verification

Create `.github/workflows/ci.yml` in milestone 1.

Windows job (`windows-latest`):

```text
rustup toolchain install 1.98.1 --profile minimal --component rustfmt,clippy
rustup target add x86_64-pc-windows-msvc --toolchain 1.98.1
cargo fmt --all -- --check
cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo build --locked --target x86_64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
```

Ubuntu job (`ubuntu-latest`): install MinGW x86-64, install the pinned Rust
GNU target, then run:

```text
cargo test --locked --lib
cargo build --locked --target x86_64-pc-windows-gnu
cargo build --locked --release --target x86_64-pc-windows-gnu
```

Use an MSVC developer-command environment or `llvm-objdump -p` to capture the
release import table. Upload both release executables, byte counts, and import
reports as artifacts. Debug and release are separate commands; a debug build must
never be described as both.

No CI job claims runtime/UI behavior. The release gate is an interactive Windows
11 desktop smoke on a clean standard-user account without Rust, Visual Studio,
PowerShell 7, or other developer tools.

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

Only if still wanted after core release quality. Before coding, extend this spec
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

Gate on a clean Windows 11 standard-user account:

- executable starts without developer runtimes or PowerShell 7;
- login enable/disable is idempotent and normal exit leaves no registration drift;
- hotkey, clipboard, launcher, emoji, scripts, keyboard navigation, basic IME,
  theme, 100/150/200% DPI, and multi-monitor movement smoke successfully;
- release size/imports and OS/build-specific Mica limits are recorded;
- no clipboard or script content leaves the device.

---

## 13. Intentional divergences from BRIEF

- **Hotkey ownership:** hotkey/tray stay on eframe's main Win32 event loop and use
  callback-to-channel-to-repaint wakeups. The dedicated message-only HWND owns all
  clipboard reads/writes, not the hotkey.
- **Resident lifetime:** the eframe root viewport is hidden, never destroyed for
  normal operation. If its measured idle cost fails, the approved response is a
  clean host cutover, not an un-wakeable destroy/recreate loop.
- **Second launch:** exits silently; v1 has no incomplete broadcast IPC.
- **Tray pulled forward:** Open, Refresh, and Quit ship in milestone 1 so the app
  is operable during hotkey conflict and always has an exit path.
- **Clipboard representation:** images normalize to bounded RGBA and write back as
  CF_DIBV5. This makes validation, dedup, memory accounting, thumbnails, and Paint
  compatibility deterministic.
- **Emoji color:** v1 preview is monochrome and destination color is explicitly not
  guaranteed. Exact pasted grapheme codepoints are the product contract.
- **Index cache:** no disk index in v1. Background startup/summon rescan is simpler;
  add a cache only after cold-start measurement justifies schema and invalidation
  weight.

## 14. Approved decisions and closed questions

Approved on 2026-09-18 and retained after adversarial revision:

1. eframe + Glow, with a measured clean cutover to direct egui-winit/egui_glow if
   the milestone-1 footprint gate fails; no raw Direct2D/GDI implementation.
2. Monochrome emoji preview; no color atlas in v1.
3. Clipboard bounds and eviction: 100 entries, 8 MiB item, 64 MiB total; oversize
   rejected; exact duplicate promoted; oldest-first eviction.
4. Named-mutex single instance acquired before any UI/resource initialization.
5. MSVC + GNU cross-link CI, host core tests, release size/import artifacts, and no
   dependency-free claim from linking alone.

Closed by this revision:

- receiver polling alone is forbidden; every external event explicitly repaints;
- destroy-on-hide is rejected under main-loop hotkey ownership;
- `src/lib.rs` is mandatory for `cargo test --lib`;
- clipboard access is serialized on its HWND thread;
- `AllowSetForegroundWindow`, `AttachThreadInput`, and synthetic physical-modifier
  releases are not the v1 paste strategy;
- candidate versions are validated and locked in milestone 1, not described as
  already known good;
- persistence, dedup, ranking, retries, and schemas are specified above.

## Out of scope until later

Extension ecosystem, AI, window management, calculator, OCR, automatic
password/secret detection, clipboard persistence, directory watchers, index cache,
second-launch IPC, automatic elevation, bundled ANGLE, a color emoji atlas, and
Wine as a verification target.
