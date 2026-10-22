---
id: 024
slug: shared-request-cache-and-list-cursor
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [architecture, ui]
---

# One request cache and one list cursor for every view

## Decision

Views cache worker-produced textures in `core::request_cache::RequestCache`
and keep their selection and scroll position in `core::list_cursor::ListCursor`.
An effect outbox to replace the model's `take_*` accessors is deferred, and a
shared worker abstraction is not adopted.

## Context

An earlier review found four hand-written texture caches (launcher icons,
file icons, emoji glyphs, clipboard thumbnails) and five copies of the
selection/first-row/wheel state. The caches had drifted: the launcher and
emoji views cached an icon or glyph fetched before a DPI change at the old
size, while the file view guarded only part of that race. The same review
proposed an effect outbox and a worker abstraction.

## Alternatives considered

- Keep per-view caches and fix each one (rejected: the drift is how the
  stale-size bug happened, and every new view would copy it again).
- Put the cache and cursor in `app/` (rejected: `app` builds only for
  Windows, so neither would get host tests; generic over the value type,
  both fit `core` without egui, plan §4).
- Effect outbox now (deferred: about 20 accessors drained in a fixed order,
  some coalescing, with the paste finisher timing-sensitive; it needs its
  own Windows-verified change after the pending Windows pass).
- Worker abstraction (rejected: the 14 threads differ in COM apartments,
  cancellation and latest-only semantics, so a shared type would be thin or
  leak those differences).

## Rationale

The cache owns the edge check: `accept` takes a result only for a request
still in flight at the current edge, so a view that calls it before
`insert` (all of them except clipboard thumbnails, which have no edge)
cannot reintroduce the stale-size bug. The cursor's math was already shared
as free functions; the struct removes the repeated fields, and the
launcher's card offset and the emoji grid still use the free functions for
their special cases.

## Consequences

- `AppEvent::IconReady` and `AppEvent::EmojiGlyph` carry the edge they were
  rendered at.
- A new cached texture or windowed list uses these types.
- The outbox was adopted later, in decision 025.
