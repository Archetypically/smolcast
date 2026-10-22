---
id: 023
slug: windows-icon-font-for-built-in-glyphs
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [design, icons]
---

# Draw built-in icons with Windows' own icon font

## Decision

Rows without a program icon (built-in commands, system actions, quicklinks,
the files fallback, scripts without an icon) draw a glyph from Windows' icon
font, Segoe Fluent Icons (`SegoeIcons.ttf`, Windows 11) or Segoe MDL2 Assets
(`segmdl2.ttf`, Windows 10), which share codepoints and are what the Start
menu's power menu and Settings use. The font is loaded from
`%WINDIR%\Fonts` into its own egui family; a glyph the font lacks, or a
missing font, falls back to the emoji used before. Search Files rows show
Explorer's real icon for each item.

## Context

The user asked whether system actions had icons (they were emoji) and chose
"as native as possible".

## Alternatives considered

- Emoji (rejected: colorful, inconsistent with Windows' monochrome system
  icons, and not what the Start menu shows).
- Stock shell icons (`SHGetStockIconInfo`) or `imageres.dll` resources
  (rejected: no stock icons exist for sleep, restart, shut down, sign out,
  mute or dark mode, and resource ids are undocumented and change between
  releases).
- Bundling an icon font or SVGs (rejected: licensing and size, when every
  Windows install already has the font).
- Painted icons (rejected: more code for a worse match to Windows).

## Rationale

The same glyphs Windows uses, crisp at every DPI, in the text color so they
follow light, dark and high contrast, at no size cost. Codepoints were taken
from a published Segoe MDL2 codepoint list; Segoe Fluent Icons keeps them.

## Consequences

- The glyph tables live beside their users (`app/launcher.rs`: `command_glyph`,
  `system_glyph`, `GLYPH_SEARCH`; `app/file_view.rs`: folder and document);
  `theme::glyph` draws one.
- Sign Out uses SwitchUser (E748): MDL2's SignOut glyph could not be
  verified offline.
- Apps and scripts keep their real program icons (DESIGN.md "Row icons").
