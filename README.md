# smolcast

A small, Windows-native command palette inspired by Tinycast. Planned: fuzzy app launch, searchable clipboard history, emoji, and named scripts. Optional: snippet expansion.

Rust + egui + windows-rs. No Electron, WebView, Node runtime, extensions, AI features, or telemetry. One executable using Windows system APIs; the 5–15 MB release size in [BRIEF.md](BRIEF.md) is a target, not a measured result. Build tools and optional script interpreters are development/user dependencies, not bundled runtimes.

## Status

**Milestone 1 implemented (Linux side); Windows smoke pending.**

Cargo package, pinned toolchain, setup scripts, CI, config schema v1, eframe Glow root viewport, hotkey + tray + notice machinery, named-mutex single instance, the clipboard HWND worker with ready handshake, and ordered shutdown are coded. On the Linux host: `cargo test --lib` (13 config tests), `fmt`, and `clippy` pass, and the GNU debug + release cross-links succeed (release ≈ 5.9 MB, GUI subsystem, Windows system DLLs only). The Windows-gated proof — MSVC debug+release build, the interactive smoke gates (hotkey/tray wake after 60 s hidden, 100 summon/hide cycles, X-hides, tray-Quit exits, second-launch silence, hotkey-conflict recovery), and the §2.2 size/CPU/RAM/latency measurement — still has to run on a Windows 11 desktop.

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

Debian/Ubuntu x64 or WSL:

1. Install Task using its official installation instructions.
2. Run `task up`.

`scripts/setup.sh` installs `build-essential`, CA certificates, curl, and MinGW x64 C/C++ tools via apt, then the pinned Rust toolchain (1.98.1) with rustfmt, Clippy, and `x86_64-pc-windows-gnu`. It links a temporary Rust program against a Windows API. It requires sudo for missing system packages and network access for downloads. Both setup paths fetch the locked Cargo dependencies.

**Local verification:** `cargo test --lib`, `fmt`, and `clippy` pass; the GNU debug and release cross-links succeed (release ≈ 5.9 MB, GUI subsystem). The Windows-native MSVC build, runtime smoke, and idle/latency measurements have not yet been performed.

### Commands

| Command | Contract |
| --- | --- |
| `task` | List commands |
| `task up` | Install development dependencies and smoke-check the toolchain; no app launch |
| `task build` | Debug Windows build, using the lockfile |
| `task build-release` | Release Windows build, using the lockfile |
| `task run` | Build and run natively on Windows, or via WSL Windows interop |
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
- **Privacy:** clipboard history is session-only with pause, clear, and delete controls; clipboard contents never enter logs. Honor Windows clipboard-history exclusion hints; do not promise automatic detection of every password or secret.
- **Paste-back:** record HWND/PID/thread before showing the palette; write the clipboard on its owner thread, validate/restore the destination, wait for all physically held Control/Shift/Alt/Windows keys to be released, and inject only while the same destination is foreground. Never synthesize releases for user-held keys, use incomplete foreground-unlock tricks, or retry into another window. Clipboard-only/manual paste is the safe fallback; no automatic elevation.
- **Emoji:** use pinned Unicode Emoji/CLDR data and preserve complete grapheme sequences. V1 preview is monochrome. Destination color depends on the destination renderer/font and is not guaranteed; unchanged pasted codepoints are the contract.
- **Scripts:** user-authored TOML command entries, explicit interpreter and working directory, optional registered hotkey. Treat command text as intentional user code, never interpolate fuzzy-search text. Surface config errors, unavailable interpreters, launch errors, and shortcut conflicts. PowerShell 7 (`pwsh`) is optional and not shipped as an app requirement.
- **Footprint/native feel:** use eframe + Glow and demand-driven redraw while the root window is hidden. If the milestone-1 idle/size gate fails, cut over cleanly to direct egui-winit/egui_glow; do not destroy the only window/event loop that owns summon delivery. Verify system fonts, DPI changes, multi-monitor position, theme and contrast, keyboard navigation, and basic IME behavior. Mica is best-effort with a solid fallback.
- **Persistence:** `%APPDATA%\smolcast\config.toml` stores settings/scripts and `%LOCALAPPDATA%\smolcast\usage.json` stores usage. Writes use same-directory temp files and atomic replacement. Clipboard contents stay off disk; the app index is rescanned rather than cached in v1; login registration is opt-in.
- **Snippets:** low-level input hooks bring layout, IME, dead-key, recursion, and integrity-level risks. Separate optional milestone, disabled by default; do not make it a prerequisite for shipping the core features.

Relevant API constraints: Windows can deny [foreground activation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow), and [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) cannot inject into higher-integrity applications. Packaged app launch uses [ActivateApplication](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iapplicationactivationmanager-activateapplication). Native Rust builds require [MSVC tools and a Windows SDK](https://rust-lang.github.io/rustup/installation/windows-msvc.html).

## Implementation plan

The engineering plan lives in **[plans/IMPLEMENTATION_PLAN.md](plans/IMPLEMENTATION_PLAN.md)** — the approved, authoritative spec for dependencies, module layout, event/thread contracts, data schemas, ranking, clipboard ownership, paste-back, CI, and milestone behavior gates. Milestone 1 is implemented through the Linux-verifiable scope; the Windows-hosted smoke gates and measurements are the remaining gate to milestone 2.

This README stays the product/status doc; the spec is the plan of record. If the two disagree, the spec wins.
