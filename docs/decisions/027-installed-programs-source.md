---
id: 027
slug: installed-programs-source
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [launcher, windows]
---

# Index installed programs from the Uninstall registry keys

## Decision

The launcher scan reads the Uninstall keys (HKLM, HKLM WOW6432Node, HKCU) as
a fourth, lowest-priority source. A row becomes an app when it is not a
`SystemComponent`, has no `ParentKeyName` or `ReleaseType`, and its
`DisplayIcon` names an existing `.exe` on a fixed drive that is not in a
cached-installer, Windows or `WindowsApps` folder and whose name does not
read as setup, uninstall, redist, maintenance, installer, update or dpinst.
A row is dropped when its exe sits in the folder of a Start Menu shortcut's
target, or under it unless that folder is shared by many programs (a drive
root, a profile, Program Files, AppData and the like). It launches the exe in
its own folder. The exact lists are in plan §6.1.

## Context

Raycast found Overwatch and Battle.net; smolcast did not. Neither has a
Start Menu shortcut on the user's machine (the "Overwatch Beta/Test" folders
are empty and `Get-StartApps` lists neither); both exist only as Uninstall
rows whose `DisplayIcon` is the game or client exe. Of about 120 visible rows
on that machine, most are runtimes, redistributables, drivers and setup exes.

## Alternatives considered

- User-chosen folders to scan, as Tinycast's search scopes do for `.app`
  bundles (rejected for now: needs setup per game, and a Windows folder holds
  helper and updater exes beside the app; could come later as a fallback).
- Non-packaged AppsFolder rows (rejected: they mirror Start Menu shortcuts,
  so they would not add Overwatch).
- Steam/Battle.net library parsing (rejected: one parser per store).
- Keeping every visible row (rejected: drivers and runtimes flood the list).
- Dropping only exact target matches (rejected: Edge registers a versioned
  subfolder exe and SteelSeries a sibling of its shortcut's exe, so both
  appeared twice).

## Rationale

Installers maintain these keys and "Installed apps" reads them, so smolcast
keeps no index of its own (decision 007 holds). The filter was checked
against the user's machine: 16 rows pass, 12 already in the Start Menu are
dropped, and Overwatch, Battle.net, 1Password and Windows 11 Installation
Assistant are added. The name list is the maintenance cost; a new kind of
junk row may need a word added.

## Consequences

- `core::installed_programs` holds the filter (host-tested);
  `win::installed` reads the keys; `AppSource::InstalledPrograms` and
  `LaunchSpec::Executable` are new (plan §6.1–§6.2).
- Launching the exe is what the row's icon names, not always what the store
  would run; Overwatch's exe hands off to Battle.net. Unverified on Windows.
- Only `AppId::Classic` shortcuts cover a row. An MSI-advertised shortcut
  (no resolvable target) or a packaged app's shortcut does not, so its
  Uninstall row can still appear as a second entry.
- Programs on removable or network drives are not listed.
