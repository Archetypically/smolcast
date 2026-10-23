---
id: 036
slug: markdown-twins-for-agents
author: archetypically
status: resolved
created: 2026-10-23
updated: 2026-10-23
tags: [website, agents, seo]
---

# Markdown twins of each page, no content negotiation

## Decision

Publish every indexable page's `<main>` as a static markdown twin at its
path plus `.md` (`/` is `/index.md`), generated and checked by
`website/scripts/sync.mjs`. Point agents to the twins with a
`rel="alternate"` link, a hidden note in the shared top bar, `llms.txt` and
the sitemap; each twin names its page as canonical in an HTTP `Link` header.

## Context

The user wanted agents that send `Accept: text/markdown` to get markdown.
The site is static on Cloudflare Pages, whose `_headers` and `_redirects`
cannot look at request headers.

## Alternatives considered

- A Pages Function middleware negotiating on `Accept` (rejected for now:
  the site gains runtime code and per-request function calls; it can be
  added later on top of the twins, which it would serve).
- Cloudflare's managed "Markdown for Agents" (rejected: needs a Pro plan the
  user does not want, and converts the layout with less control).
- Hand-written markdown (rejected: drifts from the pages).

## Rationale

Static twins cover agents that follow `llms.txt`, `rel="alternate"` or the
`.md` convention with no runtime and no plan change, and the sync check
keeps them in step with the HTML. The canonical `Link` header keeps search
engines indexing the HTML page rather than its twin.

## Consequences

- `task web:sync` writes the twins and the `_headers` block, then runs
  Prettier; `--check` fails on a stale twin, a missing alternate link or
  sitemap entry, or a twin without a page. Twins are excluded from Prettier.
- `scripts/markdown.mjs` drops the working palette, icons, buttons,
  breadcrumbs and `aria-hidden` content; new page components may need a rule.
- True negotiation at the same URL would need a Pages Function.
