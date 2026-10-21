---
id: 009
slug: snippets-palette-tier-first
author: archetypically
status: resolved
created: 2026-09-27
updated: 2026-10-21
tags: [snippets, input]
---

# Ship palette snippets; keep as-you-type expansion a separate tier

## Decision

Snippets are saved text searched and pasted from the palette, created and
edited in place, stored as `[snippets.<id>]` in config.toml. As-you-type
expansion is a later tier with its own approval; `keyword` is validated
strictly now because it will be the trigger.

## Context

Expansion needs a low-level keyboard hook, with layout, IME, dead-key,
recursion and integrity-level risks (§10.6, §12.6).

## Alternatives considered

- Build expansion first, as the brief listed (rejected: high risk for a
  nice-to-have).

## Rationale

Most of the value with no input hook.

## Consequences

- Loosening keyword rules later is compatible; tightening would not be.
