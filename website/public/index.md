# The smol command palette for Windows.

Apps, files, clipboard, emoji, scripts and snippets behind one hotkey, in a free 7 MB app that sits idle until you need it.

## One palette for the things you reach for all day.

Press `Ctrl`+`Alt`+`Space`, type a few letters, press `Enter`. The footer always says what Enter will do.

### App launcher

Fuzzy search over Start Menu shortcuts, Store apps and installed programs that add no shortcut, ranked by how often and how recently you use them. Pin up to five favorites.

### Calculator and conversions

`2^10`, `100f to c`, `3 GiB as MB`. The answer appears as you type and Enter pastes it. Length, weight, volume, temperature, time, data, area, speed and angles, all offline.

### Clipboard history

Text and images, searchable, with a preview, pasted straight back into the app you came from. Uses Windows clipboard history when it is on, or its own, encrypted on disk.

### Emoji picker

A searchable grid of every Unicode 17.0 emoji, in color, pasted exactly as chosen, skin tones and all.

### File search

Find files and folders by name through the Windows Search index, with no index of smolcast’s own. Enter opens; `Ctrl`+`Enter` shows the item in Explorer.

### System commands

Lock, sleep, restart, shut down, sign out, empty the Recycle Bin, toggle dark mode or mute, turn off the display. The ones that close apps or delete files ask for a second Enter.

### Quicklinks and web search

Open a URL, file or folder by name, or type a query after its keyword. Every search ends with a row for the engine you pick: Google, Bing, DuckDuckGo, Brave, Ecosia, Startpage, Kagi or your own URL.

### Scripts and snippets

Name a command in the config and run it from the palette or its own hotkey. Save text you paste often, find it by name or keyword, and edit it right in the palette.

### Also included

- **Favorites.** Pin five entries, reorder them with `Ctrl`+`Shift`+`↑` or `↓`, and they rank higher as you type.
- **Hide entries.** Take anything out of the list, and bring it back from Manage Hidden Entries.
- **Forgiving.** Dismiss by accident and it comes back as you left it for 30 seconds.
- **Self-updating.** Checks for a release once a day, installs in place when you say so.
- **Feels native.** Mica backdrop, rounded corners, light and dark, opens on the monitor under your cursor.
- **Quick motion.** Views slide and the selection glides, and nothing waits on an animation.
- **Tidy.** Offers to move broken Start Menu shortcuts to the Recycle Bin.

## Quick, and it never makes you wait for it.

Motion in smolcast is short, interruptible and paint-only: the palette settles in, views slide, the selection glides, and your next key always lands. It follows Windows’ Animation effects setting.

A simulation drawn to smolcast’s own measurements, labels and results, in real time: a conversion and an emoji pasted into a notes window, a paste from clipboard history, and the web and file fallbacks. Captions and key caps are added for the viewer. App icons are stand-ins. Music is original.

## Built for the keyboard. The mouse works too.

Try the keys here: the caps press down with yours.

`Ctrl`+`Alt`+`Space`

Open the palette (change it in settings)

`↑` `↓`

Choose a result

`Enter`

Open, paste or run

`Tab`

Fill in a quicklink’s keyword

`Ctrl`+`Enter`

Show a file in Explorer

`Ctrl`+`Shift`+`F`

Add to or remove from favorites

`Ctrl`+`Shift`+`H`

Hide from the list

`Esc`

Go back, or dismiss

`Backspace`

Go back from an empty search

## No Electron, no WebView, no Node runtime.

smolcast is written in Rust with egui and the Windows APIs. It imports only Windows system DLLs and stays out of the way until you call it.

See how smolcast compares with [Raycast for Windows](https://smolcast.evanlee.engineer/compare/raycast), [PowerToys Command Palette](https://smolcast.evanlee.engineer/compare/powertoys-command-palette) and [Flow Launcher](https://smolcast.evanlee.engineer/compare/flow-launcher), or [every Windows launcher side by side](https://smolcast.evanlee.engineer/compare/).

- **7 MB** One executable. No installer, admin rights or runtime.
- **0%** CPU while hidden. It wakes on the hotkey and nothing else.
- **0** Extensions, accounts, AI features or telemetry.
- **MIT** Open source. Read it, build it, change it.

## Your data stays on your computer.

The only request smolcast makes on its own is a daily check of its GitHub releases, which sends nothing about you beyond smolcast’s version. Turn it off with `check_for_updates`.

Web searches and quicklinks open in your browser only when you choose them, and file search uses the Windows Search index on your PC. Clipboard contents never go into logs, smolcast’s own history is encrypted for your Windows account, and it honors apps that mark clipboard data as excluded from history.

[Read the privacy policy](https://smolcast.evanlee.engineer/privacy) for every file it keeps and what this site records.

## A settings form, or one TOML file.

Open **smolcast settings** from the palette, or edit `%APPDATA%\smolcast\config.toml`. The form keeps your comments.

```
# Open the palette with Ctrl+Alt+Space
summon_hotkey = "control+alt+Space"
search_engine = "duckduckgo"
palette_size  = "medium"

[scripts.open-project]
name        = "Open project"
interpreter = "pwsh.exe"
args        = ["-NoProfile", "-File", "C:\\Tools\\open.ps1"]
hotkey      = "control+alt+KeyP"

[snippets.signature]
name    = "Email signature"
keyword = ";sig"
text    = "Best,\nMe"

[quicklinks.gh]
name = "Search GitHub"
url  = "https://github.com/search?q={query}"
```

## Download it, run it, press the hotkey.

[Download for Windows](https://github.com/Archetypically/smolcast/releases/latest)

- Windows 11, x64
- About 7 MB
- Free

1.  Put `smolcast-<version>-x86_64.exe` in a folder you can write to, such as `%LOCALAPPDATA%\Programs\smolcast`, and run it.
2.  Press `Ctrl`+`Alt`+`Space` and start typing.
3.  Turn on **Start smolcast when you sign in** in its settings to keep it around.

 (Releases are not code-signed yet, so SmartScreen may warn about the first download. Updates installed from inside smolcast are checked against the release’s SHA-256 checksum. Read the [code signing policy](https://smolcast.evanlee.engineer/code-signing).)

### Build from source

Install [Task](https://taskfile.dev/docs/installation), then run these in PowerShell. It also cross-compiles from Linux or WSL.

```
git clone https://github.com/Archetypically/smolcast
cd smolcast
task up
task build-release
```

---

The markdown version of https://smolcast.evanlee.engineer/
