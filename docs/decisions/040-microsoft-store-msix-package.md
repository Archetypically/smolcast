---
id: 040
slug: microsoft-store-msix-package
author: archetypically
status: resolved
created: 2026-10-25
updated: 2026-10-25
tags: [release, store, packaging, updates]
---

# Ship the same exe to the Microsoft Store as an unvirtualized MSIX

## Decision

The Microsoft Store gets an MSIX built by `scripts/package_msix.ps1` around
the release exe, with no separate build: the exe detects package identity at
run time. In the package it never self-updates (the Store does), and "Start
smolcast when you sign in" uses the manifest's startup task instead of the
Run key. The manifest turns off AppData and HKCU write virtualization
(`desktop6`, restricted capability `unvirtualizedResources`). `release.yml`
packages every release (not prerelease) tag and submits it with the
Microsoft Store Developer CLI, from a `microsoft-store` environment holding
the Partner Center credentials. The Store signs the package.

## Context

The user reserved the name in Partner Center and asked for MSIX packaging
and a pipeline that sends tagged releases to the Store (2026-10-25).
smolcast is a portable exe that updates itself (decision 019) and registers
at sign-in through the Run key (§12.7), neither of which works inside a
package: the install folder is read-only and versioned, Store policy forbids
updating outside the Store, and a Run entry starting the exe directly runs
it without package identity.

## Alternatives considered

- Default virtualization (rejected: a packaged desktop app's children run in
  its package, so apps and scripts launched from smolcast would write new
  AppData files and HKCU keys into smolcast's private copy, invisible to
  themselves when started elsewhere and deleted on uninstall).
- Windows 11's per-folder exclusions (`virtualization:ExcludedDirectories`)
  (rejected: only for named folders under AppData, and they take precedence
  over the full opt-out, while launched programs write anywhere).
- Breaking launched processes out of the package with
  `PROC_THREAD_ATTRIBUTE_DESKTOP_APP_POLICY` (rejected for now: needs
  `CreateProcess`, while launches go through `ShellExecuteEx` and
  `ActivateApplication`; revisit if certification refuses the capability).
- An MSI/EXE Store listing pointing at the GitHub exe (rejected: needs a
  signed installer (decision 034, not approved yet), Store updates would not
  apply, and the self-updater would fight the Store).
- A separate Store build behind a Cargo feature (rejected: two binaries to
  test; one runtime check does it).

## Rationale

One exe means CI's smoke test and the user's Windows checks cover the Store
build too. Turning virtualization off makes the packaged smolcast behave like
the portable one for everything it starts, and keeps one config folder for
both. The Store signs MSIX packages itself, so the Store path does not wait
on SignPath.

## Consequences

- Plan §15 holds the contract; `docs/techContext.md` the one-time setup.
- `unvirtualizedResources` and `runFullTrust` are restricted capabilities:
  each submission explains them in Partner Center ("Submission options"),
  and certification may refuse the first. If it does, see the breakaway
  alternative above.
- Uninstalling the Store build leaves `%APPDATA%\smolcast` and
  `%LOCALAPPDATA%\smolcast` behind, like the portable build.
- The Store CLI only updates a product that already has a published
  submission, so the first submission (listing, age rating, privacy URL,
  package) is made by hand with the MSIX `task msix` builds.
- Prerelease tags never reach the Store; an MSIX version is
  `major.minor.patch.0`, each part at most 65535.
