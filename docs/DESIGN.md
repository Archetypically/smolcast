# smolcast visual design

smolcast looks and behaves like Raycast, adapted to Windows 11. This is the
reference for anyone changing the UI. The tokens live in
[`src/app/theme.rs`](../src/app/theme.rs). Change them there, not per view.

## Principles

- **Keyboard first.** Every action has a key, and the footer shows the key for
  the main action. Mouse use works but is secondary.
- **Quiet chrome.** Neutral surfaces, no borders around fields or buttons, and
  hairline dividers instead of boxes. Color means status (warning, error), not
  decoration.
- **One accent, used sparingly.** Raycast red (`#FF6363`) is used only for the
  text cursor, text selection and links, plus one grace note: the heart on the
  About page. Row selection is a soft gray fill, never the accent.
- **Follow the system.** Dark and light themes follow Windows. Mica shows
  through when it is available (see plan §12.7, Appearance).
- **Quick, and honest about work.** The palette never blocks on work. Anything
  still running shows a small spinner with a plain label ("Indexing apps…",
  "Loading clipboard history…", "Pasting…") until it finishes. A spinner only
  appears after 150 ms, so quick work does not flash one.
- **Alive, never slow.** Motion is short, interruptible and paint-only: it
  never delays input or layout. See Motion.

## App icon

"s." : a white lowercase s (Inter Display Bold, as outlines) with the accent
red as its full stop, on a dark squircle (`#2C2C30` to `#161618`) with a 10%
white hairline rim. The accent appears once, as in the app. Source:
`assets/smolcast.svg`; `scripts/generate_icon.py` renders it to the `.ico`
at 16–256 px, dropping the rim and enlarging the mark at 24 px and below. It
is the exe, taskbar and tray icon (decision 026). Change the SVG, never the
`.ico`.

## Layout

From top to bottom, all sections span the full window width:

1. **Search bar.** A frameless 18-point text field with a 16-point side gutter
   and 12-point vertical padding. Sub-views put a painted back arrow (a
   28-point rounded square) to its left. Forms with no search field show the
   view title at the same size.
2. **Divider.** A 1-point hairline across the full window, ignoring margins.
3. **Body.** 8-point padding. The root list starts with a small, gray
   "Suggestions" (empty query) or "Results" header. While apps are indexed, the
   header's right side shows "Indexing apps…" (first scan) or "Refreshing
   apps…" with a spinner. The list stays usable.
4. **Footer action bar.** A divider, then a 40-point bar pinned to the bottom
   of the window. Left: where you are ("smolcast" or the view title), plus the
   summon key as a key cap at the root. Right: the main action with its key
   caps (`Open Application Enter`, `Paste Enter`, `Save Ctrl S`,
   `Delete Enter` while confirming a snippet delete), then
   `Back Esc` in sub-views. While that action runs ("Launching…",
   "Pasting…"), a spinner and the label replace its key caps.

The base window follows `palette_size`: small 560×418, medium 640×460,
large 760×544 points. Heights differ by whole rows (`theme::window_size`), so
a larger palette shows more results: −1 row for small and +2 for large in
every list. The emoji grid has 11, 12 or 15 columns. Text, rows and spacing do
not change with it; `text_size` sets the fonts separately. The window grows to
fit notices and the hotkey recovery UI
and shrinks back when they go. The host measures content without the footer's
bottom padding so that shrinking works.

## Tokens

| Token | Dark | Light |
| --- | --- | --- |
| Background | `#1C1C1E` | `#FAFAFA` |
| Text | `#EDEDED` | `#1C1C1E` |
| Secondary text | `#8E8E93` | `#6E6E73` |
| Selected row, key caps | white 10% | black 8% |
| Text selection | `#FF6363` at 35% | `#FF6363` at 35% |
| Hover | white 5% | black 4% |
| Keyboard focus ring (buttons, checkboxes, menus) | 1 pt secondary text | 1 pt secondary text |
| Controls (buttons, form fields) | white 7% | black 5% |
| Dividers | white 10% | black 8% |

| Size | Value |
| --- | --- |
| Search text (`text_size` small / medium / large) | 16 / 18 / 20 pt |
| Body / button / small / monospace text, medium | 14 / 13 / 12 / 13 pt |
| Same, small / large | 13 / 12 / 11 / 12 and 16 / 14 / 13 / 14 pt |
| Corner radius: rows, windows | 8 pt |
| Corner radius: buttons | 6 pt |
| Corner radius: key caps | 4 pt |
| Result row height | 38 pt, 4 pt apart (`ROW_STRIDE` = 42) |
| Side gutter (`GUTTER`): where all content starts and ends | 16 pt |
| Body margin (`BODY_MARGIN`): where row and cell fills start | 8 pt |
| Row content inset (`ROW_INSET` = gutter − body margin) | 8 pt |
| Gap between body sections (`SECTION_GAP`) | 8 pt |
| App icon | 22 pt |
| Emoji cell / glyph | 44 / 28 pt |

