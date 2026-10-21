---
id: 017
slug: docs-bank-and-public-readme
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [process, docs]
---

# Keep project context in docs/, the README public

## Decision

Project context uses the memory-bank pattern in `docs/` (projectbrief,
productContext, systemPatterns, techContext, activeContext, decisions/), per
the `.agents/skills` memory-bank and making-decisions skills. README.md is a
public product page. No machine-specific paths are tracked; they come from a
gitignored `.env`.

## Context

Status, contracts and notes had piled into README.md and a 1,400-line plan,
and agent memory was a local untracked file.

## Alternatives considered

- A hidden `.memory-bank/` directory (rejected: this repo is public, so the
  context should be visible).
- Keep `plans/IMPLEMENTATION_PLAN.md` (rejected: mixed spec, status and
  decisions; now split into systemPatterns, activeContext and decisions).

## Rationale

One place per kind of knowledge; contributors and agents start from the same
committed context.

## Consequences

- Code cites `docs/systemPatterns.md` sections as "plan §N".
- The local MEMORY.md is retired.
