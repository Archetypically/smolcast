---
id: 020
slug: file-search-through-windows-search
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [launcher, files, search]
---

# Search files through the Windows Search index, from a fallback row

## Decision

File search queries the Windows Search service's own index over OLE DB
(`Search.CollatorDSO`, `SystemIndex`), matching each typed word as a prefix
of a file-name word; smolcast keeps no index. It lives in its own "Search
Files" view. The root list never mixes files into its ranked results; instead
every non-empty query (not a calculation or a quicklink query) ends with a
"Search Files for "query"" row that opens the view with the query.

## Context

The user wanted file search only if Windows can do it natively without an
index of smolcast's own, and asked whether it should be part of the top-level
search or an option as the user finishes typing. The project brief rules out
directory watchers and an app index cache.

## Alternatives considered

- Own index or crawler (rejected by the user's condition and the brief).
- Enumerating a `search-ms:` shell folder (rejected: no ranking or row limit,
  and enumeration can fall back to a slow crawl of unindexed folders).
- Files ranked among apps in the root list (rejected: an index query on every
  keystroke, results that arrive after the apps and reshuffle the list, and
  document names drowning apps for short queries; the root stays instant and
  deterministic).
- A file row only when nothing else matches, like the web search (rejected:
  fuzzy matching finds some app for most queries, so the row would rarely
  appear).

## Rationale

Windows already indexes the user's folders (and the whole PC in Enhanced
mode), ranks results, and keeps the index current. The fallback row costs one
row and no query until chosen, and the view debounces typing (120 ms) so a
fast typist sends one query.

## Consequences

- Only indexed locations are found; the empty state says where to change
  that. If the Windows Search service is off, the view shows the error.
- Index entries for deleted files are dropped by a `stat` before display.
- Enter opens the item, Ctrl+Enter shows it in Explorer.
- Plan §6.6 holds the contract. Decision 022 puts this row and the web
  search together under a "Search for" header after every query's results.
