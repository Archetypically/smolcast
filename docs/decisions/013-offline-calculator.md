---
id: 013
slug: offline-calculator
author: archetypically
status: resolved
created: 2026-10-21
updated: 2026-10-21
tags: [launcher, calculator]
---

# Answer arithmetic and unit conversions inline, offline only

## Decision

The root search evaluates arithmetic and unit conversions (`core::calc`)
and shows a Raycast-style card above the results; Enter pastes the result.
Currencies are excluded.

## Context

Calculator was a brief non-goal "maybe later"; the user asked for Raycast's
inline conversions.

## Alternatives considered

- Currencies with fetched rates (rejected: network dependency and staleness).
- A third-party math/units crate (rejected: the needed scope is small).

## Rationale

Instant, private and dependency-free.

## Consequences

- Answers appear only with an operator or a conversion, so app names and bare
  numbers keep searching apps.
