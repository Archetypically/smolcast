---
id: 041
slug: themes-independent-of-appearance
author: archetypically
status: resolved
created: 2026-10-27
updated: 2026-10-27
tags: [design, appearance, settings]
---

# Color themes are independent of light and dark mode

## Decision

Two settings: `appearance` (follow Windows, light or dark) and `theme`
(Default, Claude, Slate, Sage, Mono). Every theme is a pair of palettes, one
light and one dark, each with its own accent, so any theme combines with any
appearance. Themes are built into the app and change colors only. Over Mica,
a tinted theme paints a share of its background (at its default opacity, or the
optional `mica_opacity` override, 0–100%); neutral themes leave Mica clear.
Settings previews the colors live while the form is open.

## Context

The user asked for themes detached from the light/dark/system choice, a
clean light and dark default, a Claude theme, and themes that feel designed
rather than novelty, working with Mica. The earlier parked sketch was
user-written TOML theme files.

## Alternatives considered

- Themes that are each light or dark (rejected: a theme would fight the
  system mode, and the user wanted the two toggles independent).
- User theme files under `%APPDATA%` (deferred: a small curated set comes
  first; the `Palette` shape is what a file would fill in later).
- Leaving Mica fully clear for every theme (rejected: Mica is neutral, so a
  tinted theme would lose its hue and read as Default with a new accent).
- A solid panel for tinted themes (rejected: throws Mica away entirely).
- Ports of popular editor palettes (Nord, Solarized) (rejected: they read as
  borrowed; the set is tuned to smolcast's quiet chrome).

## Rationale

Tinted themes take their fills from their own text color at the default's
strengths, so each stays as quiet as Default and differs only in temperature
and accent. A per-theme opacity keeps Mica visible while carrying the hue, and
the override lets someone who prefers more or less backdrop tune it.

## Consequences

- Plan §10.2 gains `appearance`, `theme` and `mica_opacity`; §12.7 Appearance
  describes the scheme, the opacity and the live preview.
- The settings writer removes keys the form cleared, which optional keys
  such as `mica_opacity` need.
- The website's `tokens.css` still comes from the Default palettes
  (`LIGHT`, `DARK`).
