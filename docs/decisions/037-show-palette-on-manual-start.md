---
id: 037
slug: show-palette-on-manual-start
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [lifetime, autostart, ux]
---

# A start by the user opens the palette; a start at sign-in does not

## Decision

When the user starts smolcast (Start menu, Explorer, a shortcut), the
palette opens on the first frame, through the existing `force_visible`
path, and can be dismissed as usual. The login Run entry passes `--login`
(`autostart::LOGIN_FLAG`). A start with that flag, or with the updater's
`--wait-pid`, stays in the tray. A config error still forces the palette
either way.

Reconciliation compares the whole command, so a Run entry written before
the flag existed differs and is rewritten with it on the next start.

## Context

Starting smolcast showed eframe's forced first frame for a moment, then
hid it (plan §2.1). The user read that as a bug: nothing said the program
had started. A start at sign-in must stay quiet.

## Alternatives considered

- Detect sign-in by system uptime or the parent process (rejected:
  heuristic; a slow sign-in or a launcher in between would guess wrong).
- Show the palette on the first run ever only (rejected: every manual
  start had the same blank-flash problem).
- Show it after an update relaunch too (not chosen: the relaunch follows
  the user's Install click and should land the way smolcast normally
  runs, in the tray).

## Consequences

- A second launch while smolcast runs still exits silently (plan §5.1);
  summoning the running instance needs IPC and is a separate feature.
- A user with `start_at_login` on who upgrades by replacing the exe by hand
  sees the palette at the next sign-in once: the old flagless entry runs,
  then is rewritten. Updater upgrades relaunch quietly and rewrite it first.
- Anyone who added their own Run or Startup-folder entry without `--login`
  gets the palette at sign-in.
