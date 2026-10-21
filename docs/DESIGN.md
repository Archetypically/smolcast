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
  text cursor, text selection and links. Row selection is a soft gray fill,
  never the accent.
- **Follow the system.** Dark and light themes follow Windows. Mica shows
  through when it is available (see plan §12.7, Appearance).
- **Quick, and honest about work.** The palette never blocks on work. Anything
  still running shows a small spinner with a plain label ("Indexing apps…",
  "Loading clipboard history…", "Pasting…") until it finishes. A spinner only
  appears after 150 ms, so quick work does not flash one.

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

The base window is 640×460. It grows to fit notices and the hotkey recovery UI
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
| Controls (buttons, form fields) | white 7% | black 5% |
| Dividers | white 10% | black 8% |

| Size | Value |
| --- | --- |
| Search text | 18 pt |
| Body / button / small / monospace text | 14 / 13 / 12 / 13 pt |
| Corner radius: rows, windows | 8 pt |
| Corner radius: buttons | 6 pt |
| Corner radius: key caps | 4 pt |
| Result row height | 38 pt, 4 pt apart (`ROW_STRIDE` = 42) |
| Side gutter | 16 pt |
| App icon | 22 pt |
| Emoji cell / glyph | 44 / 28 pt |

The font is Segoe UI, with egui's bundled fonts as fallbacks.

## Components

- **Result row** (`list::row`). Leading icon, then the name, then gray
  trailing text (the entry type, the age, or a snippet keyword). Selected:
  rounded selection fill. Hovered: hover fill. The text color does not change.
- **Row icons.** Use the program's real icon wherever one exists. For
  applications, it comes from the Start Menu shortcut or the packaged app. For
  scripts, it comes from the interpreter executable, with bare names such as
  `pwsh.exe` resolved through the search path. A glyph is used only when no
  icon exists: built-in commands, clipboard entries, snippets, or a failed
  lookup (▶ for scripts).
- **Key cap** (`theme::key_cap`). Small gray text on a rounded selection fill.
  It is painted at the text's size so centered layouts cannot stretch it.
- **Action hint** (`theme::action_hint`). A label followed by its key caps,
  for right-to-left footer layouts.
- **Back button** (`theme::back_button`). A painted arrow, not a font glyph.
- **Busy indicator** (`theme::busy`, `busy_small`, `action_busy`). A small
  egui spinner beside weak text. Gate it with `theme::busy_due`, which applies
  the 150 ms delay. Spinners animate only while drawn, so a hidden palette
  never repaints for one.

## Rules

- **Write key names out.** Use "Enter", "Esc" and "Ctrl", not `↵` or `⌘`.
  Windows keyboards print the names, and the fallback fonts lack some arrow
  glyphs.
- **Draw icons, don't type them.** If a symbol is not certain to be in Segoe
  UI or Segoe UI Emoji, paint it with the painter instead of using a text
  glyph. The existing command glyphs (📋 😀 📝 ✏ ⚙ ▶) are covered by those fonts.
- **Size lists from the stride.** List heights are row counts times
  `theme::ROW_STRIDE`, never magic numbers.
- **Color only for status.** Warnings are amber and errors are red, in small
  text. Nothing else gets color.
- **Check new views in both themes.** Also check them at 100%, 150% and 200%
  scaling.
