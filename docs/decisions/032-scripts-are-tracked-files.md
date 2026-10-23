---
id: 032
slug: scripts-are-tracked-files
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [scripts, config, security]
---

# Scripts are tracked files, run with saved or typed arguments

## Decision

A script is quick access to a file on disk: `[scripts.<id>]` with `path`,
optional saved `args` and a hotkey. smolcast picks the program from the
extension when it builds the process, not when the file is tracked. The
palette tracks, edits, untracks (the entry only, never the file) and runs
scripts, and "Run with Arguments" appends arguments typed for one run.
Hand-written `interpreter`/`args`/`cwd` commands keep working; the palette
no longer creates them.

## Context

The user asked for Raycast's Script Commands. After a first cut that also
let the palette create free-form commands, they said the useful shape is
"track/untrack/execute script with arguments": a shortcut to a script on
disk, optionally with arguments.

## Alternatives considered

- Store the resolved command (program, flags, path) when tracking
  (rejected: the user's arguments could not be told apart from the flags
  when editing, and a fix to the extension table would not reach existing
  entries).
- A palette editor for any program and argument vector, "Create Script"
  (dropped at the user's request: it read as writing code, and commands
  are rare enough for the config file).
- A watched scripts folder with `@raycast.*` metadata comments (rejected:
  a second source of truth and a directory watcher, out of scope).
- Typed arguments from the root search (`deploy prod`), like quicklinks
  (rejected: §10.3 keeps search text out of processes; an explicit prompt
  makes passing arguments deliberate).
- Running batch files through `cmd.exe /d /c <path>` (rejected: cmd
  re-parses the line, so a typed `a&b` would start `b`; the standard
  library escapes arguments for a batch program itself).

## Rationale

The config holds what the user chose (a file and their arguments); the
mapping to a process is code, kept in one place and covered by tests.
Typed arguments are split without a shell and appended as separate
elements, after the file, so they can never replace it.

## Consequences

- Plan §10.2 schema: `path` optional; `interpreter` and `cwd` default to
  empty and are required only without `path`; a `path` must be absolute.
- Plan §10.3 documents resolution, the argument line rules and the view.
- `.ps1` runs as `-NonInteractive -ExecutionPolicy RemoteSigned`, not
  Bypass (security review): the default policy blocks every script, while
  RemoteSigned still refuses downloaded ones until the user unblocks them.
- The user allowed unit tests for command and argument parsing only
  (`core::script_file`), as a one-time exception to the smoke-test rule.
- Raycast's output modes remain an open idea (activeContext).
