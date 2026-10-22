---
id: 022
slug: fallback-searches-after-every-query
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [launcher, search]
---

# Offer web and file search after the results of every query

## Decision

Every non-empty root query ends with two fallback rows, the web search and
then "Search Files", under a `Search for "query"` section header. With no
results they are the only rows and the header is left out. A calculation or
a quicklink query shows no fallbacks. The engine setting and badge from
decision 014 are unchanged.

## Context

Decision 014 showed the web search only when nothing matched, and the file
search row (decision 020) was added after every query. The user asked
whether both should always be present, under a divider below the results.

## Alternatives considered

- Web search only when nothing matches (decision 014; rejected: fuzzy
  matching finds some app for most queries, so whether the row appeared was
  luck).
- A bare divider above the fallbacks (rejected: section headers are the
  palette's only separator inside lists, DESIGN.md).
- Pinning the fallbacks to the bottom of the window (rejected: it costs
  three rows of results on every query).

## Rationale

The same rows in the same place for every query, as in Raycast, at no cost
until chosen; Enter still opens the top match, and with no match it searches
the web as before.

## Consequences

- Supersedes decision 014's "only when nothing matches"; its engine setting,
  custom URL and monogram badge stand.
- The header takes one list row when results come first, like
  "Suggestions"; the fallbacks scroll with the list.
- Plan §5.4 and DESIGN.md (Fallback rows) describe the layout.
