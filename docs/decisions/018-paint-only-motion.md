---
id: 018
slug: paint-only-motion
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [ui, motion, accessibility]
---

# Animate the palette's content in paint, not the window

## Decision

Spawn, dismiss, navigation and selection animations transform and fade what
egui paints (`Ui::with_visual_transform`, opacity); the OS window is shown and
hidden at once, as before. Dismissals fade out for 90 ms before hiding;
completed actions hide immediately. A `reduce_motion` setting (default:
follow Windows' Animation effects) keeps fades and drops movement.

## Context

The user asked for quick spawn-in and spawn-out animations and transitions
between views, without losing snappiness. The palette is an undecorated,
always-on-top, optionally Mica-backed glow window that a fast summon path
shows DWM-cloaked (§2.1), and paste-back depends on hiding promptly (§8.2).

## Alternatives considered

- Fade the whole window with `WS_EX_LAYERED` + `SetLayeredWindowAttributes`
  (rejected: layered windows break the Mica backdrop and have a history of
  problems with OpenGL surfaces; untestable outside Windows and risky for the
  summon fast path).
- Rely on DWM's own show/hide transitions (rejected: they do not apply to this
  popup-style window consistently, and cannot be timed or reversed).
- Animate with egui layer transforms (`Context::set_transform_layer`)
  (rejected: they also transform input, so clicks during an animation would
  land on moving targets, and the transform is sticky state to reset).
- Fade out on every hide, including after a launch or paste (rejected: the
  target window would wait 90 ms, and paste-back restores focus right after
  the hide).

## Rationale

Paint-only motion cannot slow input: the search field has focus and takes
keys on the first frame, and clicks hit final positions. It needs no new Win32
calls beyond reading `SPI_GETCLIENTAREAANIMATION`, works the same with and
without Mica, and costs nothing at rest because egui repaints only while an
animation runs. The tradeoff is that the window frame and backdrop appear and
vanish at once while the content animates inside them.

## Consequences

- `core::motion` owns the timing tokens and the interruptible `Tween`
  (host-tested); `app::motion` holds the reduced-motion flag, highlight glides
  and hover fades. DESIGN.md has the Motion table.
- `AppModel::dismiss` fades and keeps the paste destination until the window
  hides, so a summon during the fade returns in place; `hide` is immediate.
  The host paints while `is_painted()` and starts the appear clock after a
  fast-path summon is uncloaked.
- Content is always wrapped in the transform scopes, even at rest, so egui
  widget ids (and the search field's focus) never change mid-session.
- New `[settings]` key `reduce_motion` (§10.2, §12.7).
