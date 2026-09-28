# smolcast

A small, Windows-native command palette inspired by Tinycast. Planned: fuzzy app launch, searchable clipboard history, emoji, named scripts, and saved snippets. Optional: as-you-type snippet expansion.

Rust + egui + windows-rs. No Electron, WebView, Node runtime, extensions, AI features, or telemetry. One executable using Windows system APIs; the 5–15 MB release size in [BRIEF.md](BRIEF.md) is a target, not a measured result. Build tools and optional script interpreters are development/user dependencies, not bundled runtimes.

## Status

**Milestone 1 implemented; automated Windows acceptance passes. A few manual tray and conflict checks remain.**

Cargo package, pinned toolchain, setup scripts, CI, config schema v1, eframe Glow root viewport, hotkey + tray + notice machinery, named-mutex single instance, the clipboard HWND worker with ready handshake, and ordered shutdown are coded. CI passes both the MSVC and Linux/GNU jobs. On Windows 11 (26100), with the GNU release build:

| §2.2 / §12.1 measurement | Result |
| --- | --- |
| Release exe size | 5,907,456 B; imports only Windows system DLLs |
| Hidden CPU after summon/hide | 0.0 ms over 65 s |
| Private memory (hidden, after 120 summons) | ~76 MiB (review threshold 80 MiB) |
| Hotkey → window visible | median ~102 ms, max ~158 ms (20 samples) — reviewed and accepted: eframe 0.36 throttles invisible-window repaints to 100 ms |
| 100 summon/Escape cycles | one process, stable handles |
| Wake after 65 s hidden | works |
| Second launch | exits 0 silently |

Manual checks passed: tray click and Quit, and hotkey-conflict recovery with a recorded temporary key.

**Milestone 2 (app launcher) implemented.** It covers Start Menu (user + common) and packaged AppsFolder enumeration, deterministic dedup with aliases, fuzzy + usage ranking, lazily loaded icons (capped at 128), shortcut launch via `ShellExecuteExW` and packaged-app activation by AUMID, and usage state in `%LOCALAPPDATA%\smolcast\usage.json` saved by an atomic, coalescing writer. It is verified on Windows 11: search, keyboard selection, packaged and shortcut launches, usage-ranked order after a restart, and repeated refreshes. Still pending: the removed-shortcut launch check.

**Milestone 3 (clipboard history + paste-back) implemented and verified on Windows 11.** Clipboard History is a root command. It uses Windows clipboard history (Win+V) when it is on, and smolcast's own DPAPI-encrypted, persisted history otherwise; `clipboard_source` and `clipboard_persist` override this. Paste-back restores the item, returns focus to the previous window, waits for modifiers and a short settle, then sends Ctrl+V. Text and image paste, persistence across restarts, and row layout are confirmed.

**Milestone 4 (emoji picker) implemented and verified on Windows 11.** "Search Emoji" is a root command over a generated, pinned table: Unicode Emoji 17.0 and CLDR 48.2, 3,944 sequences. Search is deterministic by keyword and phrase. Glyphs are drawn in color by Windows' DirectWrite (Segoe UI Emoji), and paste writes the exact sequence. Country flags show as letter pairs because Segoe UI Emoji has no flag glyphs, the same as in most Windows apps.

**Milestone 5 (script runner) implemented.** `[scripts.<id>]` config entries appear in the root list as **Script**. Each spawns the configured interpreter, argument vector and working directory directly, with no console and no interpolation. Optional per-script hotkeys run a script without the palette; a hotkey conflict disables only that script's key.

**Milestone 6, tier 1 (palette snippets) implemented; awaiting Windows checks.** **Search Snippets** lists saved snippets with a preview and pastes them through the same paste-back path. An exact keyword match ranks first, and line breaks are pasted as CRLF. Snippets are created, edited and deleted right in the palette (Ctrl+N / Ctrl+E / Shift+Delete, or the **Create Snippet** command). Edits are stored as `[snippets.<id>]` tables (`name`, `text`, optional `keyword`) in config.toml, and comments elsewhere in the file are preserved. As-you-type expansion is a later, separately approved tier that will reuse `keyword` as its trigger.

