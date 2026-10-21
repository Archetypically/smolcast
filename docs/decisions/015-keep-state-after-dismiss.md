---
id: 015
slug: keep-state-after-dismiss
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [ux]
---

# Keep palette state briefly after a dismiss

## Decision

A dismissal (click-away, Esc, close) keeps the view, query, selection and
drafts for `keep_state_secs` (default 30, 0 disables). A completed launch,
paste or web search starts the next summon fresh.

## Context

Accidental dismissals lost typed queries and unsaved edits.

## Alternatives considered

- Always reset (rejected: the reported problem).
- Never reset (rejected: stale views confuse the next summon).

## Rationale

Matches Raycast's "pop to root after" behavior.

## Consequences

- `AppModel::hide` keeps state; `AppModel::finish` clears it.
