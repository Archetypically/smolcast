---
name: making-decisions
description: Records smolcast design decisions as individually numbered markdown files in docs/decisions/, with a status vocabulary and template. Use when a durable design choice is made, revised, or reversed.
---

# Making Decisions

Durable design decisions live in `docs/decisions/`, one file each, so
the reasoning behind a real choice survives context loss. A decision file is
not a work log.

## When to create one

Mint decisions sparingly. All three must hold:

- **Durable**: it constrains future work beyond the task at hand
  (architecture, data formats, platform behavior, product rules, conventions).
- **Rationale would be lost**: someone reading the code could not reconstruct
  why this shape was chosen.
- **A real alternative was rejected**: if no genuine option was weighed, it was
  task execution, not a decision.

Not decisions: implementing an agreed feature, fixing a bug, a small naming or
number tweak (record those in `activeContext.md` or the PR), or follow-through
of an earlier decision (update that decision's Consequences instead).

## IDs and files

- Filename: `<NNN>-<kebab-case-slug>.md`, where `<NNN>` is the next free
  zero-padded number in `decisions/` (`ls docs/decisions`). Never reuse
  a number.
- `author`: the GitHub login driving the session
  (`gh api user --jq '.login'`, lowercased; fall back to `git config user.name`).

## Status vocabulary

| Status | Meaning |
| --- | --- |
| `draft` | Proposed; not yet agreed. |
| `open` | Agreed; not yet implemented. |
| `resolved` | Decided and implemented, or definitively closed. |
| `superseded` | Replaced; set `superseded_by` to the new ID. |
| `cancelled` | Considered and dropped; kept so the reasoning is not lost. |

To reverse or replace a decision, mark the old file `superseded` (with
`superseded_by`) and create a new one; never rewrite history in place. To
refine one, edit it and bump `updated:`.

## Template

```markdown
---
id: <NNN>
slug: <kebab-case-slug>
author: <github login>
status: <draft | open | resolved | superseded | cancelled>
created: YYYY-MM-DD
updated: YYYY-MM-DD
superseded_by: <optional ID>
tags: [comma, separated]
---

# <Title>

## Decision

<One to three imperative sentences.>

## Context

<How this came up; the problem; constraints.>

## Alternatives considered

- <Option> (rejected: <why>)

## Rationale

<Why this wins; tradeoffs accepted.>

## Consequences

- <What is now true; follow-ups; related decisions or plan sections>
```

## Checklist

- [ ] Passes the bar: durable, rationale worth keeping, real alternative weighed.
- [ ] Filename and `id` match; `author`, `created`, `updated` filled.
- [ ] All five sections present.
- [ ] A superseded decision is marked with `superseded_by`.
- [ ] Any contract it changes is updated in `docs/systemPatterns.md` in the same change.
