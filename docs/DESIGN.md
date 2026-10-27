# smolcast visual design

smolcast keeps Raycast's structure and gives it a flavor of its own:
Spotlight's search glyph, Tinycast's dissolving edges, and a Windows 11 selection pip in the brand red (decision 028). This is the
reference for anyone changing the UI. The tokens live in
[`src/app/theme.rs`](../src/app/theme.rs). Change them there, not per view.

## Principles

- **Keyboard first.** Every action has a key, and the footer shows the key for
  the main action. Mouse use works but is secondary.
- **Quiet chrome.** Neutral surfaces, no borders around fields or buttons, and
  hairline dividers instead of boxes. Color means status (warning, error), not
  decoration.
- **One accent, used sparingly.** Each theme has one accent; the default
  is the brand red (`#FF6363`, the full stop of the "s." icon). It marks where you are: the text cursor, text selection, links
  and the selected row's pip, plus one grace note: the heart on the About
  page. Row selection is a soft gray fill, never an accent fill.
- **Edges dissolve.** Dividers fade out toward the window's edges, and a
  list's edge row dims while more rows wait beyond it.
- **One column.** The search glyph, back button and row icons share a
  column, and the query starts where row names do.
- **Follow the system.** Light and dark follow Windows unless Appearance
  forces one. Mica shows through when it is available (see plan §12.7,
  Appearance), under a tinted theme's background at partial opacity.
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
   and 12-point vertical padding, headed by a painted magnifier in the
   22-point icon slot, so the query starts on the row names' column (gutter
   + 22 + 10). Sub-views put the painted back arrow (a 28-point rounded
   square centered on the same slot) there instead. Forms with no search
   field show the view title at the same size and column.
2. **Divider.** A 1-point hairline across the full window, ignoring margins,
   dissolving to nothing over the outer 18% at each end.
3. **Body.** 8-point padding. The root list starts with a small, gray
   "Suggestions" (empty query) or "Results" header. While apps are indexed, the
   header's right side shows "Indexing apps…" (first scan) or "Refreshing
   apps…" with a spinner. The list stays usable.
4. **Footer action bar.** A divider, then a 40-point bar pinned to the bottom
   of the window. Left: where you are ("smolcast" or the view title), plus the
   summon key as a key cap at the root. Right: the main action with its key
   caps (`Open Application Enter`, `Paste Enter`, `Save Ctrl S`,
   `Delete Enter` while confirming a snippet delete, `Run Enter` and
   `Untrack Enter` in Manage Scripts), then
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

## Themes

`theme` and `appearance` are independent (decision 041): every theme has a
light and a dark variant, and Appearance (Follow Windows, Light, Dark) picks
which one shows. Themes change colors only; layout, type, radii and motion
stay the same.

| Theme | Light background / text | Dark background / text | Accent (light / dark) | Mica opacity |
| --- | --- | --- | --- | --- |
| Default | `#FAFAFA` / `#1C1C1E` | `#1C1C1E` / `#EDEDED` | `#FF6363` | 0% |
| Claude | `#FAF9F5` / `#1F1E1D` | `#262624` / `#EEEDE7` | `#C6613F` / `#D97757` | 60% |
| Slate | `#F7F8FA` / `#1A1E25` | `#1B1E24` / `#E4E8EF` | `#2F6BD8` / `#6FA3FF` | 45% |
| Sage | `#F6F7F3` / `#1C211D` | `#1B1F1C` / `#E3E8E2` | `#3A8459` / `#74BF8E` | 50% |
| Mono | `#FFFFFF` / `#111111` | `#0E0E0E` / `#F2F2F2` | the text color | 0% |

Rules for a theme: hue lives in the background, the gray text and one
accent, never in extra decoration. The selection, hover, control and
divider fills of a tinted theme are its own text color at the default
theme's strengths (below), so they carry its hue. Secondary text keeps at
least 4.5:1 against the background. A neutral theme leaves Mica clear; a
tinted one paints part of its background over it (`mica_opacity` overrides
the amount, 0–100%). Settings previews a theme live while the form is
open; the theme picker leads each name with a swatch (`theme::swatch`) of
its background and accent pip.

## Tokens

The default theme's tokens:

