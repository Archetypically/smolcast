---
id: 038
slug: web-search-globe-glyph
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [launcher, search, design]
---

# Lead the web search row with a globe glyph, not an engine mark

## Decision

The web search fallback row shows a globe glyph for every engine and names
the engine only in its text ("Search Google for …"). smolcast ships no
engine logos, monograms or brand colors.

## Context

Decisions 014 and 022 led the row with a painted monogram badge (the
engine's letter on a circle in its color) because bundled logos raised
licensing questions. The user asked about bundling real logos instead.
Bundled PNGs would cost only tens of KB, but logos are trademarks, and some
owners' guidelines require permission for this use.

## Alternatives considered

- Bundled logo PNGs or SVGs (rejected: trademark permissions per brand,
  notably Google, the default, and Bing).
- Favicons fetched and cached at runtime (rejected: network access and a
  request to the engine before any search).
- The monogram badge (rejected: a letter in a brand's color still borrows
  the brand, and the name in the row already says which engine it is).
- The magnifier glyph (rejected: the Search Files row right below uses it).

## Rationale

The name in plain text is enough to identify the engine and is the least
risky use of a mark. A globe reads as "web", differs from the files row, and
covers custom URLs the same way. It also removes code.

## Consequences

- `theme::engine_badge` and `SearchEngine::badge` are gone; the row uses
  `theme::glyph` with Segoe Fluent Icons' Globe (U+E774, 🌐 fallback).
- DESIGN.md (Fallback rows) and the website's quicklinks tile show the globe.
- Refines decision 022's badge; its fallback rows are unchanged.
