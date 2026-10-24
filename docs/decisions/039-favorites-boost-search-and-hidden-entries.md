---
id: 039
slug: favorites-boost-search-and-hidden-entries
author: archetypically
status: resolved
created: 2026-10-24
updated: 2026-10-24
tags: [launcher, ranking, persistence]
---

# Favorites earn a search bonus; hidden entries get their own file and view

## Decision

While searching, a favorite's combined score gains a fixed bonus worth 20
fuzzy points (`FAVORITE_BONUS = 2000`). Any root entry except Manage Hidden
Entries can be hidden (Ctrl+Shift+H or its right-click menu): it leaves the
root list and search, and is stored with its name in
`%APPDATA%\smolcast\hidden.json`. The Manage Hidden Entries command lists
hidden entries; Enter or a click unhides one.

## Context

The user asked for favorites to lead the list and be preferred by fuzzy
search, and for a way to hide entries with an interface to unhide them.
Favorites already led the empty query (decision 011) but ranked like any
entry while searching.

## Alternatives considered

- Favorites as a strict tier above every other match (rejected: a loose
  match on a favorite, such as "pa" in Notepad, would beat an exact prefix
  match such as Paint).
- A bonus scaled by the fuzzy score (rejected: harder to predict; a flat
  bonus is one rule in §6.3).
- Hidden IDs inside `favorites.json` (rejected: needs a schema bump, and an
  older build would drop them when it rewrites the file).
- Hidden entries managed in the Settings form or `config.toml` (rejected:
  entries are opaque IDs and come and go with installs; a palette list with
  search and icons is how every other list is managed).
- Hide only apps (rejected: commands and quicklinks clutter the list too).

## Rationale

Measured `SkimMatcherV2` scores for common queries differ by about 8–20
points between two word-start matches and by 30 or more between a strong
and a scattered match, so 20 points prefers favorites among comparable
matches without overriding a much better one. Hidden entries keep the name
they had so the view can still list one whose app is uninstalled; the view
itself cannot be hidden, so nothing is ever lost.

## Consequences

- `matcher::rank` takes the favorites (plan §6.3); hidden entries are
  filtered after ranking, and a hidden quicklink's keyword does nothing.
- Hiding a favorite removes it from favorites.
- `hidden.json` follows favorites' load and save rules (plan §10.4, §10.5).
- Refines decision 011: favorites now also matter while searching.
