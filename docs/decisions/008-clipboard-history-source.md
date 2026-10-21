---
id: 008
slug: clipboard-history-source
author: archetypically
status: resolved
created: 2026-09-24
updated: 2026-10-21
tags: [clipboard, privacy]
---

# Prefer Windows clipboard history; keep smolcast's own encrypted

## Decision

Clipboard History shows Windows clipboard history (Win+V) when it is on, and
smolcast's own history otherwise (`clipboard_source = auto`, overridable).
smolcast's history persists across restarts, encrypted with DPAPI per user,
unless `clipboard_persist = false`.

## Context

Running a second history alongside Windows' duplicates data and confuses users
about which one they see (§7.0, §7.2).

## Alternatives considered

- Always smolcast's own history (rejected: duplicates Win+V for people who use it).
- Session-only history (rejected as the default: losing history on restart
  was unwanted; still available).

## Rationale

Respect the system feature when the user has chosen it; otherwise provide one
that is private at rest.

## Consequences

- Windows exclusion hints are honored; contents never enter logs.
