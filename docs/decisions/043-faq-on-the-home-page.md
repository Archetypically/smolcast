---
id: 043
slug: faq-on-the-home-page
author: archetypically
status: resolved
created: 2026-10-31
updated: 2026-10-31
tags: [website, seo, aeo]
---

# FAQ as a home page section; comparisons named as alternatives

## Decision

The website's questions and answers are a section of the home page
(`/#faq`), visible at rest as question headings over answer paragraphs, and
described by a `FAQPage` node in the home page's JSON-LD graph whose text
matches the visible answers. The comparison pages keep their `/compare/`
URLs but are named as alternatives: titles read "smolcast vs X: a ... X
alternative", the breadcrumb and footer say "Alternatives", and the hub is
"Windows launcher alternatives, compared".

## Context

The user wanted an FAQ for answer-engine optimization and asked whether it
belongs on the home page or its own page, and wanted the comparison pages
to use the word people search with ("Raycast alternative").

## Alternatives considered

- A separate `/faq` page (rejected: a thin page that repeats the home page;
  the answers are about the product the home page describes, and an extra
  hop splits the signals answer engines weigh).
- Collapsed `<details>` items (rejected: DESIGN.md keeps facts visible at
  rest; collapsed answers read as hidden text).
- Renaming `/compare/` to `/alternatives/` (rejected: churns live URLs,
  social cards and inbound links for no reader benefit).

## Rationale

Answer engines quote short question-and-answer pairs from an authoritative
page; the home page is that page, and its `SoftwareApplication` node gives
the `FAQPage` its subject. Google limits FAQ rich results to a few sites,
so the markup serves machine readers, not a SERP feature; the section has
to earn its place as help for people. Titles carry "alternative" because
that is the query; the headlines stay "smolcast vs X", short and plain.

## Consequences

- Each answer is a real question users ask, true on `main`, and kept in
  step in three places: the section, its JSON-LD and the `llms.txt`
  summary. A behavior change that alters an answer updates all three.
- Page titles stay within 70 characters with " · smolcast" (html-validate
  `long-title`).
- The palette has a "Questions and Answers" row, in `app.js` and the home
  page's no-script list.
