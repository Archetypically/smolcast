---
id: 029
slug: desktop-smoke-test-in-ci
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [testing, ci]
---

# One desktop smoke test drives the real exe with real input in CI

## Decision

Regressions are caught by a short smoke test (`tests/smoke.rs`) that launches
the release exe on CI's `windows-latest` desktop, drives it with `SendInput`
keystrokes and clicks, and checks only what a user would see: the palette
shows and hides, text is pasted into a window, a shortcut's program runs.
New tests default to this level (integration or system); unit tests are not
added by default.

## Context

The user (2026-10-22) wanted a sanity check against regressions, not an
exhaustive suite, that holds the bar by using a real binary, a real display
and real interaction. Every Windows behavior that matters (hotkey, focus,
clipboard, paste-back, launching) sits outside the library tests (§0, §11).
An earlier sketch in `activeContext.md` proposed replaying a key script from
inside the app and comparing dumped model state.

## Alternatives considered

- In-app key replay plus model-state dumps (rejected: it skips the hotkey,
  focus and `SendInput` paths that break in practice, and needs test-only
  code in the exe).
- UI Automation (rejected: egui exposes no accessibility tree without
  `accesskit`, decision pending on its own).
- Screenshot diffs (rejected for now: depend on fonts, DPI and theme; the
  test saves a screenshot only on failure, for diagnosis).
- More unit tests (rejected as the default: they prove pure logic, never the
  desktop behavior that regresses).

## Rationale

Observable outcomes (window text, process visibility, a marker file) are
deterministic, so the test polls with deadlines instead of sleeping and needs
no hooks in the app. Real input exercises the same hotkey, foreground and
paste-back paths a user hits. CI runners are throwaway desktops, so the test
may reset smolcast's state and type freely there.

## Consequences

- `tests/smoke.rs` is `#[ignore]` and also refuses to run unless
  `SMOLCAST_SMOKE_DESKTOP=disposable`, so it never types on a maintainer's
  desktop (standing rule). CI's Windows job runs it with `--ignored` and
  uploads a desktop screenshot on failure.
- It covers: single instance, hotkey summon and Esc, calculator paste,
  clipboard history paste, emoji paste, launching a Start Menu shortcut.
  Add a step only for a user path that has regressed or is hard to verify by
  hand; keep it short.
- The runner has no GPU and only OpenGL 1.1, which Glow refuses; CI puts
  Mesa's software OpenGL (mesa-dist-win, pinned) beside the exe for the
  smoke step only. The shipped exe and release artifact are unchanged.
- Existing core regression tests (§12) stay; this changes the default for
  new tests, not the old ones.
- §11's "No CI job claims runtime/UI behavior" is narrowed: the smoke test is
  a regression signal, not the §12 release gate on a clean Windows 11 account.
