---
id: 026
slug: embedded-icon-and-version-resource
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [build, release, design]
---

# Embed the app icon and version information in the exe

## Decision

`build.rs` compiles `assets/smolcast.ico` (icon resource 1) and a
VERSIONINFO block into `smolcast.exe` through the `winresource` build
dependency, for Windows targets only. Version, author, site and description
come from `Cargo.toml` (`version`, `authors`, `homepage`, `description`).
The tray loads resource 1 at the small-icon size for the system DPI at
startup (a later scale change keeps that frame until restart).
The icon is "s." on a dark squircle; `assets/smolcast.svg` is its source and
`scripts/generate_icon.py` (cairosvg + Pillow, maintainer-only) renders the
committed `.ico` and 256 px `.png`. The release workflow refuses to
publish an exe whose embedded product version differs from the tag.

## Context

The released exe showed the generic exe icon in Explorer and a blank
Properties > Details tab, and the tray drew a 32 px placeholder in code. The
user asked for the logo on the file, refreshed, and for author attribution
and a link to evanlee.engineer in the file properties
(smolcast.evanlee.engineer later).

## Alternatives considered

- Logo: a palette pictogram (search field over result rows; the first
  version, felt generic), a floating window with a caret (read as a battery
  when small), `>_` (the stock terminal icon), a slash key (blurs in the
  tray), a hand-drawn cast-line s, and a ripple (reads as a record button).
  The user chose "s.".

- Hand-written `.rc` with `embed-resource` (rejected: version numbers would
  have to be split into macros by hand; `winresource` fills them from Cargo).
- Calling `rc.exe`/`windres` from `build.rs` ourselves (rejected: tool
  discovery for MSVC and MinGW is what the crate already does).
- Keeping the drawn tray icon (rejected: two logos to keep in step, and no
  DPI-matched frames).

## Rationale

One `.ico` serves Explorer, Properties, the taskbar and the tray, and the
version shown in Properties is the one the updater compares (§13). The
build dependency adds nothing to the exe beyond the resources.

## Consequences

- Explorer's Details tab shows Description, Product name, versions,
  Copyright and Original filename only; Company, Comments and Homepage are
  stored but hidden, so the site also appears in the copyright line.
- The s is Inter Display Bold (SIL OFL 1.1) converted to outlines, so the
  SVG needs no font; outlines in a logo are not a derived font.
- When smolcast.evanlee.engineer is live, change `homepage` in `Cargo.toml`.
- The GNU cross-build needs `x86_64-w64-mingw32-windres` (MinGW binutils,
  already installed with the MinGW compiler); MSVC builds use the Windows
  SDK's `rc.exe`.
