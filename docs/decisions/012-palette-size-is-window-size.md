---
id: 012
slug: palette-size-is-window-size
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [design, settings]
---

# Palette size changes the window; text size is a separate setting

## Decision

`palette_size` (small/medium/large) sets the window to 560×418, 640×460 or
760×544 points, differing by whole rows so lists show more results. Row
height, spacing and icons do not change. `text_size` sets font sizes
independently.

## Context

The first version zoomed the whole UI, which made everything bigger rather
than showing more.

## Alternatives considered

- One egui zoom factor (rejected by the user: "the window should be bigger").
- A free-form width/height setting (rejected: easy to break the layout).

## Rationale

Separates the two real needs: more results, and more readable text.

## Consequences

- Row counts derive from the size (`theme::visible_rows`); keyboard zoom is
  disabled.
