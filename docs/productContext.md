# Product context

## Why smolcast exists

Raycast-style launchers on Windows carry a browser engine and a script runtime
to host extensions. Most people use a handful of built-in features: launching
apps, clipboard history, emoji, a few scripts and snippets. smolcast does
those natively, in one small executable that is idle when hidden.

## How it should feel

- **Instant.** One hotkey summons the palette on the monitor under the cursor;
  typing searches immediately; Enter does the obvious thing.
- **Familiar, with its own flavor.** Raycast's shape (a large frameless
  search field, a soft gray selection, section headers, a footer that names
  the main action and its keys), with a red selection pip, a search glyph on
  the icon column, and edges that dissolve. `DESIGN.md` is the visual source
  of truth.
- **Forgiving.** An accidental dismiss keeps the palette's state for a short
  while (`keep_state_secs`, default 30 s); a completed action starts fresh.
- **Honest.** Anything still running says so ("Indexing apps…", "Pasting…")
  instead of looking frozen. Failures are short notices, never dialogs.
- **Alive, never slow.** Short animations (the palette settling in, views
  sliding, the selection gliding) add warmth, but none of them delays input:
  typing works from the first frame. Reduced motion keeps only fades.
- **Private.** Clipboard contents, file searches and script text never leave
  the device or enter logs. The only request smolcast makes on its own is the
  daily update check, which can be turned off.

## What each feature does for the user

| Feature | User-facing behavior |
| --- | --- |
| Launcher | Fuzzy search over Start Menu shortcuts, packaged apps and installed programs without a shortcut (Uninstall registry rows, filtered; decision 027), ranked by match, frequency and recency. Real program icons. Up to 5 favorites lead the empty query and rank higher while searching (Ctrl+Shift+F or right-click to pin; Ctrl+Shift+Up/Down to reorder). Broken Start Menu shortcuts are listed with an offer to move them to the Recycle Bin. |
| Calculator | "2^10", "40cm to in", "100f to c", "3 GiB as MB" show a card with the input and result; Enter pastes the result. No currencies (they need the network). |
| Web search | Every search ends with "Search Google for …", which opens the default browser; the engine is a setting (major engines or a custom URL). |
| System commands | "Lock Screen", "Sleep", "Restart", "Shut Down", "Sign Out", "Empty Recycle Bin", "Toggle Dark Mode", "Toggle Mute", "Turn Off Display". Restart, shut down, sign out and emptying the bin ask for a second Enter. |
| Quicklinks | Named URLs, files or folders from `config.toml`. One with `{query}` in its URL takes text after its id: `gh rust egui` searches GitHub. |
| File search | "Search Files" (or the "Search Files for …" row at the end of any search) lists file and folder names from the Windows Search index. Enter opens, Ctrl+Enter shows the item in Explorer. Only indexed locations are found. |
| Updates | Once a day smolcast asks GitHub whether a newer release exists; if so, a line under the root list offers Install and Restart, What's New, or Skip. "Check for Updates" asks now. |
| Clipboard History | Text and images, searchable, with a preview; Enter pastes into the app you came from. Windows clipboard history when it is on, otherwise smolcast's own, encrypted and optionally persisted. |
| Emoji | Searchable grid with color glyphs; pastes the exact sequence. |
| Scripts | Quick access to script files on disk. "Track Script File" picks a file (with optional saved arguments and a hotkey); its type picks the program. "Manage Scripts" runs (Enter), runs with extra typed arguments (Tab), edits and untracks them; tracked scripts also appear in the root search. Stored in `config.toml`, comments preserved; untracking never touches the file. |
| Snippets | Saved text searched by name or keyword and pasted; created and edited in the palette. |
| Settings | "smolcast settings" edits hotkey, startup, update checks, window and text size, reduced motion, keep-state time, web search engine, and clipboard options, preserving hand-written comments in `config.toml`. |
| About | "About smolcast" (also found as "Credits") shows the version, the author, and what smolcast is built with. |

## Where user data lives

- `%APPDATA%\smolcast\config.toml`: settings, scripts, snippets, quicklinks
  (hand-editable; the form preserves comments).
- `%APPDATA%\smolcast\favorites.json`: pinned entries.
- `%LOCALAPPDATA%\smolcast\usage.json`: launch counts (a cache).
- `%LOCALAPPDATA%\smolcast\clipboard\`: smolcast's clipboard history,
  DPAPI-encrypted per user, when persistence is on.
- `%LOCALAPPDATA%\smolcast\update.json`: when updates were last checked, and
  a skipped version.
