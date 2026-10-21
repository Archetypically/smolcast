---
id: 005
slug: paste-back-without-foreground-tricks
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [paste, input]
---

# Paste back only into the same, still-foreground destination

## Decision

Record the destination window before showing the palette. On commit, write
the clipboard, restore the destination, wait until the user has released all
modifiers, and send Ctrl+V only while that same window is in the foreground.
Otherwise leave the item on the clipboard and say "copied, not pasted".

## Context

Windows can refuse foreground changes, and SendInput cannot reach
higher-integrity windows (§8).

## Alternatives considered

- AllowSetForegroundWindow / AttachThreadInput tricks (rejected: incomplete
  and fragile).
- Synthesizing releases for held modifiers (rejected: corrupts the user's
  real key state).
- Retrying into whatever window is foreground (rejected: can paste into the
  wrong app).

## Rationale

Never pasting into the wrong place matters more than always pasting.

## Consequences

- Clipboard-only is the safe fallback; no automatic elevation.
- Calculator answers and snippets reuse this path.
