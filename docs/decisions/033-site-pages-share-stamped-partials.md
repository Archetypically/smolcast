---
id: 033
slug: site-pages-share-stamped-partials
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [website, design, seo]
---

# Site pages share stamped partials and generated tokens

## Decision

The website grows past one page (a privacy policy and comparison pages)
without a build step. Each page in `website/public/` is complete,
deployable HTML. The parts every page shares (head links, top bar,
footer) live once in `website/partials/` and are stamped into marked
regions of each page by `scripts/sync.mjs`. The app's colors are generated
into `public/tokens.css` from `src/app/theme.rs` by the same script. CI
runs it with `--check`, which fails on any drift, and on an indexable page
missing from `sitemap.xml` or `llms.txt`. On every page but home, Ctrl+K
opens the palette over the page, and its rows link between pages.

## Context

The user asked for a privacy page and SEO comparison pages (decision 030
had a single page), asked how several pages would keep one look, and
asked that a theme change not be hand-copied. The site's CSS was already
one file. The remaining copies were each page's chrome and the app's
colors, which were duplicated from `theme.rs` into `styles.css` and the
film's CSS.

## Alternatives considered

- A static site generator (Eleventy, Astro) or a build into `dist/`
  (rejected: a few pages do not need templates, and a build makes the
  deployed files differ from the reviewed ones. It also adds a step that
  can fail at deploy time).
- Hand-copied chrome with no check (rejected: it drifts silently).
- Client-side includes, with JavaScript injecting the header and footer
  (rejected: crawlers and AI fetchers that run no script would see pages
  without navigation, and the pages would flash on load).
- Reading the tokens at runtime from a shared JSON file (rejected: it
  needs a script to style a page).

## Rationale

Stamping keeps what decision 030 valued: `public/` is exactly what ships,
and is readable and diffable as-is. It also gives the shared parts a
single source. The check turns drift into a CI failure instead of a
review chore. Generating the tokens from `theme.rs` makes the app the one
source of truth for its palette, as DESIGN.md already said it was.

## Consequences

- A new page copies an existing page's skeleton, keeps the
  `<!-- partial:NAME -->` markers, and gets a sitemap entry and an
  `llms.txt` link. The check enforces both.
- Partials are written as Prettier formats them at a page's four-space
  indent, and `partials/` is excluded from Prettier.
- `tokens.css` is never edited by hand. Web-only tokens (page, muted,
  shadows) stay in `styles.css`.
- Comparison pages state the date and sources they were checked against,
  and need rechecking when the other products change.
