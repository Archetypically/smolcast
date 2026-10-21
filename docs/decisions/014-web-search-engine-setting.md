---
id: 014
slug: web-search-engine-setting
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [launcher, settings]
---

# Fall back to a web search with a chosen engine

## Decision

When nothing matches, the only row searches the web in the default browser.
The engine is a setting (`search_engine`): Google by default, Bing,
DuckDuckGo, Brave Search, Ecosia, Startpage, Kagi, or a custom URL template
with `%s`. The row shows a painted monogram badge for the engine.

## Context

Windows has no system-wide default search engine; each browser keeps its own,
with no reliable way to read it.

## Alternatives considered

- Launch the default browser with a bare query and rely on its engine
  (rejected: browser-specific and unreliable).
- Bundle or fetch engine logos (rejected: licensing and network).

## Rationale

Predictable, private and configurable.

## Consequences

- The Settings picker lists engines by name only (user request).
