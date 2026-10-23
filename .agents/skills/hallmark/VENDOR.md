# Vendored: Hallmark

- Source: https://github.com/nutlope/hallmark, `skills/hallmark/`
- Commit: 7ec868225c6caa040558c321ee0fefa0e73f83a8 (2026-10-22)
- License: MIT, `LICENSE` beside this file
- Omitted: `references/themes/*.jpg` (5 MB of theme previews). They are only
  looked at when a theme is dealt, which smolcast never does: its system is
  locked in `docs/DESIGN.md`. `theme.mjs` handles their absence.
- Nothing else is changed. This file is the only addition.

## How smolcast uses it

- The website is held to Hallmark's floor at **polish** depth with the
  system locked by `docs/DESIGN.md` (see its Taste section). The stamp,
  waivers and dismissals are the first lines of `website/public/styles.css`.
- Audit: `node .agents/skills/hallmark/scripts/sloplint.mjs <pages>
  --genre modern-minimal`, run from a scratch directory (it writes
  `.hallmark/` into the working directory, which git ignores).
- Its static pass resolves `href="/styles.css"` against the filesystem root,
  so lint a copy of `website/public/` whose pages link `styles.css` and
  `tokens.css` relatively; otherwise every page falsely fails 26a
  (`:focus-visible`) and 34 (`overflow-x`).
- The render tier needs `puppeteer-core` and `CHROME_PATH`; the static tier
  is enough for a polish.

## Updating

Clone the repository, copy `skills/hallmark/` over this folder (keeping this
file), delete `references/themes/*.jpg`, and update the commit above.
