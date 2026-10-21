---
id: 006
slug: emoji-exact-sequences-color-preview
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [emoji]
---

# Paste exact emoji sequences; preview in color through DirectWrite

## Decision

The emoji table is generated from pinned Unicode Emoji 17.0 and CLDR 48.2 data
and committed. Paste writes the exact grapheme sequence. The grid renders
color glyphs by rasterizing Segoe UI Emoji with DirectWrite off the UI
thread.

## Context

The original plan (§9, §14) approved a monochrome preview to avoid a color
atlas. DirectWrite rasterization turned out to give color without bundling
one.

## Alternatives considered

- Monochrome egui font preview (superseded: looked broken next to Windows'
  own pickers).
- A bundled color emoji atlas (rejected: binary size).

## Rationale

The product contract is the exact pasted codepoints; how the destination
renders them is not guaranteed. Color preview costs no bundle size.

## Consequences

- Flags show as letter pairs, because Segoe UI Emoji has none.
