---
id: 011
slug: favorites-stored-separately
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [launcher, persistence]
---

# Store up to 5 favorites in their own roaming file

## Decision

Favorites live in `%APPDATA%\smolcast\favorites.json`, at most 5 shown.
An unreadable file is renamed aside, never overwritten. Favorites whose app is
not installed stay saved, hidden and uncounted.

## Context

Pinned entries are user-curated, unlike launch counts.

## Alternatives considered

- In `usage.json` (rejected: usage is a cache that is reset when corrupt).
- In `config.toml` (rejected: frequent UI edits through the
  comment-preserving editor for an ordered list of opaque IDs).
- No limit (rejected: favorites would push suggestions off a 7-row list).

## Rationale

Keeps user data durable and roaming without burdening the config file.

## Consequences

- Written by the state writer with the newest snapshot winning.
- Ctrl+Shift+F mirrors Raycast's shortcut.
