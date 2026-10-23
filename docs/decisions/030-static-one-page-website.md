---
id: 030
slug: static-one-page-website
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-23
tags: [website, design, hosting]
---

# A static one-page website whose hero is the palette

## Decision

smolcast's website is one hand-written static page in `website/public/`,
deployed to Cloudflare Pages with no build step. Its hero is a working
recreation of the palette, and the palette's rows are the page's
navigation: typing filters the sections, Enter scrolls to one.

## Context

The user asked for a clean, tasteful one-pager at
`smolcast.evanlee.engineer` (and perhaps a dedicated domain later), easy to
deploy to Cloudflare Pages, with an `llms.txt` and search metadata. A survey
of Tinycast, Raycast and Alfred (2026-10-22) found the same shape in each:
a short claim, the palette as the hero image (Tinycast recreates it in
HTML), features shown as palette states rather than prose, then privacy,
install and a plain footer. Tinycast also ships JSON-LD and an `llms.txt`.

## Alternatives considered

- A site generator or framework (Astro, Next.js) (rejected: one page needs no
  routing or templates, and a build step is one more thing to break on deploy).
- Screenshots of the real app as the hero (rejected for now: there is no
  Windows capture pipeline, and a static image cannot be tried; screenshots
  can join later).
- A conventional navigation bar (rejected: the palette is the product's own
  navigation idiom, and it lets a visitor try the keyboard model at once).
- GitHub Pages (rejected: the user wants Cloudflare Pages, which also serves
  `_headers` for the CSP).

## Rationale

A recreation built from the app's own tokens (`src/app/theme.rs`) is the
most honest demo short of the exe: it has the same search glyph column,
selection pip, dissolving dividers, edge-row dimming, key caps and footer
action, and the calculator, emoji picker, a quicklink and Toggle Dark Mode
work. The page says it is a recreation. Plain HTML, CSS and one script keep
the page small, fast and portable between hosts.

## Consequences

- The app's colors reach the site through the generated
  `website/public/tokens.css` (decision 033); run `task web:sync` after
  changing `src/app/theme.rs`.
- The domain appears literally in `website/public/`; moving domains is a
  search and replace.
- The social card and the demo film are rendered from HTML in
  `website/media/` with Playwright (`task web:render`).
- The site has since grown more pages, still with no build step; their
  shared chrome and the app's colors are kept in step by decision 033.
