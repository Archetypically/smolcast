---
id: 035
slug: local-crash-log-and-user-filed-reports
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [diagnostics, privacy, palette]
---

# Local crash log, and bug reports the user files

## Decision

Log panics to a small local file through a panic hook, and file bug reports
only through a GitHub issue the user opens, reviews and submits in the
browser. smolcast itself never sends a crash report.

## Context

The user's backlog (2026-10-23) asked for crash logs and a Report a Bug
command. The release build aborts on panic with stripped symbols and no
console, so a crash leaves no trace. The privacy page promises that nothing
leaves the PC on its own apart from the update check.

## Alternatives considered

- Automatic crash upload (Sentry or similar) (rejected: breaks the privacy
  promise, needs a service and an account, and adds a dependency).
- Windows Error Reporting minidumps via `WerRegisterLocalDump` style
  settings (rejected: needs registry keys under HKLM or per-app policy,
  dumps can hold clipboard contents, and they are useless without
  published symbols).
- Backtraces in the log (rejected: the release exe is stripped, so frames
  are bare addresses; the panic location already names file and line).
- The crash report always attached to Report a Bug (rejected: a panic
  message can quote input; the user ticks it after seeing it).

## Rationale

A panic location and message point at the bug in most cases, cost no
dependency, and stay on the user's disk. A prefilled issue URL needs no
token or network code in the app, and GitHub shows the user everything
before it is posted.

## Consequences

- Plan §14 holds the contract; §10.1 lists the new files.
- The privacy page lists `crash.log` and Report a Bug.
- Native crashes are not logged. Revisit with WER only if they show up.
