# Tech context

## Stack and layout

One Cargo package with two targets:

- **Library** (`src/lib.rs`, `src/core/`): Windows-free logic (config,
  ranking, history, DIB codec, emoji table, calculator, favorites, snippets,
  placement, web search URLs). It compiles and tests on the Linux host.
  `lib.rs` and `core/` never import `windows`, eframe, egui, tray or hotkey
  crates (`systemPatterns.md` §4).
- **Binary** (`src/main.rs`, `src/app/`, `src/win/`): the Windows-only app.
  `app/` is the egui UI (model, views, theme); `win/` holds every `unsafe`
  Windows call behind owning wrappers (§0).

Dependencies are pinned exactly in `Cargo.toml` and locked; the toolchain is
pinned in `rust-toolchain.toml` (1.98.1, rustfmt + clippy). Host crates:
fuzzy-matcher, image (BMP only), serde, serde_json, toml, toml_edit,
unicode-segmentation. Windows-only: eframe/egui 0.36.2 (Glow), global-hotkey,
tray-icon, raw-window-handle, windows 0.62.2. Adding a crate needs a reason
the standard library or the selected Windows APIs cannot cover (§0, §3).

## Platform constraints

- Windows 11 x64 is the only runtime target. Linux cross-links the GNU binary
  and runs the library tests; that proves nothing about focus, clipboard,
  hotkeys, COM, tray, graphics or input injection. Wine is not a verification
  target.
- Windows can refuse [foreground activation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow),
  and [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
  cannot inject into higher-integrity apps; paste-back degrades to
  "copied, not pasted" (§8).
- Packaged apps launch through
  [ActivateApplication](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iapplicationactivationmanager-activateapplication)
  by AUMID, never an invented exe path (§6).
- Native MSVC builds need
  [MSVC tools and a Windows SDK](https://rust-lang.github.io/rustup/installation/windows-msvc.html).

## Setup

Install [Task v3](https://taskfile.dev/docs/installation) first, then run
`task up`:

- **Windows 11 x64** (PowerShell): `scripts/setup.ps1` installs Visual Studio
  2022 C++ Build Tools via winget when absent, checks the Windows SDK, installs
  the pinned toolchain with the MSVC target, and links a Windows API probe.
  Installers may prompt for elevation and may need a reboot.
- **Debian/Ubuntu x64 or WSL**: `scripts/setup.sh` installs build-essential,
  MinGW x64 and the pinned toolchain with `x86_64-pc-windows-gnu` (needs sudo
  for missing packages).

Machine-specific paths are never tracked. Copy `.env.example` to `.env`
(gitignored) and set `STAGE_DIR` / `INSTALL_DIR` for WSL staging.

## Commands

| Command | Does |
| --- | --- |
| `task` | List commands |
| `task up` | Install dev dependencies and smoke-check the toolchain |
| `task build` / `task build-release` | Debug / release Windows build (`--locked`) |
| `task run` | Windows: build and run. WSL: stage a debug build and launch it through interop |
| `task stage` | WSL: build (`PROFILE=release` default) and copy the exe to `STAGE_DIR` |
| `task install` | WSL: release build copied to `INSTALL_DIR` (the everyday copy) |
| `task check` / `task clippy` | Windows-target type-check / lint (`-D warnings`, all targets) |
| `task fmt` / `task fmt-check` | Format / check formatting |
| `task test` | Library tests on the host |
| `task clean` | Remove Cargo artifacts |

Default target: `x86_64-pc-windows-msvc` on Windows, `x86_64-pc-windows-gnu`
elsewhere. A running exe cannot be overwritten, so `stage` and `install`
rename the old one aside first.

## CI

`.github/workflows/ci.yml` runs fmt, clippy and the MSVC release build on
Windows, and the host tests plus the GNU release cross-link on Linux, with
dependency builds cached between runs (§11). The Windows job also runs
the desktop smoke test (`tests/smoke.rs`, decision 029): the release exe on
the runner's desktop, driven with real keystrokes and clicks, with Mesa's
software OpenGL beside the exe (the runner has no GPU); a failure uploads a
desktop screenshot. It only runs with `--ignored` and
`SMOLCAST_SMOKE_DESKTOP=disposable`, never on a maintainer's desktop. New
tests default to this level, not unit tests. CI passing is not runtime
acceptance: each milestone's Windows smoke gate (§12) is.

To release, bump `version` in `Cargo.toml`, merge, then push a matching tag
(`git tag v0.2.0 && git push origin v0.2.0`). `.github/workflows/release.yml`
builds the MSVC exe and publishes `smolcast-<version>-x86_64.exe` with its
`.sha256`, which running copies offer as an update (§13). The version, author
and site in the exe's Properties come from `Cargo.toml` through `build.rs`
(decision 026); the workflow fails if the embedded version is not the tag's.
The app icon is generated from `assets/smolcast.svg`:
`python3 scripts/generate_icon.py` (needs cairosvg and Pillow) rewrites
`assets/smolcast.ico` and `assets/smolcast.png`. A tag with a `-`
(`v0.2.0-rc.1`) is a prerelease and is never offered.

## Agent workflow notes

- Develop in WSL; nothing is installed on the Windows system drive. Try
  builds through `task stage`.
- The maintainer works on the test desktop: no scripted input or screenshots
  without asking first; prefer a manual checklist.
- Never trigger GPG/passphrase prompts; hand signing commands to the user.
