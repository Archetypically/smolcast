---
id: 016
slug: broken-shortcuts-recycle-bin
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [launcher, files]
---

# Offer to recycle only provably broken Start Menu shortcuts

## Decision

Broken shortcuts (an unloadable .lnk, or a local target missing on an
attached drive) are listed under the root list with "Move to Recycle Bin" and
"Ignore". Removal uses IFileOperation with undo.

## Context

The scan used to report only "Skipped N broken shortcuts" with no remedy.

## Alternatives considered

- Permanent delete (rejected: not undoable).
- Repair targets (rejected: a broken shortcut points to an uninstalled app).
- Treat every skipped shortcut as broken (rejected: COM failures or an
  unplugged drive would offer working shortcuts for deletion).

## Rationale

Useful cleanup that is always reversible and never guesses.

## Consequences

- All-users shortcuts need Windows' own elevation prompt.
