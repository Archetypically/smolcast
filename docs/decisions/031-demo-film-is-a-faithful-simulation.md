---
id: 031
slug: demo-film-is-a-faithful-simulation
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [website, marketing]
---

# The demo film is a faithful simulation of the app

## Decision

The website's demo film (also its `og:video` and the clip for social
posts) is a scripted simulation, `website/media/film.html`, drawn to the
app's own measurements, copy and results: labels and commands from
`src/app`, the calculator answer and emoji search results produced by
`src/core`, and `core::motion`'s timings. It is recorded with Playwright
(`task web:film`). The page calls it a simulation.

## Context

The first film recorded the website's palette recreation. Checked against
the app, it showed things the app does not do or say: "Copy" where the app
pastes, the site's sections instead of apps, a fallback row the app lacks,
invented confirmations, other fonts and icons. The user wanted the film as
close to the end-user experience as possible so the site does not
over-promise.

## Alternatives considered

- Keep the recreation, captioned (rejected: it showed behavior the app does
  not have).
- Record the real exe on CI's Windows desktop (`tests/film.rs` driven like
  the smoke test, captured with ffmpeg) (tried on 2026-10-22, then dropped by
  the user for simplicity: a new workflow and test to maintain, runs that
  take minutes, and a runner without a GPU or Mica).
- Record by hand on the maintainer's desktop (rejected: not reproducible,
  and no scripted input runs there).

## Rationale

Everything the film shows is taken from the source rather than imagined:
row names and types, footer actions ("Paste Answer", "Open Command",
"Search Web"), the fallback rows, paste-back into the window you came from,
the clipboard view's list, preview and buttons, the 12-column emoji grid and
its details line, the calculator returning nothing until the conversion is
complete. A page renders it deterministically, so it can be re-recorded
whenever the app changes.

## Consequences

- What it cannot copy is stated in `website/media/film.js` and on the page:
  each app's own icon (plain drawings stand in), and Windows' fonts and
  emoji (Selawik, Fluent UI System Icons and Fluent Emoji, open-licensed
  Microsoft counterparts, in `website/media/film/`).
- When the app's labels, commands, layout or motion change, update
  `film.js`/`film.css` and re-record. The emoji results in
  `media/film/emoji-search.json` come from `core::emoji::search`.
- Music is synthesized by `website/scripts/music.mjs`, so no third-party
  audio license applies.
- Motion graphics frame the simulation and never enter it: a backdrop
  stippled with small s. marks drifting behind the desktop, a caption per
  chapter, the key caps, a slow camera lean while the palette is open, and
  a title card. The app's windows, copy and timings stay as above. Mica
  over that plain backdrop is drawn as a near-opaque tint, as Windows'
  blur would leave it.
- Each cut (wide for the page, square and vertical for social posts) starts
  and ends on the same title-card frame, and lasts whole 12 s music phrases
  with the music's tail folded onto its start, so it loops seamlessly. The
  page holds the card until the phrase ends; keep the script under that.
