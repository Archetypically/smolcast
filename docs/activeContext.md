# Active context

Read this first each session, by section. Budget: 500 lines. Architecture
contracts are in `systemPatterns.md` (cited as "plan §N"), decisions in
`decisions/`, visual rules in `DESIGN.md`.

## Where the project is

All seven milestones are implemented (§12) and on `main`. Windows 11 is
the acceptance environment; Linux runs library tests and cross-links only.

`Cargo.toml` is at `1.0.0-rc.1` (a semver prerelease: it sorts below
`1.0.0` and is never offered as an update). `main`'s history was rewritten
again on 2026-10-24 so every commit is signed by the maintainer; GitHub
still serves the pre-rewrite objects (see Next, 2).

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
`git log`. Their deferred review items were settled on 2026-10-23 (below).

## Launcher commands, files and updates (2026-10-22), on main, unverified

Never run on Windows. System commands (§6.5), quicklinks (§10.7, 021),
file search (§6.6, 020; the user chose a fallback row over omnisearch),
the fallback section (022), self-update (§13, 019; unsigned, user: plumbing
first) and icon-font glyphs (023). New imports winhttp, powrprof, propsys
(all system DLLs). Checks are under Pending Windows checks.

## Tracked scripts (2026-10-23), on main, unverified

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

## Microsoft Store MSIX (2026-10-25), unverified, not set up

Decision 040, plan §15: the release exe ships to the Store in an MSIX
(`scripts/package_msix.ps1`, `task msix`, manifest and logos in
`packaging/msix/`), with write virtualization off. Packaged, the exe has no
self-update and uses the manifest's startup task for sign-in. CI packages
every build; `release.yml`'s `store` job
submits release tags with `msstore` once `MSSTORE_PRODUCT_ID` is set. The
manifest carries the reserved name's Partner Center identity (Name,
Publisher, PublisherDisplayName). Setup steps: techContext "Microsoft Store".
Never run on Windows: makepri/makeappx in CI (first run of `ci.yml` checks
them), install via `Add-AppxPackage -Register`, Start menu name and icons at
100/150/200%, taskbar and Alt+Tab icon, the startup task on, off, and off in
Task Manager (the notice names it), a sign-in start stays in the tray,
no "Check for Updates" and the Settings line, a launched app and a script
writing a new AppData file and HKCU key that are visible outside smolcast,
config shared with the portable exe, paste-back, hotkeys, file search and
the all-users recycle prompt from the package.

## App icon and file properties (2026-10-22), on main, unverified

Decision 026: "s." logo (`assets/smolcast.svg` → `scripts/generate_icon.py`
→ `.ico`) and VERSIONINFO embedded by `build.rs`. `homepage` is
evanlee.engineer until smolcast.evanlee.engineer is live. MSVC `rc.exe`
path untested.

## Own visual language (2026-10-22), on main, unverified

Decision 028 (supersedes 010): search glyph on the icon column, red
selection pip, dissolving dividers, dimmed edge rows (`list::RowState`),
key caps with a lower lip. Not seen on Windows yet: check every view in both
themes, at 100/150/200%, reduced motion and high contrast; divider crispness
at 150%.

## Website (2026-10-22), on main, not deployed

Decision 030: `website/`, a static site for Cloudflare Pages (`task
web:*`), whose hero is a working recreation of the palette. Since
2026-10-23 (decision 033) it also has `/privacy`,
`/code-signing` (SignPath's required policy: roles, attribution, privacy
link; its "being set up" badge comes off once releases are signed) and
`/compare/` (hub plus Raycast, PowerToys Command Palette/Run, Flow
Launcher, Tinycast, Fluent Search, Listary, Wox, Alfred, Keypirinha, Ueli;
`task web:compare`). Chrome is stamped from `partials/`, colors come from
`theme.rs`, and every indexable page has a generated markdown twin at its
path plus `.md` (decision 036; `task web:sync`; CI checks all three).
Comparison facts checked 2026-10-23; recheck when those products ship. Privacy contact: GitHub
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

