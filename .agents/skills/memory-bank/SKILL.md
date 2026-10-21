---
name: memory-bank
description: Manages smolcast's project memory in docs/ (active context, decisions, architecture contracts). Use at session start to load context, and after significant changes to record them.
---

# Memory Bank

smolcast keeps its shared project context in markdown under `docs/`. This repo
is public, so the bank lives in the visible `docs/` directory rather than a
hidden one, and it is committed: every contributor (human or agent) starts
from it, so it must be precise and current.

## Structure

| File | Holds |
| --- | --- |
| `docs/projectbrief.md` | Scope, goals, non-goals. The source of truth for what smolcast is. |
| `docs/productContext.md` | Why it exists, how it should feel, and what each feature does for the user. |
| `docs/systemPatterns.md` | The normative architecture contracts (threads, events, launcher, clipboard, paste-back, schemas, milestone gates). Code cites its sections as "plan §N". |
| `docs/techContext.md` | Toolchain, dependencies, setup, build/stage/run, and platform constraints. |
| `docs/activeContext.md` | Current focus, uncommitted or unverified work, next steps, open questions, traps. |
| `docs/decisions/` | One file per durable design decision; see the **making-decisions** skill. |
| `docs/DESIGN.md` | Visual design: layout, tokens, components, rules. |

`README.md` is the public-facing product page: no status, contracts or
internal notes go there.

## Reading

Read `docs/activeContext.md` at the start of every session, by section
(`grep -n '^## '`, then read the sections you need). Read
`docs/systemPatterns.md` sections only for the area you are touching; it is
long by design. To find a decision, list `docs/decisions/` or search it; there
is no index file.

## Updating

Update the bank when:

1. a significant change lands (feature, contract change, new constraint);
2. a new pattern or trap is discovered;
3. the user asks to **update memory bank** (then review every file);
4. context needs clarification.

Rules:

- `docs/activeContext.md` stays under **250 lines**. When an update would
  exceed it, delete shipped history (git log keeps it) and keep standing
  rules, pending checks, Next, open questions and traps.
- A contract change edits the relevant `docs/systemPatterns.md` section in the
  same change; a decision that changes a contract says so in its Consequences.
- Visual changes update `docs/DESIGN.md` in the same change.
- User-facing behavior and settings go in `README.md` only as product
  documentation (what it does, how to configure it), never as status.
- Commit bank updates **with the change they record**. A standalone context
  commit is the exception: a decision recorded before its code, or a pure
  correction of a stale note.
- The bank is public: no secrets, personal paths beyond the documented dev
  setup, or private notes.

After every memory reset the bank is the only link to previous work. Keep it
accurate.
