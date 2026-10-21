# smolcast

**A small, fully native command palette for Windows 11. One hotkey for your apps, clipboard, emoji, scripts and snippets, in a single executable that sits idle until you need it.**

smolcast is a Windows take on [Tinycast](https://github.com/abue-ammar/tinycast) and Raycast's core features, written in Rust with egui and the Windows APIs. No Electron, no WebView, no Node runtime, no extensions, no AI, and no telemetry: about a 7 MB exe that imports only Windows system DLLs and uses no CPU while hidden.

## Features

- **App launcher**: fuzzy search across Start Menu shortcuts and Store apps, ranked by how often and how recently you use them, with each program's real icon.
- **Favorites**: pin up to 5 entries to the top of the list (Ctrl+Shift+F), and reorder them with Ctrl+Shift+Up/Down.
- **Calculator and conversions**: type `2^10`, `40cm to in`, `100f to c` or `3 GiB as MB` and the answer appears inline. Enter pastes it. Length, weight, volume, temperature, time, data, area, speed and angles, all offline.
- **Web search fallback**: when nothing matches, search Google, Bing, DuckDuckGo, Brave, Ecosia, Startpage, Kagi or your own URL in your default browser.
- **Clipboard history**: text and images, searchable, with a preview, pasted straight back into the app you came from. Uses Windows clipboard history (Win+V) when it is on, or smolcast's own, encrypted on disk.
- **Emoji picker**: searchable grid of every Unicode 17.0 emoji, in color, pasted exactly.
- **Scripts**: name a command in the config, run it from the palette or its own hotkey.
- **Snippets**: save text you paste often, find it by name or keyword, and edit it right in the palette.
- **Feels native**: Raycast-style look, Mica backdrop, rounded corners, light and dark themes, opens on the monitor under your cursor.
- **Forgiving**: dismiss by accident and the palette comes back as you left it for 30 seconds.
- **Tidy**: offers to move broken Start Menu shortcuts to the Recycle Bin.

## Install

There are no prebuilt releases yet. Build it from source (below), then put `smolcast.exe` anywhere and run it. Turn on **Start smolcast when you sign in** in its settings to keep it around.

smolcast runs as a normal user and needs no installer, admin rights, or runtime.

## Using it

1. Press **Ctrl+Alt+Space** (change it in settings).
2. Type to search; **Up/Down** to choose; **Enter** to open, paste, or run.
3. Type `clipboard`, `emoji`, `snippets` or `settings` to open those views; **Esc** or **Backspace** on an empty search goes back.
4. Right-click any app to add it to your favorites.

The footer always shows what Enter will do. The tray icon has Open, Refresh and Quit.

## Configuration

Open **smolcast settings** from the palette, or edit `%APPDATA%\smolcast\config.toml` directly. The settings form preserves your comments.

| Setting | Default | What it does |
| --- | --- | --- |
| `summon_hotkey` | `control+alt+Space` | Global hotkey that opens the palette |
| `start_at_login` | `false` | Start smolcast when you sign in |
| `palette_size` | `medium` | `small`, `medium` or `large` window; larger shows more results |
| `text_size` | `medium` | `small`, `medium` or `large` text |
| `keep_state_secs` | `30` | How long a dismissed palette keeps your search and drafts (`0` = never) |
| `search_engine` | `google` | `google`, `bing`, `duckduckgo`, `brave`, `ecosia`, `startpage`, `kagi` or `custom` |
| `web_search_url` | Google | Search URL for `custom`; `%s` is replaced by the query |
| `clipboard_capture` | `true` | Record clipboard history |
| `clipboard_source` | `auto` | `auto` (Windows history when it is on), `windows`, or `smolcast` |
| `clipboard_persist` | `true` | Keep smolcast's history across restarts, encrypted |
| `clipboard_max_entries` | `100` | History length |
| `clipboard_max_item_bytes` / `clipboard_max_total_bytes` | 8 MiB / 64 MiB | Size limits |

Scripts and snippets live in the same file:

```toml
[scripts.open-project]
name = "Open project"
interpreter = "pwsh.exe"
args = ["-NoLogo", "-NoProfile", "-File", "C:\\Tools\\open-project.ps1"]
cwd = "C:\\Tools"
hotkey = "control+alt+KeyP"   # optional

[snippets.signature]
name = "Email signature"
keyword = ";sig"
text = "Best,\nMe"
```

Scripts run exactly the interpreter and arguments you write: nothing is interpolated from your search.

## Privacy

Nothing leaves your computer. smolcast has no network code except opening a web search in your browser when you choose one. Clipboard contents never go into logs, and smolcast's own clipboard history is encrypted for your Windows account. It honors apps that mark clipboard data as excluded from history, but it cannot detect every password.

## Building from source

You need [Task](https://taskfile.dev/docs/installation). Then:

```powershell
task up             # installs the pinned Rust toolchain and Windows build tools
task build-release  # target\x86_64-pc-windows-msvc\release\smolcast.exe
```

It also cross-compiles from Linux or WSL (`task up` there installs MinGW). See [docs/techContext.md](docs/techContext.md) for every command and the WSL workflow.

## Contributing

Issues and pull requests are welcome. Start with [docs/activeContext.md](docs/activeContext.md) for the current state and [docs/systemPatterns.md](docs/systemPatterns.md) for the architecture. Design choices and their reasons are in [docs/decisions/](docs/decisions/), and the visual rules are in [docs/DESIGN.md](docs/DESIGN.md).

> [!IMPORTANT]
> smolcast stays small on purpose. Features that need an extension system, a browser engine, a background service, or the network are out of scope ([docs/projectbrief.md](docs/projectbrief.md)).

## License

[MIT](LICENSE)
