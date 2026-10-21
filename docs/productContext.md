# Product context

## Why smolcast exists

Raycast-style launchers on Windows carry a browser engine and a script runtime
to host extensions. Most people use a handful of built-in features: launching
apps, clipboard history, emoji, a few scripts and snippets. smolcast does
those natively, in one small executable that is idle when hidden.

## How it should feel

- **Instant.** One hotkey summons the palette on the monitor under the cursor;
  typing searches immediately; Enter does the obvious thing.
- **Raycast-like.** A large frameless search field, a soft gray selection,
  section headers, and a footer that names the main action and its keys.
  `DESIGN.md` is the visual source of truth.
- **Forgiving.** An accidental dismiss keeps the palette's state for a short
  while (`keep_state_secs`, default 30 s); a completed action starts fresh.
- **Honest.** Anything still running says so ("Indexing apps…", "Pasting…")
  instead of looking frozen. Failures are short notices, never dialogs.
- **Alive, never slow.** Short animations (the palette settling in, views
  sliding, the selection gliding) add warmth, but none of them delays input:
  typing works from the first frame. Reduced motion keeps only fades.
- **Private.** Clipboard contents and script text never leave the device or
  enter logs.

## What each feature does for the user

| Feature | User-facing behavior |
| --- | --- |
| Launcher | Fuzzy search over Start Menu shortcuts and packaged apps, ranked by match, frequency and recency. Real program icons. Up to 5 favorites lead the empty query (Ctrl+Shift+F or right-click to pin; Ctrl+Shift+Up/Down to reorder). Broken Start Menu shortcuts are listed with an offer to move them to the Recycle Bin. |
| Calculator | "2^10", "40cm to in", "100f to c", "3 GiB as MB" show a card with the input and result; Enter pastes the result. No currencies (they need the network). |
| Web search | When nothing matches, "Search Google for …" opens the default browser; the engine is a setting (major engines or a custom URL). |
| Clipboard History | Text and images, searchable, with a preview; Enter pastes into the app you came from. Windows clipboard history when it is on, otherwise smolcast's own, encrypted and optionally persisted. |
| Emoji | Searchable grid with color glyphs; pastes the exact sequence. |
| Scripts | Named commands from `config.toml`, run from the palette or their own hotkey. |
| Snippets | Saved text searched by name or keyword and pasted; created and edited in the palette. |
| Settings | "smolcast settings" edits hotkey, startup, window and text size, reduced motion, keep-state time, web search engine, and clipboard options, preserving hand-written comments in `config.toml`. |
| About | "About smolcast" (also found as "Credits") shows the version, the author, and what smolcast is built with. |

## Where user data lives

- `%APPDATA%\smolcast\config.toml`: settings, scripts, snippets
  (hand-editable; the form preserves comments).
- `%APPDATA%\smolcast\favorites.json`: pinned entries.
- `%LOCALAPPDATA%\smolcast\usage.json`: launch counts (a cache).
- `%LOCALAPPDATA%\smolcast\clipboard\`: smolcast's clipboard history,
  DPAPI-encrypted per user, when persistence is on.
