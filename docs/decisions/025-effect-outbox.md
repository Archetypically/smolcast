---
id: 025
slug: effect-outbox
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [architecture]
---

# The model hands the host one ordered queue of effects

## Decision

`AppModel::take_effects` returns a `Vec<app::effect::Effect>`, and the host
executes it in order with one `match` (`EframeApp::run_effects`). It replaces
twenty host-facing `take_*` accessors. The model pushes its own effects
(clipboard commands, the paste finisher, update work, config save, reload,
apply, open, trial hotkey) straight into the queue; the views keep their
request slots and queues, and `take_effects` appends them after the model's
own, in a fixed order.

## Context

The host drained the model through about twenty accessors, called in an
order spread over three functions. Decision 024 deferred the outbox until it
could be done without losing the slots that coalesce (one scan, one launch,
the latest usage and favorites snapshot) or the paste finisher's timing. An
ordered, recordable effect stream is also what the parked UI smoke-test idea
needs to compare a run's output.

## Alternatives considered

- Views push into a shared outbox directly (rejected for now: the pushes sit
  deep in painting code, so every `ui` signature would take the outbox, and
  the launcher's launch slot doubles as its "launch already queued" guard).
- One effect per bool or slot, deduplicated in the queue (rejected: a
  `Vec` cannot express "latest wins" without searching it; the views'
  `Option` slots already do).
- Keep the accessors (rejected: the order lived in the host, far from the
  code that relies on it, and each new request meant a field, an accessor,
  a host call and a doc comment).

## Rationale

The model's own effects keep their push order, so a paste's clipboard write
precedes its finisher, and the finisher is pushed while the write receipt is
reduced in `step` and started in that same host step, as before (it now
starts before that step's history store write, which it does not depend on).
Coalescing stays where it was, in the views' slots; the host keeps only the
latest config edit, reload and trial key of a drain, as the model's old
slots did. A config save still waits for the refresh worker slot: the host
holds the latest edit in `queued_save` and starts it when the slot is free,
then any refresh, then the reload and the trial key, the old order.

## Consequences

- A new kind of host work adds an `Effect` variant and one match arm; the
  compiler finds the missing arm.
- `take_relaunch` stays an accessor: it is read once, at exit.
- Plan §1.2 describes the outbox. Runtime behavior is meant to be unchanged;
  it needs a Windows pass like any host change.
