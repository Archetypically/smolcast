---
id: 021
slug: quicklinks-with-keyword-queries
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [launcher, config]
---

# Quicklinks are config tables whose id is the keyword for their query

## Decision

`[quicklinks.<id>]` tables hold a `name` and a `url` (any URL, file or
folder). A `url` containing `{query}` takes a query: typing `<id> <text>` at
the root puts that quicklink first as `Name "text"`, and Enter opens it with
the text percent-encoded (inserted as typed for a drive or UNC path).
Choosing such a quicklink without text, or pressing Tab on it, fills the
search with `<id> ` so the query can be typed. Quicklinks are edited in
`config.toml`, like scripts.

## Context

Tinycast and Raycast have quicklinks with placeholders; the user asked for
them. smolcast already has a web search engine setting with `%s`.

## Alternatives considered

- A separate `keyword` field (rejected: the table id is already a validated,
  unique, space-free name, and scripts use their id as an alias the same way).
- `%s` as the placeholder (rejected: it collides with literal `%s` in URLs
  and reads poorly next to percent-encoding; `{query}` matches Raycast).
- `{clipboard}`, `{date}` and other placeholders now (deferred: the clipboard
  belongs to the clipboard thread, and each needs its own encoding rules).
- An in-palette editor like snippets (deferred: config-only matches scripts;
  add one if quicklinks see real use).

## Rationale

Small, predictable, and keyboard-first: the keyword works like a browser's
search keyword, and nothing is expanded except `{query}`.

## Consequences

- Plan §10.7 holds the schema; `core::quicklink` the expansion rules.
- The web search engine setting stays separate (decision 014).
