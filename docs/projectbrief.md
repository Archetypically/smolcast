# Project brief

smolcast is a minimal Raycast replacement for Windows 11, inspired by
Tinycast (a tiny native macOS launcher: fuzzy launcher, clipboard, emoji,
snippets). It is the Windows-native equivalent of the same idea, with the same
philosophy and Windows APIs: no Electron, no WebView, no Node, no telemetry.

This file is the source of truth for scope. Architecture contracts are in
`systemPatterns.md`; current status is in `activeContext.md`.

## Features

Shipped or in verification:

- **App launcher**: fuzzy search across installed apps (Start Menu shortcuts,
  packaged apps and installed programs), usage-ranked, with up to 5 pinned favorites.
- **Clipboard history**: text and images, searchable, pasted back into the
  previously focused app. Uses Windows clipboard history when it is on, or
  smolcast's own encrypted history.
- **Emoji picker**: searchable grid, pasted back exactly.
- **Scripts**: named commands from config, run from the palette or a
  per-script hotkey.
- **Snippets**: saved text searched and pasted from the palette, edited in
  place.
- **Calculator and unit conversions**: arithmetic and offline unit conversion
  inline in the root search.
- **Web search fallback**: after any search's results, search the web with a
  chosen engine in the default browser.
- **System commands**: lock, sleep, restart, shut down, sign out, empty the
  Recycle Bin, toggle dark mode, mute, turn off the display.
- **Quicklinks**: named URLs, files or folders from config, with an optional
  typed query (`gh rust egui`).
- **File search**: file and folder names through the Windows Search index
  (no index of smolcast's own), from a "Search Files" view and a fallback row.
- **Self-update**: a daily check of the project's GitHub releases (a setting),
  installed in place on request.
- **Polish**: tray icon, start at login, Raycast-style look, Mica, palette and
  text size settings.

Later, each needing its own approval: as-you-type snippet expansion (a
keyboard hook).

## Non-goals

- An extension ecosystem. It is what makes Raycast heavy (native shell,
  WebView2, Node and a Rust core exist to run TypeScript extensions); smolcast
  needs none of it.
- AI features and window management.
- Network-dependent features in the core: currency conversion, online
  dictionaries, favicon fetching. The one exception is the update check
  (decision 019): one request a day to GitHub's release API, off with a
  setting, carrying no user data.

Out of scope until there is a measured or explicit need: OCR, automatic
password/secret detection, directory watchers, an app index cache,
second-launch IPC, automatic elevation, bundled ANGLE, a color emoji atlas,
and Wine as a verification target.

## Stack

Rust + egui (eframe, Glow) + windows-rs, as one executable.

- Why: a single static binary in the 5–15 MB range, an immediate-mode UI that
  fits a palette, and compile checks from Linux through the
  `x86_64-pc-windows-gnu` target.
- Rejected: WPF (heavier idle memory, maintenance-mode framework, no Linux
  builds) and WinUI 3 (100 MB+ self-contained runtime, deployment quirks).
- The native-feel gap (Mica, rounded corners, Windows 11 fonts and colors) is
  closed with known recipes. Framework-deep limits (text rendering finesse,
  IME depth, screen readers) matter little for a palette that is visible for a
  few seconds at a time.

## Success criteria

- Summon to visible palette in about 100 ms; zero CPU while hidden.
- Release exe small (single-digit MB) and importing only Windows system DLLs.
- Nothing the user copies, types into scripts, or pastes leaves the device.
