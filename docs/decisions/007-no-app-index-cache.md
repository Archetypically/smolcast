---
id: 007
slug: no-app-index-cache
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [launcher, performance]
---

# Rescan apps in the background instead of caching the index

## Decision

There is no on-disk app index. Apps are scanned at startup and when a summon
finds the index older than 5 minutes, on a worker; results carry a
generation so a stale scan can never replace a newer one.

## Context

Start Menu and packaged-app enumeration takes a moment but never blocks the
UI (§6.4).

## Alternatives considered

- A persisted index with invalidation (rejected for now: schema and
  invalidation weight without a measured cold-start problem).
- Directory watchers (rejected: idle cost and complexity).

## Rationale

Simpler and always correct; the list stays usable while a refresh runs.

## Consequences

- Add a cache only after measuring cold-start pain.
