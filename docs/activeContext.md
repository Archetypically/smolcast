# Active context

Read this first each session, by section. Budget: 250 lines. Architecture
contracts are in `systemPatterns.md` (cited as "plan §N"), decisions in
`decisions/`, visual rules in `DESIGN.md`.

## Where the project is

All seven milestones are implemented (§12). `main` holds everything but the
visual-language branch. Windows 11 is the acceptance environment; Linux runs
library tests and cross-links only.

| Milestone | State |
| --- | --- |
| 1 Foundation, lifetime, palette | Verified on Windows (size 5.9 MB, 0 ms hidden CPU, ~76 MiB, summon ~102 ms before the fast path). |
| 2 App launcher | Verified; one gate check pending (below). |
| 3 Clipboard + paste-back | Verified (text, image, persistence). |
| 4 Emoji | Verified; color glyphs via DirectWrite (decision 006). |
| 5 Scripts | Implemented. |
| 6 Snippets, tier 1 | Implemented; Windows checklist pending (§12.6). |
| 7 Polish | Placement, click-away, login item, appearance, settings, Raycast look verified in parts; the 2026-10-21 batches are unverified. |

## 2026-10-21 batches, on main, unverified on Windows

Decisions 011–018 (launcher features, motion and polish); details in
`git log`. Deferred review items: Esc drops unsaved Settings edits; a failed
thumbnail thread leaves "Loading preview…"; a failed clipboard write can
leave the clipboard empty.

## Launcher commands, files and updates (2026-10-22), on main, unverified

Never run on Windows. GNU release exe 7.82 → 7.96 MB; new imports winhttp,
powrprof, propsys (all system DLLs).

- **System commands** (§6.5): Lock Screen, Sleep, Restart, Shut Down, Sign
  Out, Empty Recycle Bin, Toggle Dark Mode, Toggle Mute, Turn Off Display, as
  root rows typed "System". Restart, Shut Down, Sign Out and Empty Recycle Bin
  need a second Enter.
- **Quicklinks** (§10.7, decision 021): `[quicklinks.<id>]` with `name` and
  `url`; `{query}` takes text after the id (`gh rust egui`). Config-only.
- **File search** (§6.6, decision 020): the Windows Search index over OLE DB,
  no index of ours. "Search Files" view (debounced, Enter opens, Ctrl+Enter
  reveals) and a "Search Files for …" row after any query. The user chose
  that fallback row over omnisearch, so apps are not drowned by documents.
- **Fallback section** (decision 022, supersedes 014's "only when nothing
  matches"): every non-calculation, non-quicklink query ends with the web and
  file search rows under a `Search for "query"` header (no header when they
  are the only rows).
- **Self-update** (§13, decision 019): daily check of `releases/latest`
  (`check_for_updates`, on), "Check for Updates" command, offer row under the
  root list, download over WinHTTP, size + SHA-256 check, rename swap,
  relaunch with `--wait-pid`. Unsigned for now (user: plumbing first).
  `.github/workflows/release.yml` publishes on a `v*` tag.
- **Native icons** (decision 023): built-in commands, system actions,
  quicklinks and the files fallback draw Windows' icon font (Segoe Fluent
  Icons / MDL2, loaded from `%WINDIR%\Fonts`), emoji only as fallback;
  Search Files rows show Explorer's icon per item.

## App icon and file properties (2026-10-22), on main, unverified

Decision 026: new "s." logo (`assets/smolcast.svg` →
`scripts/generate_icon.py` → `assets/smolcast.ico`; user picked it from six
directions),
embedded with VERSIONINFO by `build.rs` (`winresource`, build dependency);
the tray loads the embedded icon. Author, `homepage`
(evanlee.engineer until smolcast.evanlee.engineer is live) and repository
are in `Cargo.toml`. `release.yml` checks the embedded version. Resources
verified in the GNU exe with `llvm-readobj`; MSVC `rc.exe` path untested.

## Installed programs (2026-10-22), on main, unverified

Decision 027, plan §6.1: the scan adds filtered Uninstall registry rows no
Start Menu shortcut covers (Raycast found Overwatch and Battle.net; neither
has a shortcut). A dry run here adds Overwatch, Battle.net, 1Password and the
Windows 11 Installation Assistant. Linted for the GNU target only.

## Own visual language (2026-10-22), branch, unverified

Decision 028 (supersedes 010): search glyph on the icon column, red
selection pip, dissolving dividers, dimmed edge rows (`list::RowState`),
key caps with a lower lip. Not seen on Windows yet: check every view in both
themes, at 100/150/200%, reduced motion and high contrast; divider crispness
at 150%.

## Desktop smoke test in CI (2026-10-22), on main

Decision 029; see techContext CI. If slow, make it a release `example`.

## Website (2026-10-22), branch, not deployed