| Token | Dark | Light |
| --- | --- | --- |
| Background | `#1C1C1E` | `#FAFAFA` |
| Text | `#EDEDED` | `#1C1C1E` |
| Secondary text | `#8E8E93` | `#6E6E73` |
| Selected row, key caps | white 10% | black 8% |
| Text selection | accent at 35% | accent at 35% |
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
| Selection pip | 3 × 14 pt, accent, 2 pt radius, 2 pt inside the fill's left edge |
| Edge row opacity | 45% |
| Result row height | 38 pt, 4 pt apart (`ROW_STRIDE` = 42) |
| Side gutter (`GUTTER`): where all content starts and ends | 16 pt |
| Body margin (`BODY_MARGIN`): where row and cell fills start | 8 pt |
| Row content inset (`ROW_INSET` = gutter − body margin) | 8 pt |
| Gap between body sections (`SECTION_GAP`) | 8 pt |
| Icon slot (`ICON_SLOT`): app icons, search glyph, back button column | 22 pt |
| Icon to name gap (`ICON_GAP`) | 10 pt |
| Emoji cell / glyph | 44 / 28 pt |

The font is Segoe UI, with egui's bundled fonts as fallbacks. The `font`
setting can put any installed family ahead of Segoe UI for interface text;
sizes and layout do not change with it, and monospace text keeps egui's font.

## Components

- **Result row** (`list::row`, with a `RowState`). Leading icon, then the
  name, then gray trailing text (the entry type, the age, or a snippet
  keyword). Selected: rounded selection fill with the accent pip
  (`theme::selection_pip`) at its left edge. Hovered: hover fill. The text
  color does not change. Edge: the first or last row drawn while more rows
  wait above or below dims to 45%, unless selected.
- **Favorites.** Up to 5 pinned entries (`core::favorites`) lead the empty
  root list under a "Favorites" header, followed by a "Suggestions" header
  (which takes one row). While searching they get a ranking bonus (decision
  039), and their trailing text reads "Favorite · Application". Toggle with
  Ctrl+Shift+F or the row's right-click menu, which also has Move Up and
  Move Down (Ctrl+Shift+Up and Ctrl+Shift+Down).
- **Hidden entries.** The right-click menu ends with "Hide" (Ctrl+Shift+H);
  an info notice names the entry and Manage Hidden Entries. That view (eye
  glyph with a slash, U+ED1A) lists hidden entries as result rows under a
  "Hidden" header, with the entry type or "Not installed" as trailing text;
  its footer action is "Unhide". With nothing to show it says "Nothing is
  hidden." (or "No matches.") over a small gray hint.
- **Fallback rows.** After the ranked entries of any query that is not a
  calculation or a quicklink query, scrolling with them, under a
  `Search for "query"` section header (the query cut with "…" past 40
  characters; the header takes one row, and is left out when nothing else
  matched). First "Search Google for "query"", led by a globe glyph
  (the same for every engine; no engine logos or marks, decision 038) with a gray "Web Search" label; Enter opens the default
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
  selection fill and pip when selected.
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
  theme's accent and beats once, lub-dub, when the page opens; hovering it beats
  again), then the "Built with" credits and the license in small text. The
  footer shows only `Back Esc`.
- **Settings form.** Sections (General, Palette, Clipboard; Palette starts
  with Appearance, Theme and Mica opacity, a slider with its number, plus a
  Reset button once set, disabled where Mica is off), each headed by
  `theme::section_label` (the same small gray style as "Suggestions"). Labels
  sit in a shared 112-point column so controls align across sections. Rows
  are 6 points apart and sections 16 points apart. Short gray hints sit beside
  controls. Save, Revert and the status are on the left of the bottom row, and
  "Edit config file…" is on the right.
- **Key cap** (`theme::key_cap`). Small gray text on a rounded selection fill
  with a darker 1-point lower lip, like a physical key. It is painted at the
  text's size so centered layouts cannot stretch it. While its key is held,
  the cap lights up (brighter fill, full-strength text) and the lip flattens.
- **Action hint** (`theme::action_hint`). A label followed by its key caps,
  for right-to-left footer layouts.
- **Back button** (`theme::back_button`). A painted arrow, not a font glyph,
  in a 28-point square centered on the icon slot. On hover the arrow leans
  1.5 points to the left.
