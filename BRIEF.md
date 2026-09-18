Minimal Raycast replacement for Windows 11, inspired by Tinycast (tiny native macOS launcher: ~4MB, zero dependencies, fuzzy launcher + clipboard + emoji + snippets). Build the Windows-native equivalent of the same idea: same philosophy, Windows APIs. No Electron, no WebView, no Node, no telemetry.

## Features (v1)

- App launcher — fuzzy search across installed apps, usage-ranked
- Clipboard history — text + images, searchable, paste back into the focused app
- Emoji picker — searchable grid
- Run scripts — named shell commands via fuzzy search, optional per-command hotkeys
- Snippets — nice-to-have: keyword expansion

## Non-goals

- Extension ecosystem (this is what bloats Raycast — its native shell + WebView2 + Node + Rust core stack exists to run TypeScript extensions; we need none of it)
- AI features, window management, calculator (maybe later)

## Stack: Rust + egui + windows-rs

- Why: ~5–15MB single static binary, immediate-mode UI fits a palette perfectly, compile-checkable from Linux via the x86_64-pc-windows-gnu target
- Rejected: WPF (heavier idle RAM, maintenance-mode framework, cannot build on Linux), WinUI 3 (100MB+ self-contained runtime, deployment quirks)
- Agents close the native-feel gap: Mica, rounded corners, Win11 colors/fonts are all known recipes. Framework-deep limits (text rendering finesse, IME, screen readers) do not matter for a palette visible two seconds at a time.

## Architecture

- One exe, tray icon, launch at login (Run registry key or Startup shortcut)
- Hidden message-only window owns the global hotkey (RegisterHotKey) → summons the egui palette; Enter runs the top hit
- Apps: enumerate .lnk files under %ProgramData%\Microsoft\Windows\Start Menu and %AppData%\Microsoft\Windows\Start Menu, resolve targets; include UWP apps via shell enumeration; rank by launch frequency
- Clipboard: AddClipboardFormatListener on the hidden window → WM_CLIPBOARDUPDATE → copy CF_UNICODETEXT / CF_DIB out eagerly (source app can free it) → ring buffer (~100 items, skip huge blobs) → ignore self-writes to avoid paste loops → paste by setting clipboard + SendInput Ctrl+V into the previously focused window
- Emoji: static unicode + keyword table → grid picker → paste via the same clipboard path
- Scripts: TOML config of named commands (pwsh -Command ...) → fuzzy search → spawn detached
- Snippets (optional): WH_KEYBOARD_LL hook → keyword match → SendInput replacement
- Win11 feel: Mica via DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE), rounded corners via DWMWA_WINDOW_CORNER_PREFERENCE, Segoe UI Variable, Win11 color tokens

## Build & verify

- Dev on Windows 11 with the MSVC toolchain: cargo build
- Release: cargo build --release, strip symbols; UPX optional

## Build order

- 1. Global hotkey + empty palette window skeleton
- 2. App launcher (fuzzy match + launch)
- 3. Clipboard history + paste-back
- 4. Emoji picker
- 5. Script runner
- 6. Snippets
- 7. Polish: Mica, tray icon, login item, minimal settings UI
