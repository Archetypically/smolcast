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

Run this workflow on the branch, in order:

1. A Sonnet subagent at high effort runs `/code-review` on the branch; fix
   what it confirms.
2. Haiku subagents at xhigh effort run `/simplify`; keep the changes that
   pass fmt, clippy and the tests.
3. The `pr-shepherd` subagent (`.claude/agents/pr-shepherd.md`; Haiku at
   medium effort) rewrites the branch history into logical commits (each
   building on its own, docs committed with the change they record),
   force-pushes the branch, opens the PR, watches CI until it is green, and
   merges if the user authorized it. It reports each operation's result back
   to the main agent.
4. That subagent never waits more than 4 minutes 30 seconds at a time (it
   keeps the main agent's prompt cache warm). The main agent passes it the
   commit trailer lines naming the main agent's model, and it uses them as
   given, never its own model.