- **Search glyph** (`theme::search_glyph`). A painted magnifier in the icon
  slot, secondary text color, 1.5-point stroke.
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
| Selection | The row or emoji cell fill (and the row's pip) glides to its new place | 80 ms | linear |
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
  (the selection pip, the About page's heart).
- **Check new views in both modes and every theme.** Also check them at 100%, 150% and 200%
  scaling, with reduced motion on, and under a high-contrast theme.
- **Animate paint, never input.** New motion goes through `core::motion`
  tokens and `app::motion` helpers, stays under 200 ms, and has a reduced
  form with no movement.

## Taste: made, not generated

smolcast should feel made by someone who cares, never generated. These
rules bind the app and the website. The website's floor follows
[Hallmark](https://github.com/nutlope/hallmark), whose linter
(`sloplint.mjs`) is how to audit a page (techContext, Website).

**Delight comes from responsiveness, not decoration.** The joy is a palette
that answers before you finish typing, a key cap that presses with your
key, a pip that slides to the row you chose. Motion:

- answers something the person did, or marks one moment once (the
  underline drawing in, the heart beat). Nothing loops or pulses to ask
  for attention.
- is short: interactions under 200 ms, an entrance under 350 ms, ease-out,
  no bounce, overshoot or spring.
- moves only `transform` and `opacity` (and color or shadow for hover
  tints), never
  width, height, top, left or padding, and never `transition: all`.
- never makes anyone wait: input acts on final positions, content is
  readable with scripts off, and hidden-until-script states are scoped
  under `.js`.
- has a reduced-motion form with no movement.
- is spent on one signature per screen. A page that has a reveal has one,
  and documents have none.

**Never ship these tells:**

- An eyebrow label over a heading, or labels in tracked capitals. The
  heading says it.
- Middle-dot meta strings ("Windows 11 · x64 · 7 MB") or arrows glued to
  link text ("Compare →"). Use pills, line breaks, or a verb as the label.
- Decorative status dots, emoji as icons, a card inside a card, a
  purple-to-blue gradient, a hand-drawn browser window.
- Fade-up-on-every-scroll, parallax, typewriter headlines, counting
  numbers, shimmer, tilt or magnetic hovers, confetti, cursor trails.
- An invented number, quote, logo or testimonial. Comparisons cite a
  source, a version and a date, bold only where smolcast comes out ahead,
  and leave a row plain when the other side does not publish the fact.
- Literal colors past the token block. Every color is a token
  (`tokens.css` from `theme.rs`, or the web-only block in `styles.css`).

**Copy is short and casual for a skimmer.** One idea per screen, a lead of
one or two short sentences, typographic quotes (’ “ ”), and facts in pills,
badges and tables rather than paragraphs. Fullness comes from scale and
real product states (palette states, never stock art), not more words.

**Craft floor on the web:** `overflow-x: clip` on `html` and `body`; grid
tracks as `minmax(0, 1fr)`; an instant `:focus-visible` ring on everything
interactive; nothing scrolls sideways at 360 px.

## Progressive enhancement and discovery

The website works, and says everything it has to say, before any script
runs, because that is what search crawlers and AI fetchers read. JavaScript
only adds behavior on top.

- **HTML first.** Every page is complete, static HTML: the facts, tables
  and links are in the markup. A script may replace a piece of it (the home
  palette replaces its plain list of links) or decorate it (the "Runs on"
  bolding), never supply content that is not already there.
- **Visible without script.** Nothing is hidden at rest unless the hiding
  is scoped under `.js`, the class the script sets. Controls that need a
  script (the search pill) are hidden when it never runs.
- **No guesses in the markup.** Anything that depends on the visitor (their
  OS, their theme) is added on the client; the served page is the same for
  everyone.
- **Real links, linked from home.** Every page is reachable through plain
  `<a href>` links with descriptive text, from home and from its neighbors;
  the sync check fails when an indexable page is not linked from home, the
  sitemap or `llms.txt`.
- **Answer first.** A page's title, description and lead state its answer
  in plain words a reader or a model can quote ("smolcast collects
  nothing."); facts sit in tables, lists and pills, not buried in prose.
- **Cite and date.** Claims about other products carry a source, the version
  checked and a date, in footnotes and as `dateModified`; stale facts are
  worse for trust than missing ones.
- **Describe it for machines too.** Each page has a canonical URL, a meta
  description, Open Graph tags and JSON-LD (`SoftwareApplication` on home;
  `WebPage` or `CollectionPage` plus `BreadcrumbList` elsewhere), and
  `llms.txt` summarizes every page in a line.
- **Never optimize visibly.** No keyword stuffing, hidden text, doorway
  pages or FAQ filler written for crawlers. A page earns its ranking by
  being the clearest, most honest answer, and reads as written for people.

## Website

The website (`website/`, decision 030) speaks the same language on the web:

- **The palette is the hero and the navigation.** A recreation at the
  medium size (640 wide, 38-point rows 4 apart) with the app's colors in
  the generated `website/public/tokens.css`; its rows jump to sections and
  open the other pages, and Esc dismisses
  it the way the app is dismissed.
- **Feature tiles are palette states**, not screenshots or icons: each shows
  the query and rows that feature produces.
- **Other pages** (privacy, code signing, comparisons) share the home page's top bar,
  footer, type and tokens, in a 1040-wide column. Each opens with a hero
  (breadcrumb, short headline, a lead that answers the page's question in
  casual words, an "Updated" pill), then puts facts in
  components a skimmer can read at a glance: **cards** (smolcast's carries
  the selection pip), **pills** (`.tag`) for short facts, **status badges**
  (`.status`: "Built in" on a green tint with a check, "Pro", "Plugin" or
  "Optional" on an amber tint, a plain gray "No"; the word carries the
  meaning), big
  **figures**, a feature **matrix** grouped into The basics, Features and
  Under the hood, where only smolcast's wins are bold (a ringed badge),
  and only when the other side publishes the fact; the other tool's
  strengths are told in its card and its "Go with" list, which sits second,
  after smolcast's. The one exception is the "Runs on" row: its cells name
  their systems (`data-os`), and the script bolds the ones that run on the
  visitor's computer, with "Runs on your computer" under them (Windows 11
  versus 10 only where the browser reports it; otherwise any Windows). The
  served HTML stays plain, so crawlers see no guess. Each
  competitor card links out to the tool itself ("Get Raycast for
  Windows", tagged `?ref=smolcast`). Then
  "Coming from" mapping cards, a download card, links to other pages
  drawn as palette rows, and **footnotes** with each source, its version
  and the date checked. Comparisons say where the other tool is the
  better pick.
- **Backdrop**: the film's stipple of s. marks (`public/stipple.svg`, a
  mask colored by `--stipple`) on a fixed layer behind every page, full in
  the side margins and fading out as it nears the content column, so never
  behind text and faint or absent on phones. It does not move on the site.
- **Social cards**: every page has its own 1200 × 630 card (`public/og/`),
  rendered from `media/og-page.html`, which reads the page itself: its
  headline and lead on the left, its facts on the right (the matrix's
  glanceable rows, or the privacy figures), on the stippled dark page.
  Home keeps `og.png`, the palette converting 40 cm to inches. Re-render
  after changing a page's headline, lead or table.
- **Elsewhere**, Ctrl+K opens the palette over the page on a blurred scrim;
  Esc or a click outside dismisses it. Before its script runs (and for
  crawlers), the home palette is a list of every section and page as
  plain links drawn like its rows.
- **Type**: the system UI stack led by Segoe UI Variable, Cascadia Code for
  code; no web fonts. Headlines are tight (−0.035 em) and short.
- **Accent**: as in the app. The red marks the caret, the selection pip,
  a section reached from the palette, the tour's play glyph and the
  footer heart; buttons are neutral (text-colored fill). The headline
  "The smol command palette for Windows." ends on a red full stop, the
  icon's "s." at headline size (`.dot`), and "smol" has a red underline
  (`.smol`) that draws in once on the hero. Words stay in the text color:
  red text fails contrast on the light theme.
- **Motion**: the selection glides, rows enter with a 12 ms stagger when a
  view opens, key caps press with the visitor's keys (on every page), the
  home page's sections fade up once (6 px, 320 ms; its one reveal), the
  heart beats lub-dub once, cards and matrix rows tint under the pointer.
  Document pages have no reveal: facts are there at once. Every animation
  has a reduced-motion form. See Taste for what never ships.
- **Touch**: the palette stays the hero on phones (six rows, no trailing
  types). The list scrolls natively with row snapping and hands the swipe
  to the page at its ends; key caps and key hints are hidden on coarse
  pointers, and the try chips type without raising the on-screen keyboard.
- **Film**: the demo film is a simulation drawn to the app's own measurements,
  labels and results (decision 031), on a Windows 11 desktop at 150%, with the
  shortcuts pressed shown as key caps on a dark pill at the bottom. Around it,
  restrained motion: a quiet stipple of small white s. marks drifting up and
  right, one short caption per chapter (accent numeral, words rising), a slow
  camera lean while the palette is open. It starts and ends on the icon, name
  and line, frame for frame, so it loops. Keep it in step with the app. The
  social card and the hero are the web recreation, and say so.