Decision 030: `website/`, a static one-pager for Cloudflare Pages (`task
web:*`), whose hero is a working recreation of the palette. Domain
`smolcast.evanlee.engineer` is hard-coded in `public/` (canonical, OG,
JSON-LD, sitemap, robots, llms.txt): grep and replace it on a move. The demo
film is a faithful simulation (decision 031, `task web:film`); keep it in step
with the app. It now carries restrained motion graphics (backdrop stippled
with s. marks, drifting; chapter captions, camera lean, title card) and loops
seamlessly at 36 s in three cuts: wide for the page, square and vertical in
`media/out/` for social posts. To do: check touch scrolling on a real phone;
create the Pages project and DNS, deploy, then point `Cargo.toml`
`homepage` and the README at it.

## Pending Windows checks

- Installed programs: Overwatch and Battle.net appear with icons and launch
  (Overwatch via Battle.net); no driver/runtime rows; no Edge/SteelSeries
  duplicates.
- App icon: Explorer icon at every view size, Properties > Details fields,
  taskbar/Alt+Tab where shown, tray icon sharp at 100/150/200%.

- Effect outbox: paste-back from clipboard, emoji and snippets (pastes, and
  the failure notices still appear); launch, recycle, rescan from the tray;
  icons, glyphs, thumbnails and file icons load; usage and favorites persist
  across restart; settings and snippet saves report "Saved." and reload; a
  hotkey conflict's trial key; Check for Updates.

