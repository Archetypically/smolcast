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
  text cursor and links. Selection is a soft gray fill, never the accent.
- **Follow the system.** Dark and light themes follow Windows. Mica shows
  through when it is available (see plan §12.7, Appearance).

## Layout

From top to bottom, all sections span the full window width:

1. **Search bar.** A frameless 18-point text field with a 16-point side gutter
   and 12-point vertical padding. Sub-views put a painted back arrow (a
   28-point rounded square) to its left. Forms with no search field show the
   view title at the same size.
2. **Divider.** A 1-point hairline across the full window, ignoring margins.
3. **Body.** 8-point padding. The root list starts with a small, gray
   "Suggestions" (empty query) or "Results" header.
4. **Footer action bar.** A divider, then a 40-point bar pinned to the bottom
   of the window. Left: where you are ("smolcast" or the view title), plus the
   summon key as a key cap at the root. Right: the main action with its key
   caps (`Open Application Enter`, `Paste Enter`, `Save Ctrl S`), then
   `Back Esc` in sub-views.

The base window is 640×460. It grows to fit notices and the hotkey recovery UI
and shrinks back when they go. The host measures content without the footer's
bottom padding so that shrinking works.

## Tokens

| Token | Dark | Light |
| --- | --- | --- |
| Background | `#1C1C1E` | `#FAFAFA` |
| Text | `#EDEDED` | `#1C1C1E` |
| Secondary text | `#8E8E93` | `#6E6E73` |
| Selection, key caps | white 10% | black 8% |
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
- **Key cap** (`theme::key_cap`). Small gray text on a rounded selection fill.
  It is painted at the text's size so centered layouts cannot stretch it.
- **Action hint** (`theme::action_hint`). A label followed by its key caps,
  for right-to-left footer layouts.
- **Back button** (`theme::back_button`). A painted arrow, not a font glyph.

## Rules

- **Write key names out.** Use "Enter", "Esc" and "Ctrl", not `↵` or `⌘`.
  Windows keyboards print the names, and the fallback fonts lack some arrow
  glyphs.
- **Draw icons, don't type them.** If a symbol is not certain to be in Segoe
  UI, paint it with the painter instead of using a text glyph.
- **Size lists from the stride.** List heights are row counts times
  `theme::ROW_STRIDE`, never magic numbers.
- **Color only for status.** Warnings are amber and errors are red, in small
  text. Nothing else gets color.
- **Check new views in both themes.** Also check them at 100%, 150% and 200%
  scaling.
