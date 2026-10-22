---
id: 028
slug: own-visual-language
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [design]
---

# A visual language of its own, built on Raycast's

## Decision

Keep Raycast's structure (decision 010: a large frameless search field, a
soft gray selection, section headers, a footer action bar with key caps) and
give it a flavor of its own from the other launchers:

- **Search glyph** (Spotlight, Tinycast): a painted magnifier heads the
  search field in the row icons' column, so the query starts on the row
  names' column. The back button takes the same slot in sub-views.
- **Selection pip** (Windows 11): the selected row's fill carries a short
  bar in the brand red at its left edge, and glides with it. It echoes the
  red full stop of the "s." icon.
- **Dissolving edges** (Tinycast): dividers fade out toward the window's
  edges instead of stopping hard, and a list's first or last row dims while
  more rows wait beyond it.
- **Key caps with a lower lip**: a darker bottom edge, which flattens while
  the key is held.

## Context

The user liked the Raycast look but wanted smolcast to have a distinct,
designed identity that takes the best of Raycast, Tinycast, Alfred and
Spotlight. Tinycast's design system (`docs/ui.md` in its repository, AGPL;
ideas only, never code) was reviewed for this.

## Alternatives considered

- Tinycast's full look: no dividers, floating bars over a blurred scrim,
  26-point panel corners, Liquid Glass controls (rejected: Windows draws the
  8-point DWM corners, egui cannot blur, and macOS glass reads as foreign on
  Windows).
- Spotlight's accent-filled selection (rejected: a red fill shouts; the pip
  marks the row without recoloring it).
- Tinycast's action capsule: the footer's main action in a clickable pill
  (tried, then rejected by the user: they prefer the plain label and key
  caps).
- Alfred's numbered quick-select keys per row (deferred: a behavior change,
  not a visual one).

## Rationale

Each element earns its place: the shared icon column makes the search and
the results read as one grid; the pip ties selection to the brand; dimmed
edge rows say "there is more" in a list that scrolls by whole rows.

## Consequences

- The accent now also marks the selected row (DESIGN.md, Principles).
- `list::row` takes a `RowState` (rest, selected, edge) instead of a bool.
- Emoji cells keep their fill and lift; the grid has no pip.
