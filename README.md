# smolcast

**A small, fully native command palette for Windows 11. One hotkey for your apps, files, clipboard, emoji, scripts and snippets, in a single executable that sits idle until you need it.**

smolcast is a Windows take on [Tinycast](https://github.com/abue-ammar/tinycast) and Raycast's core features, written in Rust with egui and the Windows APIs. No Electron, no WebView, no Node runtime, no extensions, no AI, and no telemetry: about a 7 MB exe that imports only Windows system DLLs and uses no CPU while hidden.

## Features

- **App launcher**: fuzzy search across Start Menu shortcuts, Store apps and installed programs that add no shortcut (many games and game launchers), ranked by how often and how recently you use them, with each program's real icon.
- **Favorites**: pin up to 5 entries to the top of the list (Ctrl+Shift+F), reorder them with Ctrl+Shift+Up/Down, and see them ranked higher when you search.
- **Calculator and conversions**: type `2^10`, `40cm to in`, `100f to c` or `3 GiB as MB` and the answer appears inline. Enter pastes it. Length, weight, volume, temperature, time, data, area, speed and angles, all offline.
- **Web search fallback**: every search ends with a row to search Google, Bing, DuckDuckGo, Brave, Ecosia, Startpage, Kagi or your own URL in your default browser.
- **File search**: find files and folders by name through the Windows Search index, with no index of smolcast's own. Type **Search Files**, or pick the "Search Files for …" row at the end of any search. Enter opens, Ctrl+Enter shows the item in Explorer.
- **System commands**: Lock Screen, Sleep, Restart, Shut Down, Sign Out, Empty Recycle Bin, Toggle Dark Mode, Toggle Mute and Turn Off Display. The ones that close your apps or delete files ask for a second Enter.
- **Quicklinks**: open a URL, file or folder by name, or type a query after its keyword (`gh rust egui`).
- **Clipboard history**: text and images, searchable, with a preview, pasted straight back into the app you came from. Uses Windows clipboard history (Win+V) when it is on, or smolcast's own, encrypted on disk.
- **Emoji picker**: searchable grid of every Unicode 17.0 emoji, in color, pasted exactly.
- **Scripts**: track a script file on disk, then run it from the search or its own hotkey, with saved arguments or ones you type for that run.
- **Snippets**: save text you paste often, find it by name or keyword, and edit it right in the palette.
- **Feels native**: a calm look of its own in Raycast's shape, Mica backdrop, rounded corners, light and dark themes, opens on the monitor under your cursor.
- **Lively, never slow**: the palette settles in, views slide, the selection glides, and nothing waits on an animation. Reduced motion follows Windows' Animation effects setting or your choice.
- **Forgiving**: dismiss by accident and the palette comes back as you left it for 30 seconds.
- **Tidy**: offers to move broken Start Menu shortcuts to the Recycle Bin.
- **Self-updating**: checks for a new release once a day and installs it in place when you say so.

## Install

Download `smolcast-<version>-x86_64.exe` from the latest [release](https://github.com/Archetypically/smolcast/releases/latest), or build it from source (below). Put it in a folder you can write to, such as `%LOCALAPPDATA%\Programs\smolcast`, rename it `smolcast.exe` if you like, and run it. Turn on **Start smolcast when you sign in** in its settings to keep it around.

smolcast runs as a normal user and needs no installer, admin rights, or runtime. Releases are not code-signed yet, so Windows SmartScreen may warn about the first download; updates installed from inside smolcast come only from this repository's GitHub releases and are checked against the release's SHA-256 checksum, which catches a corrupted download but is not a signature. The [code signing policy](https://smolcast.evanlee.engineer/code-signing) says how releases will be signed and how to check one.

## Using it

1. Press **Ctrl+Alt+Space** (change it in settings).
2. Type to search; **Up/Down** to choose; **Enter** to open, paste, or run.
3. Type `clipboard`, `emoji`, `snippets`, `files` or `settings` to open those views; **Esc** or **Backspace** on an empty search goes back.
4. Right-click any app to add it to your favorites.

The footer always shows what Enter will do. The tray icon has Open, Refresh and Quit. **About smolcast** shows the version and credits, **Check for Updates** asks for a new release right away, and **Report a Bug** opens a GitHub issue filled in with your version and Windows build (and, if you tick it, the last crash report).

## Configuration

Open **smolcast settings** from the palette, or edit `%APPDATA%\smolcast\config.toml` directly. The settings form preserves your comments.

| Setting | Default | What it does |
| --- | --- | --- |
| `summon_hotkey` | `control+alt+Space` | Global hotkey that opens the palette |
| `start_at_login` | `false` | Start smolcast when you sign in |
| `check_for_updates` | `true` | Check GitHub once a day for a new release |
| `palette_size` | `medium` | `small`, `medium` or `large` window; larger shows more results |
| `text_size` | `medium` | `small`, `medium` or `large` text |
| `reduce_motion` | `system` | `system` (follow Windows' Animation effects), `on` (fades only, no sliding or zooming) or `off` |
| `keep_state_secs` | `30` | How long a dismissed palette keeps your search and drafts (`0` = never) |
| `search_engine` | `google` | `google`, `bing`, `duckduckgo`, `brave`, `ecosia`, `startpage`, `kagi` or `custom` |
| `web_search_url` | Google | Search URL for `custom`; `%s` is replaced by the query |
| `clipboard_capture` | `true` | Record clipboard history |
| `clipboard_source` | `auto` | `auto` (Windows history when it is on), `windows`, or `smolcast` |
| `clipboard_persist` | `true` | Keep smolcast's history across restarts, encrypted |
| `clipboard_max_age_days` | `0` | Forget items older than this many days (`0` = keep until the limits push them out) |
| `clipboard_max_entries` | `100` | History length |
| `clipboard_max_item_bytes` / `clipboard_max_total_bytes` | 8 MiB / 64 MiB | Size limits |

Scripts, snippets and quicklinks live in the same file:

```toml
[scripts.deploy]
name = "Deploy"
path = 'C:\Tools\deploy.ps1'
args = ["-Env", "prod"]        # optional
hotkey = "control+alt+KeyD"    # optional

[snippets.signature]
name = "Email signature"
keyword = ";sig"
text = "Best,\nMe"

[quicklinks.gh]
name = "Search GitHub"
url = "https://github.com/search?q={query}"   # type "gh rust egui"

[quicklinks.downloads]
name = "Downloads"
url = 'C:\Users\me\Downloads'
```

You rarely need to write `[scripts]` by hand: **Track Script File** picks a `.ps1`, `.cmd`, `.bat`, `.py`, `.pyw`, `.js`, `.mjs` or `.exe`, with optional arguments and a hotkey. smolcast runs it with the program for its type (Windows PowerShell for `.ps1`, under the RemoteSigned policy, so a downloaded script needs **Unblock-File** first; Python, Node.js; batch files and programs run themselves) in the file's folder. **Manage Scripts** lists them: **Enter** runs, **Tab** runs with extra arguments you type (quote one that has spaces), **Ctrl+E** edits and **Shift+Delete** untracks; untracking removes the entry and never touches the file. A hand-written entry with `interpreter`, `args` and `cwd` instead of `path` runs that command. Nothing from your search is passed to a script; only arguments you type into **Run with Arguments** are, each as its own argument, never through a shell. A quicklink's id is its keyword: type it, a space, then your query, and `{query}` in its URL is replaced (URL-encoded, or as typed for a folder path). Choose a quicklink without a query, or press Tab on it, to fill in its keyword.

## Privacy

Your data stays on your computer. The only request smolcast makes on its own is a daily check of this project's GitHub releases (turn it off with `check_for_updates`); it sends nothing about you beyond smolcast's version in its user agent. Web searches and quicklinks open in your browser only when you choose them, and file search uses the Windows Search index on your PC. Clipboard contents never go into logs, and smolcast's own clipboard history is encrypted for your Windows account. It honors apps that mark clipboard data as excluded from history, but it cannot detect every password.

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
> smolcast stays small on purpose. Features that need an extension system, a browser engine, a background service, or the network (beyond the update check) are out of scope ([docs/projectbrief.md](docs/projectbrief.md)).

## License

[MIT](LICENSE)
