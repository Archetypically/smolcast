---
id: 004
slug: clipboard-bounds-and-normalized-images
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [clipboard, memory]
---

# Bound clipboard history by bytes and normalize images to RGBA

## Decision

smolcast's history holds at most 100 entries, 8 MiB per entry and 64 MiB in
total. Oversize items are rejected without evicting anything; exact
duplicates are promoted; eviction is oldest first. Images are normalized to
bounded RGBA and written back as CF_DIBV5. All clipboard access runs on one
message-only HWND thread.

## Context

Item counts alone do not bound memory when images are involved, and clipboard
access races with other apps (§7).

## Alternatives considered

- Count-only ring buffer from the brief (rejected: unbounded image memory).
- Keep the source app's raw formats (rejected: no deterministic dedup, memory
  accounting, thumbnails, or Paint compatibility).
- Clipboard calls from whichever thread needs them (rejected: ownership and
  retry races).

## Rationale

Deterministic memory, dedup and paste results; the limits are user-adjustable
within validated ranges.

## Consequences

- Limits live in `[settings]` and are validated (§7.5, §10.2).
