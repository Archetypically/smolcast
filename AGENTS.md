## Project context

smolcast keeps its shared context in `docs/` using the memory-bank pattern.
Use the `.agents/skills/memory-bank/SKILL.md` skill: read
`docs/activeContext.md` at the start of every session (by section), and update
it after significant changes, committed together with the change it records.

- `docs/projectbrief.md`: scope and non-goals (source of truth for what smolcast is).
- `docs/productContext.md`: why it exists and how each feature behaves for the user.
- `docs/systemPatterns.md`: normative architecture contracts; code cites its sections as "plan §N".
- `docs/techContext.md`: toolchain, setup, commands, constraints.
- `docs/decisions/`: one file per durable decision, per `.agents/skills/making-decisions/SKILL.md`.
- `docs/DESIGN.md`: visual design; consult it before adding or styling UI.

`README.md` is the public product page: features, install, usage, configuration.
Never put status, contracts or internal notes there. This repository is public:
never commit machine-specific paths, usernames or private notes; local paths go
in the gitignored `.env` (see `.env.example`).

## Before opening a pull request

Run this review workflow on the branch, in order, then open the PR and watch
CI until it is green:

1. A Sonnet subagent at high effort runs `/code-review` on the branch; fix
   what it confirms.
2. Haiku subagents at xhigh effort run `/simplify`; keep the changes that
   pass fmt, clippy and the tests.
3. A Haiku subagent rewrites the branch history into logical commits (each
   building on its own, docs committed with the change they record).
