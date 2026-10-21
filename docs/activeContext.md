# Active context

Read this first each session, by section. Budget: 250 lines. Architecture
contracts are in `systemPatterns.md` (cited as "plan §N"), decisions in
`decisions/`, visual rules in `DESIGN.md`.

## Where the project is

All seven milestones are implemented (§12). Committed on `main` through
`bed3aab` (responsiveness pass), plus the 2026-10-21 batch and a full-codebase
review pass, merged as logical commits on the user's request before the
Windows check (next sections). Windows 11 is the acceptance environment;
Linux runs library tests and cross-links only.

| Milestone | State |
| --- | --- |
| 1 Foundation, lifetime, palette | Verified on Windows (size 5.9 MB, 0 ms hidden CPU, ~76 MiB, summon ~102 ms before the fast path). |
| 2 App launcher | Verified; one gate check pending (below). |
| 3 Clipboard + paste-back | Verified (text, image, persistence). |
| 4 Emoji | Verified; color glyphs via DirectWrite (decision 006). |
| 5 Scripts | Implemented. |
| 6 Snippets, tier 1 | Implemented; Windows checklist pending (§12.6). |
| 7 Polish | Placement, click-away, login item, appearance, settings, Raycast look verified in parts; the 2026-10-21 batch is unverified. |

## 2026-10-21 batch, committed but unverified on Windows

Contents:

- **Icons**: fetched at a stock size ≥ 2× the drawn edge, padding trimmed for
  small-only icons (7-Zip), area-averaged to exact pixels, drawn
  pixel-aligned (`core::icon_scale`). The white frame on 7-Zip's icon is its
  own art.
- **Spacing**: one 16 pt content edge (`GUTTER`, `BODY_MARGIN`, `ROW_INSET`,
  `SECTION_GAP`, `theme::inset`); the search text is centered; the footer's
  right inset is fixed; focused text fields lose egui's white outline.
- **Settings**: General / Palette / Clipboard sections; window size, text
  size, keep-state seconds, web search engine (decisions 012, 014, 015).
- **Keep state after dismiss** (decision 015).
- **Calculator card** (decision 013); the user approved its style.
- **Web search fallback** with engine setting (decision 014); the Settings
  picker shows names only, the row keeps a monogram badge.
- **Broken shortcuts** offer to recycle (decision 016).
- **Favorites** (decision 011).
- **Mouse-wheel scrolling** in every list and the emoji grid: whole rows
  (`list::wheel_rows`, `list::scroll_window`); the selection follows into
  view; the calculator card stays put.
- **Docs reorg** to this bank (decision 017); `.env` for machine paths
  (`.env.example`); public README.
- **Review pass** (correctness): usage/favorites files that fail to read are
  never overwritten (`persistence::Stored`); clipboard entries are deleted
  only when they fail to decrypt or decode, not on I/O errors; shortcuts are
  called broken only on a definite "not found"; a failed AppsFolder
  enumeration keeps the old index; the taskbar/desktop is never a paste
  destination; re-applying the live summon key is a no-op; stale launcher
  and snippet indices after a reload no longer panic; the web row waits for
  the first scan; a trial hotkey survives an unrelated save; lists clamp
  their scroll when they shrink; calculator rejects non-finite steps.
  Deferred: unsaved Settings edits are dropped by Esc (snippet editor warns);
  a failed thumbnail thread leaves "Loading preview…"; a failed clipboard
  write can leave the clipboard empty.
- **Simplify pass**: shared `win::{wide, wide_path, from_wide_nul,
  win_error_parts}` and `shell_execute`; unit captions live in `UNITS`;
  `theme::{WARNING, ERROR}` tokens; `list::scroll_window` returns the
  selection; dead `Wake`/`Notice` methods gone; the clipboard row preview
  and search look only at the start of large items.

## Motion and polish batch (2026-10-21), on a branch, unverified on Windows

Built and linted for the Windows target; host tests pass. Never run on
Windows yet. Contents (decision 018, DESIGN.md Motion):

- Summon fades/scales content in (160 ms, clock starts after uncloak);
  Esc/click-away/close fade out (90 ms) then hide; summon during the fade
  returns in place; launch/paste/web search still hide at once.
- View and form changes slide the body in (150 ms).
- `reduce_motion` setting (`system` follows `SPI_GETCLIENTAREAANIMATION`,
  re-read on each show): fades only. Settings > Palette > Reduce motion.

## Pending Windows checks

- Motion batch: summon feel with and without the fast path (no stale frame,
  animation visible from its start), Esc and click-away fades, re-summon
  during the fade (keeps destination: paste still works), paste/launch still
  instant, view slides, typing during
  the appear animation, reduced motion via the setting and via Windows'
  Animation effects switch, idle CPU 0 after animations end.

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

1. User's Windows pass over both batches above, including the review fixes;
   fix findings.
2. Decide the deferred review items above, and the structural refactors the
   review proposed (one texture LRU, a list cursor, a worker abstraction,
   an effect outbox instead of ~40 `take_*` accessors).
3. Flag-capable emoji font for the Flags group (backlog below).
4. **Before making the GitHub repo public**: history was rewritten on
   2026-09-25, but GitHub still serves the pre-rewrite commits. Delete and
   recreate the repository (or have GitHub Support purge it), then push
   `main`. Also add release notes: license notices, checksum, first-run
   behavior, and the clean-account check (§12.7 gate).

## Open questions and parked ideas

- **Dictionary**: tabled by the user (2026-10-21). Windows has no definitions
  API (only ISpellChecker). Options if revived: "define <word>" via web
  search, bundled WordNet (+10–30 MB), or an online API.
- **Real engine logos** instead of monogram badges: needs bundled assets
  (licensing) or a network fetch.
- **Broken-shortcut "Ignore"** is in memory only; persist it if the offer
  nags across restarts.
- **Calculator Enter** pastes (consistent with snippets/emoji); Raycast copies.
  Revisit if the user prefers copy-only.
- **Flag font**: Segoe UI Emoji has no flags; a COLR flag-only font (e.g.
  Twemoji, CC-BY/MIT with notice) costs ~1–2 MB. Also rank the Flags group
  above "flag in hole" for the bare query `flag`.
- **Snippet expansion** tier 2 stays unapproved (decision 009).

## Standing rules

- Use the memory-bank and making-decisions skills (`.agents/skills/`);
  commit bank updates with the change they record.
- README is the public product page only. Never track machine-specific
  paths; they belong in `.env`.
- Develop and build in WSL; try builds via `task stage` (or `task install`
  for the everyday copy). Nothing is installed on the Windows system drive.
- No scripted input or screenshots on the user's desktop without asking;
  prefer manual checklists.
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
