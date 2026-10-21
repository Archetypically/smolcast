---
id: 003
slug: single-instance-silent-second-launch
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [lifetime]
---

# A named mutex guards one instance; a second launch exits silently

## Decision

Acquire a per-user named mutex before any UI or resource initialization. A
second launch exits 0 without showing anything.

## Context

Login autostart plus manual launches make duplicates likely; two instances
would fight over the hotkey, tray and clipboard listener (§5.1).

## Alternatives considered

- Second launch signals the first to show the palette (rejected for v1:
  needs broadcast IPC, which was out of scope).

## Rationale

Simplest correct behavior; the hotkey and tray already summon the running copy.

## Consequences

- Second-launch IPC stays out of scope until there is a need (projectbrief).