The approved engineering plan is [plans/IMPLEMENTATION_PLAN.md](plans/IMPLEMENTATION_PLAN.md). `BRIEF.md` is the product source.

## Development setup

Install [Task v3](https://taskfile.dev/docs/installation) first; it cannot bootstrap itself through `task up`.

Windows 11 x64, in PowerShell:

```powershell
winget install --id Task.Task --exact --source winget
# Open a new terminal if Task is not yet on PATH.
task up
```

`task up` invokes `scripts/setup.ps1`: installs Visual Studio 2022 C++ Build Tools when absent via winget, checks the Windows SDK, installs the pinned Rust toolchain (1.98.1, from `rust-toolchain.toml`) with rustfmt + Clippy and the MSVC target, then links and runs a temporary Windows API probe. Installers can prompt for license approval/elevation and may require a reboot. Existing incomplete Visual Studio installations may need the Windows SDK added through Visual Studio Installer as directed by the script.

> **Disk note for the maintainer's box:** Windows-side setup (below) installs several GB onto the system drive. On the WSL dev box, build only inside WSL and use `task stage`, which copies the exe to `E:\Projects\smolcast\`. MSVC binaries come from CI artifacts.

Debian/Ubuntu x64 or WSL:

1. Install Task using its official installation instructions.
2. Run `task up`.

`scripts/setup.sh` installs `build-essential`, CA certificates, curl, and MinGW x64 C/C++ tools via apt, then the pinned Rust toolchain (1.98.1) with rustfmt, Clippy, and `x86_64-pc-windows-gnu`. It links a temporary Rust program against a Windows API. It requires sudo for missing system packages and network access for downloads. Both setup paths fetch the locked Cargo dependencies.

**Local verification:** `cargo test --lib`, `fmt`, and `clippy` pass; the GNU debug and release cross-links succeed (release ≈ 5.9 MB, GUI subsystem). The MSVC build runs in CI; runtime measurements are in Status above.

### Commands

| Command | Contract |
| --- | --- |
| `task` | List commands |
| `task up` | Install development dependencies and smoke-check the toolchain; no app launch |
| `task build` | Debug Windows build, using the lockfile |
| `task build-release` | Release Windows build, using the lockfile |
| `task run` | Build and run natively on Windows; on WSL, stage a debug build to E: and launch it through interop |
| `task stage` | WSL only: build (`PROFILE=release` by default) and copy the exe to `STAGE_DIR` (default `/mnt/e/Projects/smolcast`) |
| `task check` | Windows-target type-check |
| `task clippy` | Windows-target lint, including tests |
| `task fmt` / `task fmt-check` | Format / check formatting |
| `task test` | Run portable library tests on the host |
| `task clean` | Remove Cargo artifacts |

Default target: `x86_64-pc-windows-msvc` on Windows, `x86_64-pc-windows-gnu` on Linux. Artifacts will be `target/<target>/{debug,release}/smolcast.exe`. `TARGET` can be overridden with Task variables. Commands use the pinned toolchain from `rust-toolchain.toml` (1.98.1) and `--locked`; `CARGO_HOME` is respected. These commands assume standard Cargo output paths.

Linux cross-compilation does not verify focus, clipboard, hotkeys, COM activation, tray behavior, or graphics. `task run` does not use Wine: a real Windows 11 desktop is the acceptance environment. Generic Linux can build but cannot run the app through this task.

## Survey: what remains

Feature work (app launcher, clipboard history, emoji, scripts, polish) remains, along with the Window-hosted milestone gates and packaging. The architecture contracts below are implemented so far (milestone 1) and bind the rest.

Approved architecture: one package with a host-testable core library and a Windows-only executable. Keep unsafe Windows bindings behind small owning wrappers; do not introduce a plugin system or general-purpose service framework.

- **UI/event lifetime:** keep the eframe event loop, hotkey manager, and tray on the main thread. Hotkey/tray callbacks and workers send typed events and explicitly request repaint; receiver polling alone is not a wake mechanism. One message-only Win32 window on a dedicated thread serializes every clipboard read/write and contention retry. COM jobs initialize their own apartments and return only owned data.
- **Launcher:** enumerate both Start Menu trees and shell AppsFolder items. Retain shortcut identity and use shell launch so arguments/working directories survive. Packaged apps use their AUMID/activation contract, not an invented executable path. Deduplicate before ranking and increment usage only after a successful launch.
- **Clipboard:** item count alone does not bound image memory. Approved caps: 100 entries, 8 MiB per entry, 64 MiB total. Eagerly copy valid text/DIB data, close each successful `OpenClipboard` with `CloseClipboard`, never free borrowed `GetClipboardData` handles, and use bounded timer retries. Normalize accepted images to RGBA, track exact self-write identity, and bound thumbnail textures. OCR is out of scope.
- **Privacy:** Clipboard History uses Windows clipboard history when it is on, and smolcast's own history otherwise (`clipboard_source`). smolcast's history persists across restarts DPAPI-encrypted per user (`clipboard_persist = false` for session-only), with delete and clear controls; clipboard contents never enter logs. Honor Windows clipboard-history exclusion hints; do not promise automatic detection of every password or secret.
- **Paste-back:** record HWND/PID/thread before showing the palette; write the clipboard on its owner thread, validate/restore the destination, wait for all physically held Control/Shift/Alt/Windows keys to be released, and inject only while the same destination is foreground. Never synthesize releases for user-held keys, use incomplete foreground-unlock tricks, or retry into another window. Clipboard-only/manual paste is the safe fallback; no automatic elevation.
- **Emoji:** use pinned Unicode Emoji/CLDR data and preserve complete grapheme sequences. V1 preview is monochrome. Destination color depends on the destination renderer/font and is not guaranteed; unchanged pasted codepoints are the contract.
- **Scripts:** user-authored TOML command entries, explicit interpreter and working directory, optional registered hotkey. Treat command text as intentional user code, never interpolate fuzzy-search text. Surface config errors, unavailable interpreters, launch errors, and shortcut conflicts. PowerShell 7 (`pwsh`) is optional and not shipped as an app requirement.
- **Footprint/native feel:** use eframe + Glow and demand-driven redraw while the root window is hidden. If the milestone-1 idle/size gate fails, cut over cleanly to direct egui-winit/egui_glow; do not destroy the only window/event loop that owns summon delivery. Verify system fonts, DPI changes, multi-monitor position, theme and contrast, keyboard navigation, and basic IME behavior. Mica is best-effort with a solid fallback.
- **Persistence:** `%APPDATA%\smolcast\config.toml` stores settings, scripts, and snippets, and `%LOCALAPPDATA%\smolcast\usage.json` stores usage. Writes use same-directory temp files and atomic replacement. Clipboard history entries are DPAPI-encrypted files under `%LOCALAPPDATA%\smolcast\clipboard\` unless persistence is turned off; the app index is rescanned rather than cached in v1; login registration is opt-in (`start_at_login = true` in `[settings]` writes the per-user Run key; `false` removes it). The **smolcast settings** command edits `[settings]` in place, keeping comments, scripts, and snippets, and reloads.
- **Snippets:** palette search-and-paste needs no input hook and ships now (plan §10.6). As-you-type expansion needs a low-level keyboard hook, which brings layout, IME, dead-key, recursion, and integrity-level risks. It stays a separate optional tier, disabled by default, and is not a prerequisite for shipping the core features.

Relevant API constraints: Windows can deny [foreground activation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow), and [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) cannot inject into higher-integrity applications. Packaged app launch uses [ActivateApplication](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iapplicationactivationmanager-activateapplication). Native Rust builds require [MSVC tools and a Windows SDK](https://rust-lang.github.io/rustup/installation/windows-msvc.html).

## Implementation plan

The engineering plan lives in **[plans/IMPLEMENTATION_PLAN.md](plans/IMPLEMENTATION_PLAN.md)** — the approved, authoritative spec for dependencies, module layout, event/thread contracts, data schemas, ranking, clipboard ownership, paste-back, CI, and milestone behavior gates. Milestone 1 is implemented through the Linux-verifiable scope; the Windows-hosted smoke gates and measurements are the remaining gate to milestone 2.

This README stays the product/status doc; the spec is the plan of record. If the two disagree, the spec wins.
