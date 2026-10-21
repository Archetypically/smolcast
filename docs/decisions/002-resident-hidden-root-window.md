---
id: 002
slug: resident-hidden-root-window
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [architecture, lifetime, hotkey]
---

# Keep one resident root window, hidden between summons

## Decision

The eframe root viewport stays alive for the whole process and is hidden, not
destroyed. The global hotkey and tray live on eframe's main event loop; their
callbacks send typed events and explicitly request a repaint.

## Context

The brief suggested a hidden message-only window owning the hotkey. Under
eframe, the window that owns the event loop also delivers hotkey and tray
messages (§1, §13).

## Alternatives considered

- Destroy the palette on hide and recreate it on summon (rejected: the loop
  that delivers the summon would go with it).
- Poll a channel from a hidden window (rejected: polling is not a wake
  mechanism and costs idle CPU).
- A separate hotkey HWND thread (rejected: two loops to coordinate for no gain).

## Rationale

One loop, explicit wakeups, and zero idle work while hidden; measured at 0 ms
CPU over 65 s hidden.

## Consequences

- No polling anywhere; every producer wakes egui (§0, §1.2).
- Window X and Alt+F4 hide; only tray Quit exits (§1.5).
