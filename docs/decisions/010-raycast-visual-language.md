---
id: 010
slug: raycast-visual-language
author: archetypically
status: resolved
created: 2026-10-20
updated: 2026-10-21
tags: [design]
---

# Follow Raycast's visual language

## Decision

The palette looks like Raycast: a large frameless search field, hairline
dividers, a soft gray selection, section headers, and a footer action bar with
key caps. `DESIGN.md` holds the tokens and rules; content aligns on one
16-point edge.

## Context

The first UI used egui's defaults, which read as a debug tool.

## Alternatives considered

- Windows 11 Fluent styling (rejected: users of this tool expect Raycast's
  feel).

## Rationale

A familiar, calm look for something summoned many times a day.

## Consequences

- Accent color only for the text cursor, selection and links.
- The calculator card follows Raycast's conversion card.
