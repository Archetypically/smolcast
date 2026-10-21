---
id: 001
slug: eframe-glow-renderer
author: archetypically
status: resolved
created: 2026-09-18
updated: 2026-10-21
tags: [architecture, rendering, footprint]
---

# Render the palette with eframe + Glow

## Decision

The UI is egui on eframe with the Glow (OpenGL) renderer. If the milestone-1
footprint gate had failed, the approved response was a clean cutover to direct
egui-winit/egui_glow, never a raw Direct2D/GDI UI.

## Context

The palette needs an immediate-mode UI that compiles from Linux, stays small,
and is idle while hidden (`systemPatterns.md` §2).

## Alternatives considered

- WPF or WinUI 3 (rejected: heavier idle memory or a 100 MB+ runtime; no Linux builds).
- Hand-written Direct2D/GDI UI (rejected: large surface to own for a palette).
- egui with the wgpu backend (rejected: larger binary and more startup work for no visible gain).

## Rationale

eframe gives windowing, input and text for free; Glow keeps the binary small.
The milestone-1 gate measured ~5.9 MB, 0 ms hidden CPU and ~76 MiB private
memory, inside the thresholds.

## Consequences

- One eframe/winit event loop on the main thread (§1). Mica needs a
  transparent GL surface, decided at startup (§12.7).
- eframe quirks are handled in `app/eframe_host.rs` (first-frame show, the
  100 ms hidden repaint delay worked around by the summon fast path).