The font is Segoe UI, with egui's bundled fonts as fallbacks.

## Components

- **Result row** (`list::row`). Leading icon, then the name, then gray
  trailing text (the entry type, the age, or a snippet keyword). Selected:
  rounded selection fill. Hovered: hover fill. The text color does not change.
- **Favorites.** Up to 5 pinned entries (`core::favorites`) lead the empty
  root list under a "Favorites" header, followed by a "Suggestions" header
  (which takes one row). While searching they rank normally, and their trailing
  text reads "Favorite · Application". Toggle with Ctrl+Shift+F or the row's
  right-click menu, which also has Move Up and Move Down (Ctrl+Shift+Up and
  Ctrl+Shift+Down).
- **Fallback rows.** After the ranked entries of any query that is not a
  calculation or a quicklink query, scrolling with them, under a
  `Search for "query"` section header (the query cut with "…" past 40
  characters; the header takes one row, and is left out when nothing else
  matched). First "Search Google for "query"", led by the engine's monogram
  badge (`theme::engine_badge`: its letter on a circle in its color; logos are
  not bundled) with a gray "Web Search" label; Enter opens the default
  browser. Then "Search Files for "query"" (search glyph, gray "Files"), which opens the
  Search Files view with the query.
- **Confirming row.** A system action that needs a second Enter (Restart,
  Shut Down, Sign Out, Empty Recycle Bin) replaces its gray type label with
  "Press Enter again to confirm" in the warning color, and the footer action
  reads "Confirm". Moving the selection or typing clears it.
- **Quicklink with a query.** `gh rust` shows the quicklink first as
  `Search GitHub "rust"`; without a query the footer reads "Type Query" and
  Enter or Tab fills in `gh `.
- **Update offer.** Under the root list when the query is empty, separated
  by a rule like the broken-shortcuts offer: a small line "smolcast X is
  available (you have Y)." then Install and Restart, What's New and Skip This
  Version. While checking or downloading, a small spinner with its label
  takes the place of the line and buttons.
- **File rows** (Search Files view). Explorer's icon (a folder or document
  glyph until it loads), the file name, and the
  containing folder in gray on the right, cut from the left with "…" past 48
  characters (full path on hover). The section header reads "Files", with
  "Searching…" while a query runs. The empty query explains where results
  come from and the Enter / Ctrl+Enter keys.
- **Broken shortcuts offer.** Under the root list, separated by a rule: a
  small gray line naming them (full paths on hover), then "Move to Recycle Bin"
  and "Ignore" buttons. A spinner shows while the move runs.
- **Calculator card** (`launcher::answer_card`). When the root query is a
  calculation or a unit conversion (`core::calc`), the section reads
  "Calculator". A full-width rounded card shows the input and the result side
  by side, in large text (1.4× the search size). Each sits over a gray caption
  pill ("Centimeters", "Inches"; "Expression", "Result" for math), with a
  painted arrow between them. The apps follow under a "Results" header. The
  card and that header take exactly three list rows, so the window keeps its
  height. The card is selected first; Enter pastes the result (the footer
  reads "Paste Answer"). Fill: control at rest, hover fill on hover,
  selection fill when selected.
- **Row icons.** Use the program's real icon wherever one exists. For
  applications, it comes from the Start Menu shortcut or the packaged app. For
  scripts, it comes from the interpreter executable, with bare names such as
  `pwsh.exe` resolved through the search path. Search Files rows show
  Explorer's icon for each file or folder. A glyph is used only when no icon
  exists: built-in commands, system actions, quicklinks, the files fallback,
  or a failed lookup. Glyphs come from Windows' icon font (Segoe Fluent Icons
  on Windows 11, Segoe MDL2 Assets on 10; decision 023) via `theme::glyph`,
  at 3/4 of the 22-point slot in the text color, so they read like the Start
  menu's power menu; the emoji is only a fallback when the font or glyph is
  missing. Clipboard entries and snippets keep their own previews.
- **About page** (`about_view`). "About smolcast" (also found by "Credits")
  is a centered column: the name at 1.6× the search size, the version,
  a one-line tagline, "Made with ♥ by Evan Lee" (the heart is painted in the
  accent and beats once, lub-dub, when the page opens; hovering it beats
  again), then the "Built with" credits and the license in small text. The
  footer shows only `Back Esc`.
