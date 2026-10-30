---
id: 042
slug: text-symbols-from-cldr-annotations
author: archetypically
status: resolved
created: 2026-10-30
updated: 2026-10-30
tags: [emoji, data, search]
---

# The emoji picker also lists CLDR-annotated text symbols

## Decision

The picker becomes "Emoji & Symbols". After the emoji, the generated table
holds every character CLDR 48.2's English `annotations/en.xml` names that
emoji-test.txt does not list (arrows, math operators, currency, punctuation,
superscripts: 369 entries), in en.xml order, in a trailing "Text Symbols"
group. Names come from the `tts` annotation and keywords from the others, so
search works the same as for emoji. The data stays pinned to the two
sources plan §9 already fetches.

## Context

The user asked for symbols in the picker and for its name to be
"Emoji & Symbols", like the Windows emoji panel's symbol tabs.

## Alternatives considered

- Whole Unicode blocks (Arrows, Mathematical Operators, Letterlike Symbols,
  Currency) from `UnicodeData.txt` (rejected: thousands of rarely wanted
  characters, a third pinned source, and only formal uppercase names with
  no keywords to search by).
- A hand-picked list in the generator (rejected: unpinned, and every
  addition is a judgment call).

## Rationale

CLDR's annotated set is curated for exactly this use (it is what keyboard
pickers search), arrives with search keywords, and is already a pinned,
hashed input, so the table stays byte-for-byte reproducible.

## Consequences

- `scripts/generate_emoji.py` is at generator version 2; the table grows to
  4,313 entries. Symbols are single code points with no skin tones.
- The command is titled "Emoji & Symbols" (aliases "Search Emoji",
  "Search Symbols"); its usage key stays `emoji`, so history carries over.
- Symbols draw through the same DirectWrite path; characters Segoe UI Emoji
  lacks rely on DirectWrite's system font fallback (unverified on Windows).