- 2026-10-22 branch: each system action (confirm flow; Restart/Shut Down
  with an unsaved Notepad still prompts; dark mode flips apps and taskbar;
  mute shows the flyout; display off stays off), the new glyphs render;
  quicklinks with and without `{query}`, Tab fill-in, a folder path target;
  the fallback header and rows scroll correctly in every window size; the
  icon-font glyphs render at 100/150/200% in light, dark and high contrast
  (Sign Out's SwitchUser glyph reads right); file icons load in Search Files;
  Search Files with the service on and off, typing fast sends one query,
  stale entries hidden, Ctrl+Enter selects in Explorer; update check against
  a real release (needs the public repo), Skip, a failed download, an
  unwritable folder, the swap and relaunch (old exe gone after restart, Run
  key still valid), `check_for_updates = false` stops automatic checks.

- Motion batch: summon feel with and without the fast path (no stale frame,
  animation visible from its start), Esc and click-away fades, re-summon
  during the fade (keeps destination: paste still works), paste/launch still
  instant, view slides, glide in each list and the emoji grid, typing during
  the appear animation, reduced motion via the setting and via Windows'
  Animation effects switch, high-contrast theme, Tab focus ring in Settings,
  About page layout at every window and text size, idle CPU 0 after
  animations end.

- 2026-10-21 batch: icon sharpness at 100/150/200%; spacing in every view;
  window/text size switching; keep-state and fresh start after launch/paste;
  calculator card and Enter paste; web search opens the browser with each
  engine; broken-shortcut recycle including the all-users elevation prompt;
  favorites add/remove/reorder, the limit notice, persistence across restart;
  wheel and trackpad scrolling in the launcher, clipboard, snippets and emoji.
- M2: remove an indexed shortcut before commit → error notice, no usage bump.
- M1: Alt+F4 hides (never explicitly confirmed).
- M6 tier 1 checklist (§12.6), including the in-palette editor.
- Responsiveness pass: summon latency re-measure and no stale-frame flashes.

## Next, in order

1. User's Windows pass over the batches above; fix findings.
2. **Before making the GitHub repo public**: history was rewritten on
   2026-09-25, but GitHub still serves the pre-rewrite commits. Delete and
   recreate the repository (or have GitHub Support purge it), then push
   `main`. Also add release notes: license notices, checksum, first-run
   behavior, and the clean-account check (§12.7 gate). Updates only work
   once the repo is public and a `v*` tag has published a release.
3. **Sign builds** (decision 019): Authenticode (e.g. Azure Trusted Signing)
   for SmartScreen, and/or an Ed25519 key embedded in the exe, verified
   before the update swap. Until then the checksum only catches corruption.
4. Decide the deferred review items above. `old-review-fixes` (on main) moved
   every view onto `RequestCache` and `ListCursor` (decision 024). Windows
   check: icons, file icons, emoji and thumbnails stay sharp across a
   150%→100% monitor move; scrolling and selection in every list.

## Open questions and parked ideas

- **Dictionary**: tabled by the user (2026-10-21). Windows has no definitions
  API (only ISpellChecker). Options if revived: "define <word>" via web
  search, bundled WordNet (+10–30 MB), or an online API.
- **Real engine logos** instead of monogram badges: needs bundled assets
  (licensing) or a network fetch.
- **Broken-shortcut "Ignore"** is in memory only; persist it if the offer
  nags across restarts.
- **Calculator Enter** pastes; Raycast copies. Revisit if the user wants copy.
- **Flag font**: Segoe UI Emoji has no flags; a COLR flag-only font (e.g.
  Twemoji, CC-BY/MIT with notice) costs ~1–2 MB. Also rank the Flags group
  above "flag in hole" for the bare query `flag`.
- **Snippet expansion** tier 2 stays unapproved (decision 009).
- **Theming**: shelved by the user (2026-10-22), "thinking about it for
  sure". Tinycast refuses a theme editor (its not-planned list). Sketch if
  revived: color-only themes as `%APPDATA%\smolcast\themes\<name>.toml`
  based on `dark` or `light`, overriding the palette tokens (background, text,
  secondary, selection, hover, control, divider, plus accent, warning, error,
  which are constants in `app/theme.rs` today); settings `appearance =
  system|dark|light` and `theme = "<name>"`; layout, spacing, fonts and radii
  stay fixed (DESIGN.md); high contrast always wins; background alpha decides
  how much Mica shows; hot reload through the config reload.
- **Tinycast gaps** (reviewed 2026-10-22, AI features ignored; Tinycast is
  AGPL, so ideas only, never code). Done on the branch: system commands,
  quicklinks, file search, self-update. Good fits not started: per-app
  hotkeys (focus/hide an app), app aliases and hiding apps, running apps
  with quit, Uninstall (Windows' uninstaller), clipboard pinning, paste as
  plain text, an app block list and type filter, snippet placeholders and
  arguments, script arguments and shown output, quicklink `{clipboard}` /
  `{date}` and an editor. Bigger: window switcher, notes, translations.
  Out: window management, layouts and rooms (non-goal), currency/crypto
  (network), Raycast extensions (non-goal), dictionary (tabled), calendar,
  Apple Shortcuts, menu-bar search, camera.
- **Screen readers**: eframe's `accesskit` feature would expose the palette
  to Narrator, but adds dependencies and size, and the painted rows need
  `widget_info` labels. Not started; needs the user's call.

## Standing rules

- Use the memory-bank and making-decisions skills (`.agents/skills/`);
  commit bank updates with the change they record.
- README is the public product page only. Never track machine-specific
  paths; they belong in `.env`.
- Develop and build in WSL; try builds via `task stage` (or `task install`
  for the everyday copy). Nothing is installed on the Windows system drive.
- No scripted input or screenshots on the user's desktop without asking;
  prefer manual checklists. CI's throwaway runner desktop is the exception
  (decision 029).
- Tests: default to the desktop smoke test (integration level); do not add
  unit tests unless asked.
- Edit repo files with the Edit/Write tools, not ad-hoc replace scripts.
- Cheap mechanical moves/renames/re-reference sweeps go to a Haiku subagent.

## Traps (read before touching the area)

- **eframe 0.36**: force-shows the root window after the first painted frame;
  `eframe_host::ui` re-hides it on the first pass. Invisible windows repaint
  at most every 100 ms, which the summon fast path works around (§2.1).
  `App` is `logic(ctx)` + `ui(ui)`; there is no `update`.
- **egui**: a new zoom factor applies next frame, and window sizes are in
  zoomed points; keyboard zoom is disabled. A focused TextEdit strokes
  `visuals.selection.stroke` (width set to 0). Modifier keys arrive as their
  own key events (`is_modifier_key` in hotkey recording).
- **windows-rs 0.62**: HWND is not `Send` (pass as `isize`); `CreateMutexW`
  returns Ok on ERROR_ALREADY_EXISTS (check `GetLastError`); pass `PCWSTR`
  from a NUL-terminated `Vec<u16>`.
- **tray-icon/muda**: event handlers are globals; setting one disables
  `receiver()`. Left-click opening uses `with_menu_on_left_click(false)`.
- **Shell icons**: `IShellItemImageFactory` with ICONONLY at sizes above an
  icon's largest frame returns that frame centered in a transparent canvas
  (`icon_scale::trim_padding`).
- **Clipboard**: `GetClipboardData` handles are borrowed; never free them.
  Every successful `OpenClipboard` is paired with `CloseClipboard` (§7.1).
- **Running exes** cannot be overwritten; `task stage`/`install` rename the
  old one aside.
- **rust-analyzer** needs `cargo.target = "x86_64-pc-windows-gnu"` in the
  user-level rust-analyzer config; a project-root `rust-analyzer.toml` cannot
  set it, and `.cargo/config.toml` `build.target` would break host tests.
- **windows-rs OLE DB**: `IRowset::GetNextRows` passes its slice as the
  provider's `HROW**`, with `cRows` = the slice length, so only a
  one-element slice pointing at one `HROW` is correct (`win/file_search.rs`).
  `PropVariantToStringAlloc` and `IPropertyStore::GetValue` need the
  `Win32_System_Variant` feature.
- **Updates**: the running exe is renamed, never overwritten; a relaunched
  build waits on `--wait-pid` before the single-instance mutex, or it would
  exit as a silent second launch (decision 003).
