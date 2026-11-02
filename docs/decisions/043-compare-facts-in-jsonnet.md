---
id: 043
slug: compare-facts-in-jsonnet
author: archetypically
status: resolved
created: 2026-11-02
updated: 2026-11-02
tags: [website, data, tooling]
---

# Comparison page facts live in Jsonnet

## Decision

The facts behind the comparison pages (decision 033) live in
`website/scripts/compare.jsonnet`: one object per tool, its sources, the
row layout, the related-page sentences, and the hub's title and
description. `website/scripts/compare.py` is the renderer only; it
evaluates the Jsonnet file with the Python binding and builds the pages.
The small shaping helpers the facts need (marking smolcast's winning
cells, the "Not listed" default, the `<kbd>` hotkey builder) are Jsonnet
functions in the same file, so every computed cell is still written once.

## Context

compare.py opened with roughly 260 lines of Python literals (the fact
tables and page copy) before the renderer started. The facts are data
plus a little computation: most pages reuse smolcast's column with a few
cells marked as wins, several cells are built from the hotkey helper, and
some copy concatenates those results. Editing a fact meant editing code,
and the file read as a program first and a fact table second.

## Alternatives considered

- Plain JSON or YAML for the facts. Rejected: neither can express the
  helpers, so the marked-win copies of smolcast's column would have to be
  baked in per page, by hand or by a generator step that reintroduces the
  code the split is meant to remove.
- Keep the facts in Python. Works, and needs no new dependency, but the
  facts stay embedded in a program and every other data file in the
  project is declarative.

## Rationale

Jsonnet is the data format whose expression level matches the content:
objects and arrays for the facts, functions for the three helpers,
comprehensions for the marked columns. The renderer shrinks to HTML
building, and a fact edit is a data edit. The conversion was verified by
rendering before and after: every page is byte-identical except the two
title fixes below.

## Consequences

- `task web:compare` now needs the Jsonnet Python binding
  (`pip install jsonnet`) wherever it runs.
- The "alternatives" title pass and the 8 MB size correction, landed
  on main in the previous all-Python generator, are carried in the
  Jsonnet data. The conversion was verified against that generator:
  the Jsonnet pair renders byte-identical pages.
