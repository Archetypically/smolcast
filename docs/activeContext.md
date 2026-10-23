# Active context

Read this first each session, by section. Budget: 500 lines. Architecture
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

Never run on Windows. System commands (§6.5), quicklinks (§10.7, 021),
file search (§6.6, 020; the user chose a fallback row over omnisearch),
the fallback section (022), self-update (§13, 019; unsigned, user: plumbing
first) and icon-font glyphs (023). New imports winhttp, powrprof, propsys
(all system DLLs). Checks are under Pending Windows checks.

## Tracked scripts (2026-10-23), branch `script-editor`, unverified

Decision 032, plan §10.2–10.3: scripts are tracked files (`path`, saved
`args`, hotkey) resolved by extension at index time; Manage Scripts runs,
runs with typed arguments (Tab), edits and untracks; Track Script File
uses the Open dialog. Unit tests for `core::script_file` parsing only (the
user's one-time exception). Linted for the GNU target. Windows check: the
dialog stays in front and the palette does not dismiss while it is up;
focus returns to the form; `.ps1`, `.cmd` and `.py` with spaces and
Unicode in the path run in the file's folder; a `.cmd` given `a&b` gets
it as one argument; a recorded hotkey registers on save; Enter on a row
hides the palette after the spawn.

## App icon and file properties (2026-10-22), on main, unverified

Decision 026: "s." logo (`assets/smolcast.svg` → `scripts/generate_icon.py`
→ `.ico`) and VERSIONINFO embedded by `build.rs`. `homepage` is
evanlee.engineer until smolcast.evanlee.engineer is live. MSVC `rc.exe`
path untested.

## Own visual language (2026-10-22), branch, unverified

Decision 028 (supersedes 010): search glyph on the icon column, red
selection pip, dissolving dividers, dimmed edge rows (`list::RowState`),
key caps with a lower lip. Not seen on Windows yet: check every view in both
themes, at 100/150/200%, reduced motion and high contrast; divider crispness
at 150%.

## Website (2026-10-22), branch, not deployed

Decision 030: `website/`, a static site for Cloudflare Pages (`task
web:*`), whose hero is a working recreation of the palette. Since
2026-10-23 (branch `website-pages`, decision 033) it also has `/privacy`
and `/compare/` (hub plus Raycast, PowerToys Command Palette/Run, Flow
Launcher, Tinycast, Fluent Search, Listary, Wox, Alfred, Keypirinha, Ueli;
`task web:compare`). Chrome is stamped from `partials/`, colors come from
`theme.rs` (`task web:sync`; CI checks both). Comparison facts checked
2026-10-23; recheck when those products ship. Privacy contact: GitHub
issues and evanlee.engineer until the user names an email. Domain
`smolcast.evanlee.engineer` is hard-coded in `public/` (canonical, OG,
JSON-LD, sitemap, robots, llms.txt): grep and replace it on a move. The demo
film is a faithful simulation (decision 031, `task web:film`); keep it in step
with the app. It now carries restrained motion graphics (backdrop stippled
with s. marks, drifting; chapter captions, camera lean, title card), loops
seamlessly at 36 s in three cuts: wide for the page, square and vertical in
`media/out/` for social posts. To do: check touch scrolling on a real phone;
create the Pages project and DNS (turn off Cloudflare's AI-crawler
blocking and managed robots.txt), deploy, then point `Cargo.toml`
`homepage` and the README at it. Then backlinks (outbound links do not
build ours): AlternativeTo listings as an alternative to Raycast, PowerToys
Run / Command Palette, Flow Launcher and Wox; winget and Scoop manifests
with the homepage; awesome-list PRs (Windows apps, Rust GUI); Show HN,
r/Windows, r/rust, Product Hunt linking the site; the GitHub repo's website
field; a link from evanlee.engineer; a note to Tinycast's author.

## Full-repo review (2026-10-23), on main, unverified

Fixes are in `git log` (PR #17). Deferred, user's call: delete `.clip`
files only on definite bad data, not any DPAPI failure (§7.2); bounded joins on quit (§1.5); WinRT history
conversion blocks the clipboard thread; `%VAR%` quicklinks are URLs (§10.7);
path quicklinks take `..` in `{query}`; `SEE_MASK_FLAG_NO_UI` may hide the
Mark-of-the-Web prompt for File Search opens (check on Windows); DLL search
hardening (CI's Mesa `opengl32.dll` relies on the app folder); clipboard
history TTL. Windows check: script hotkey with the palette open, paste from
a password manager is not captured, Reveal and Edit Config still open.

## Code review leftovers (2026-10-23), unverified, not on this branch

Raised by the website branch's review, outside its diff; verify before
fixing: `win/process.rs:50` resolves only `powershell.exe` by full path, so
other bare interpreters (`python.exe`, `node.exe`) search the exe's folder
first; `app/launcher.rs:225` a late launch result from an earlier summon can
clear the new `launch_pending` (matched by AppId only);
`app/script_view.rs:681` `select_id` is never cleared after a failed or
cancelled save; `script_view.rs:792` `editor_ui` re-resolves the script
every frame (cost only).

## Pending Windows checks

- Installed programs (decision 027, linted for GNU only): Overwatch and
  Battle.net appear with icons and launch
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

- **Emoji tile vs Hallmark 30a** (floor, unwaivable): the home emoji tile
  shows real emoji as the picker's output; Hallmark reads them as emoji
  icons. User's call: keep, or draw them as Fluent emoji images.
- **Publishing via winget and the Microsoft Store** (user, 2026-10-23):
  not worked out yet (manifests, signing, Store listing). Both want the
  privacy URL and maybe a contact email (the page lists GitHub issues and
  evanlee.engineer); ask the user.
- **Dictionary**: tabled by the user (2026-10-21). Windows has no definitions
  API (only ISpellChecker). Options if revived: "define <word>" via web
  search, bundled WordNet (+10–30 MB), or an online API.
- **Real engine logos** instead of monogram badges: needs bundled assets
  (licensing) or a network fetch.
- **Calculator Enter** pastes; Raycast copies. Revisit if the user wants copy.
- **Broken-shortcut "Ignore"** is in memory only; persist it if the offer
  nags across restarts.
- **Flag font**: Segoe UI Emoji has no flags; a COLR flag-only font (e.g.
  Twemoji, CC-BY/MIT with notice) costs ~1–2 MB. Also rank the Flags group
  above "flag in hole" for the bare query `flag`.
- **Snippet expansion** tier 2 stays unapproved (decision 009).
- **Theming**: shelved by the user (2026-10-22), "thinking about it for
  sure". Sketch: color-only `%APPDATA%\smolcast\themes\<name>.toml` over
  `dark`/`light`, overriding the `app/theme.rs` color tokens; layout, fonts
  and radii fixed; high contrast wins; hot reload via config reload.
- **Tinycast gaps** (reviewed 2026-10-22, AI features ignored; Tinycast is
  AGPL, so ideas only, never code). Good fits not started: per-app hotkeys,
  app aliases/hiding, running apps with quit, Uninstall, clipboard pinning,
  paste as plain text, snippet placeholders, script arguments and shown
  output (Raycast's modes), quicklink `{clipboard}`/`{date}` and an editor.
  Bigger: window switcher, notes. Out: window management, network features,
  extensions, dictionary (tabled).
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