Fixes are in `git log` (PR #17). Its deferred items were settled on
2026-10-23 (below). Windows check: script hotkey with the palette open, paste from
a password manager is not captured, Reveal and Edit Config still open.

## Crash log, Report a Bug, review leftovers (2026-10-23), unverified

Decision 035, plan §14: a panic hook appends to `crash.log` in
`%LOCALAPPDATA%\smolcast\` (64 KiB, then `crash.old.log`); the next start
shows one notice; the "Report a Bug" command shows what a prefilled GitHub
issue carries, with the last crash report only when ticked. The privacy page
lists both. Review leftovers fixed: bare script interpreters no longer
resolve from smolcast's folder (§10.3); launch results match by request
number, not AppId; a failed script save clears `select_id`. Left as is:
`editor_ui` re-resolving the script each frame (a few small allocations
while the form is open). Windows check: a forced panic (none can be
triggered from the UI; use a debug build) writes the entry and the notice
appears once; the issue opens prefilled, with and without the crash report;
the Bug glyph renders; a `python.exe` dropped beside smolcast.exe is not
the one a `.py` script runs.

Deferred review items, settled by the user (2026-10-23): history files are
deleted only on definite bad data (§7.2); clipboard writes allocate before
emptying (§7.2); `clipboard_max_age_days`, off by default (§7.5); Quit waits
2 s for threads other than the state writer (§1.5); a leading `%VAR%\` in a
quicklink is a path, expanded at launch (§10.7); Search Files opens without
`SEE_MASK_FLAG_NO_UI` (§6.6); MSVC builds load imported DLLs from System32
only, with an `app-folder-dlls` feature for CI's Mesa (§0); Esc in Settings
asks before discarding (§5.4); a failed thumbnail thread shows "No preview".
Kept as is: WinRT history conversion on the clipboard thread (watch for lag)
and `..` in path quicklinks (the user typed it). Windows check: CI's smoke
test still renders with the feature; the release exe starts with no DLL
beside it; Quit with a launch stuck on an offline share exits within ~2 s;
a downloaded `.exe` opened from Search Files shows the security warning;
`%USERPROFILE%\Downloads\{query}` opens a folder; Esc twice in Settings;
an age limit drops old entries and their files.

## Palette on a manual start (2026-10-23), unverified

Decision 037, plan §2.1: a start by the user opens the palette instead of
flashing a blank frame; the Run entry now passes `--login` and stays in the
tray, as does an updater relaunch. Older flagless Run entries are rewritten
on the next start. Windows check: starting from the Start menu opens the
palette and Esc dismisses it; with `start_at_login` on, the Run value ends
in `--login` and signing in shows nothing; an update relaunch stays hidden.

## Web search globe glyph (2026-10-23), unverified

Decision 038: the web search fallback row is led by a globe glyph for every
engine; the monogram badge (`theme::engine_badge`, `SearchEngine::badge`) is
gone, and the website's quicklinks tile and the demo film's source match
(the published film in `public/media/` still shows the badge until it is
re-recorded with `task web:film`). Windows check: the row shows
the globe from Segoe Fluent Icons (🌐 without the font), distinct from the
Search Files magnifier below it.

## Favorites in search, hidden entries (2026-10-24), unverified

Decision 039, plan §6.3: favorites get a ranking bonus while searching;
Ctrl+Shift+H or "Hide" takes any entry but Manage Hidden Entries out of the
root list (`hidden.json`), and that command's view unhides. The desktop
smoke test hides and unhides its marker shortcut. Windows check: a favorite
outranks a comparable match ("pa" with Notepad pinned) but not a much better
one; hiding a favorite unpins it; a hidden quicklink's keyword does nothing;
the view's icons, "Not installed" rows, search, Enter and click; the unhide
glyph (U+ED1A) renders; persistence across restart.

## Interface font setting (2026-10-27), unverified

`font` in `[settings]` (plan §10.2, §12.7; `win/fonts.rs`): an installed
family name, resolved through DirectWrite to its regular face and placed
ahead of Segoe UI; empty or unknown uses Segoe UI. Settings > Palette > Font
lists the installed families, enumerated on a worker thread each time the
form opens. Never built or run on Windows yet. Windows
check: the list includes a per-user installed font; picking one restyles the
palette after Save without a restart; a `.ttc` family (e.g. Cambria) renders
its own face; a hand-typed unknown name shows "not installed" and falls back;
a CFF `.otf` and a variable font render; back to the default restores Segoe
UI; Settings opens instantly with many fonts installed ("listing fonts…"
shows briefly on the first open only), and a font installed while smolcast
runs appears on the next open.

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
3. **Sign builds** (decisions 019, 034): Authenticode through SignPath
   Foundation (user's choice, 2026-10-23; free for OSS, also covers winget
   and a Store exe installer). In order: the repository public; the site
   deployed with its `/code-signing` policy page (written 2026-10-23;
   SignPath also wants it linked from each release's notes); an application
   at signpath.org; SignPath's GitHub action in `release.yml` before the
   checksum step. The updater's `WinVerifyTrust` check pins the subject and
   smolcast's own version resource (plan §13, decision 034).
   Until then the checksum only catches corruption. Done (2026-10-23):
   releases only on `v<semver>` tags, and build provenance attestations,
   which start when the repo goes public (plan §13); then mention
   `gh attestation verify` in the README.
4. `old-review-fixes` (on main) moved every view onto `RequestCache` and `ListCursor` (decision 024). Windows
   check: icons, file icons, emoji and thumbnails stay sharp across a
   150%→100% monitor move; scrolling and selection in every list.
5. **Repository settings** (techContext CI, "Repository settings"): the
   `v*` tag ruleset, Actions defaults, push protection and private
   vulnerability reporting are unconfirmed; only `main`'s protection is
   known to exist. The split release workflow first runs at the next tag.
6. **Microsoft Store** (decision 040): the Windows checks above, the first
   submission by hand (it also confirms the manifest's identity matches
   Partner Center), then the Entra app, `microsoft-store` environment and
   `MSSTORE_PRODUCT_ID` (techContext "Microsoft Store"). The first release
   tag after that exercises the `store` job; check the submission in
   Partner Center. Once live, add the Store link to the README and the site.

## Open questions and parked ideas

- **Backlog from the user (2026-10-23)**, not designed yet:
  - *About links*: "About smolcast" exists (`app/about_view.rs`) but links
    nowhere; add the website and the repository.
  - *Donations via Polar (polar.sh)*: a "Support smolcast" link (About, the
    website, maybe README); the app only opens a URL, no payment code.
- **Emoji tile vs Hallmark 30a** (floor, unwaivable): the home emoji tile
  shows real emoji as the picker's output; Hallmark reads them as emoji
  icons. User's call: keep, or draw them as Fluent emoji images.
- **Publishing via winget** (user, 2026-10-23): not worked out yet
  (manifest, signing). The Store is decision 040. Both want the privacy URL
  and maybe a contact email (the page lists GitHub issues and
  evanlee.engineer); ask the user.
- **Dictionary**: tabled by the user (2026-10-21). Windows has no definitions
  API (only ISpellChecker). Options if revived: "define <word>" via web
  search, bundled WordNet (+10–30 MB), or an online API.
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
  app aliases, running apps with quit, Uninstall, clipboard pinning,
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
- Write the bank as it will read on `main` after the merge: what is true
  once the change lands, never "on this branch" or "not on this branch".
  Name a branch only for work that will stay unmerged.
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