- **Settings form.** Sections (General, Palette, Clipboard), each headed by
  `theme::section_label` (the same small gray style as "Suggestions"). Labels
  sit in a shared 112-point column so controls align across sections. Rows
  are 6 points apart and sections 16 points apart. Short gray hints sit beside
  controls. Save, Revert and the status are on the left of the bottom row, and
  "Edit config file…" is on the right.
- **Key cap** (`theme::key_cap`). Small gray text on a rounded selection fill.
  It is painted at the text's size so centered layouts cannot stretch it.
  While its key is held, the cap lights up (brighter fill, full-strength text).
- **Action hint** (`theme::action_hint`). A label followed by its key caps,
  for right-to-left footer layouts.
- **Back button** (`theme::back_button`). A painted arrow, not a font glyph.
  On hover the arrow leans 1.5 points to the left.
- **Busy indicator** (`theme::busy`, `busy_small`, `action_busy`). A small
  egui spinner beside weak text. Gate it with `theme::busy_due`, which applies
  the 150 ms delay. Spinners animate only while drawn, so a hidden palette
  never repaints for one.

## Motion

Tokens live in `core::motion`; the egui side is `app::motion`. Every
animation changes paint only (`Ui::with_visual_transform`, opacity), so clicks
and keys always act on final positions, and typing works from the first frame.

| Moment | What moves | Time | Easing |
| --- | --- | --- | --- |
| Summon | Content fades in and scales 97% → 100% about the window center; the window frame and backdrop appear at once | 160 ms | ease out (cubic) |
| Dismiss (Esc, click-away) | Content fades out and scales to 97%, input off, then the window hides; a summon meanwhile reverses it | 90 ms | ease in (cubic) |
| Completed action (launch, paste, web search) | Nothing: the window hides at once | — | — |
| View change, form open or close | Body slides 12 pt in (from the right going deeper, from the left going back) and fades in; the search line fades | 150 ms | ease out (cubic) |
| Selection | The row or emoji cell fill glides to its new place | 80 ms | linear |
| Hover | Row and cell hover fills fade in and out | 80 ms | linear |
| Emoji cell | The selected glyph lifts to 108% | 120 ms | ease out (cubic) |
| Back button | The arrow leans 1.5 pt left on hover | 100 ms | linear |
| Key cap | Lights up while its key is held | 60 ms | linear |

Reduced motion (`reduce_motion`, following Windows' Animation effects by
default) keeps the fades and drops all movement: no scaling, sliding, gliding,
lifting or leaning, and egui's own animations are off. A new summon or view
starts a new motion epoch, so highlights appear in place instead of gliding in
from a stale spot. Animations request repaints only while they run; a resting
palette is idle.

## Accessibility

- **Reduced motion**, as above.
- **Keyboard focus** on buttons, checkboxes and menus shows a 1-point ring
  (egui draws keyboard focus with the active style, whose fill alone was
  nearly invisible). Text fields keep their cursor instead.
- **High contrast.** Under a Windows high-contrast theme (re-read on each
  show) the selection, hover, control and divider fills are stronger and the
  secondary text is closer to the primary text. Mica is off then (§12.7).

## Rules

- **Write key names out.** Use "Enter", "Esc" and "Ctrl", not `↵` or `⌘`.
  Windows keyboards print the names, and the fallback fonts lack some arrow
  glyphs.
- **Draw icons, don't type them.** If a symbol is not certain to be in Segoe
  UI or Segoe UI Emoji, paint it with the painter instead of using a text
  glyph. The existing command glyphs (📋 😀 📝 ✏ ⚙ ▶) are covered by those fonts.
  Built-in row icons are Windows icon-font glyphs (decision 023), with these
  emoji only as fallbacks.
- **One content edge.** Text, icons, buttons and fields start and end on the
  16-point gutter in every section and view. Only row and cell fills reach
  out to the 8-point body margin. Body content that is not a row goes inside
  `theme::inset`. Put space between sections with `theme::SECTION_GAP`,
  not ad-hoc numbers.
- **No outlines on focused fields.** The red text cursor shows focus.
- **Scroll by whole rows.** Lists are windows of rows, not scroll areas. The
  mouse wheel moves the window a row at a time (`list::wheel_rows`), and the
  selection follows into view rather than pulling the window back.
- **Size lists from the stride.** List heights are row counts times
  `theme::ROW_STRIDE`, never magic numbers.
- **Color only for status.** Warnings are amber and errors are red, in small
  text. Nothing else gets color, except the accent where Principles allow it
  (the About page's heart).
- **Check new views in both themes.** Also check them at 100%, 150% and 200%
  scaling, with reduced motion on, and under a high-contrast theme.
- **Animate paint, never input.** New motion goes through `core::motion`
  tokens and `app::motion` helpers, stays under 200 ms, and has a reduced
  form with no movement.
