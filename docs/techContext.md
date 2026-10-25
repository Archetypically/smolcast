# Tech context

## Stack and layout

One Cargo package with two targets:

- **Library** (`src/lib.rs`, `src/core/`): Windows-free logic (config,
  ranking, history, DIB codec, emoji table, calculator, favorites, snippets,
  placement, web search URLs). It compiles and tests on the Linux host.
  `lib.rs` and `core/` never import `windows`, eframe, egui, tray or hotkey
  crates (`systemPatterns.md` §4).
- **Binary** (`src/main.rs`, `src/app/`, `src/win/`): the Windows-only app.
  `app/` is the egui UI (model, views, theme); `win/` holds every `unsafe`
  Windows call behind owning wrappers (§0).

Dependencies are pinned exactly in `Cargo.toml` and locked; the toolchain is
pinned in `rust-toolchain.toml` (1.98.1, rustfmt + clippy). Host crates:
fuzzy-matcher, image (BMP only), serde, serde_json, toml, toml_edit,
unicode-segmentation. Windows-only: eframe/egui 0.36.2 (Glow), global-hotkey,
tray-icon, raw-window-handle, windows 0.62.2. Adding a crate needs a reason
the standard library or the selected Windows APIs cannot cover (§0, §3).

## Platform constraints

- Windows 11 x64 is the only runtime target. Linux cross-links the GNU binary
  and runs the library tests; that proves nothing about focus, clipboard,
  hotkeys, COM, tray, graphics or input injection. Wine is not a verification
  target.
- Windows can refuse [foreground activation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow),
  and [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
  cannot inject into higher-integrity apps; paste-back degrades to
  "copied, not pasted" (§8).
- Packaged apps launch through
  [ActivateApplication](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iapplicationactivationmanager-activateapplication)
  by AUMID, never an invented exe path (§6).
- Native MSVC builds need
  [MSVC tools and a Windows SDK](https://rust-lang.github.io/rustup/installation/windows-msvc.html).

## Setup

Install [Task v3](https://taskfile.dev/docs/installation) first, then run
`task up`:

- **Windows 11 x64** (PowerShell): `scripts/setup.ps1` installs Visual Studio
  2022 C++ Build Tools via winget when absent, checks the Windows SDK, installs
  the pinned toolchain with the MSVC target, and links a Windows API probe.
  Installers may prompt for elevation and may need a reboot.
- **Debian/Ubuntu x64 or WSL**: `scripts/setup.sh` installs build-essential,
  MinGW x64 and the pinned toolchain with `x86_64-pc-windows-gnu` (needs sudo
  for missing packages).

Machine-specific paths are never tracked. Copy `.env.example` to `.env`
(gitignored) and set `STAGE_DIR` / `INSTALL_DIR` for WSL staging.

## Commands

| Command | Does |
| --- | --- |
| `task` | List commands |
| `task up` | Install dev dependencies and smoke-check the toolchain |
| `task build` / `task build-release` | Debug / release Windows build (`--locked`) |
| `task run` | Windows: build and run. WSL: stage a debug build and launch it through interop |
| `task stage` | WSL: build (`PROFILE=release` default) and copy the exe to `STAGE_DIR` |
| `task install` | WSL: release build copied to `INSTALL_DIR` (the everyday copy) |
| `task msix` | Windows: release build packaged as the Store MSIX in `dist/msix` (`VERSION=` for a prerelease Cargo version) |
| `task check` / `task clippy` | Windows-target type-check / lint (`-D warnings`, all targets) |
| `task fmt` / `task fmt-check` | Format / check formatting |
| `task test` | Library tests on the host |
| `task clean` | Remove Cargo artifacts |

Default target: `x86_64-pc-windows-msvc` on Windows, `x86_64-pc-windows-gnu`
elsewhere. A running exe cannot be overwritten, so `stage` and `install`
rename the old one aside first.

## CI

`.github/workflows/ci.yml` runs fmt, clippy and the MSVC release build on
Windows, and the host tests plus the GNU release cross-link on Linux, with
dependency builds cached between runs (§11). The Windows job also runs
the desktop smoke test (`tests/smoke.rs`, decision 029): the release exe on
the runner's desktop, driven with real keystrokes and clicks, with Mesa's
software OpenGL beside the exe (the runner has no GPU); a failure uploads a
desktop screenshot. It only runs with `--ignored` and
`SMOLCAST_SMOKE_DESKTOP=disposable`, never on a maintainer's desktop (if it
grows slow, make it a release `example`). New
tests default to this level, not unit tests. CI passing is not runtime
acceptance: each milestone's Windows smoke gate (§12) is.

Action `uses:` references are full commit SHAs with a `# vX.Y.Z` comment, not
floating tags. `.github/dependabot.yml` checks Cargo and GitHub Actions weekly
and opens one pull request for every version update. Security updates stay on
Dependabot's own schedule. The pinned toolchain (`rust-toolchain.toml`) is not
a Dependabot ecosystem; bump it by hand.

To release, bump `version` in `Cargo.toml`, merge, then push a matching tag
(`git tag v0.2.0 && git push origin v0.2.0`). `.github/workflows/release.yml`
builds the MSVC exe and publishes `smolcast-<version>-x86_64.exe` with its
`.sha256`, which running copies offer as an update (§13). The version, author
and site in the exe's Properties come from `Cargo.toml` through `build.rs`
(decision 026); the workflow fails if the embedded version is not the tag's.
The app icon is generated from `assets/smolcast.svg`:
`python3 scripts/generate_icon.py` (needs cairosvg and Pillow) rewrites
`assets/smolcast.ico`, `assets/smolcast.png` and the MSIX logos in
`packaging/msix/Assets`. A tag with a `-` (`v0.2.0-rc.1`) is a prerelease
and is never offered.

### Microsoft Store (plan §15, decision 040)

A release (not prerelease) tag also submits the exe to the Store as an MSIX,
once the Store is set up. `task msix` (Windows, needs the Windows SDK; pass
`VERSION=1.0.0` while Cargo.toml has a prerelease) builds the same package
into `dist/msix`. One-time setup, in order:

1. Partner Center > the reserved app > Product management > Product
   identity: copy `Package/Identity/Name`, `Package/Identity/Publisher` and
   `Package/Properties/PublisherDisplayName` over the `REPLACE-ME` values in
   `packaging/msix/AppxManifest.xml`. Note the Store ID (12 characters).
2. First submission by hand: `task msix VERSION=<version>`, then a
   submission with the listing, screenshots, age rating, the privacy URL,
   and under Submission options the restricted-capability justification:
   `runFullTrust`, a desktop app; `unvirtualizedResources`, a launcher whose
   launched apps and scripts must write to the user's real AppData and
   registry. Free. Wait until it is published.
3. Partner Center > Account settings > User management > Microsoft Entra
   applications: add (or create) an app with the Manager role and a client
   secret. Note the tenant ID, client ID, secret and the Seller ID (shown
   under Account settings).
4. GitHub > Settings > Environments: create `microsoft-store` (add yourself
   as a required reviewer to approve each submission; under Deployment
   branches and tags allow only tags matching `v*`, so no workflow on a
   branch can read its secrets) with secrets
   `MSSTORE_TENANT_ID`, `MSSTORE_SELLER_ID`, `MSSTORE_CLIENT_ID` and
   `MSSTORE_CLIENT_SECRET`. Then add the repository variable
   `MSSTORE_PRODUCT_ID` (the Store ID), which switches the Store steps on.

The secret expires (24 months at most); renew it in Entra and update the
environment secret. Certification takes up to a few days; follow it in
Partner Center.

## Website

`website/` is the public site (decisions 030, 033), self-contained so it can
move hosts. `website/public/` is deployed to Cloudflare Pages as-is: no build
step, no framework, no third-party requests. Pages: home (`index.html`),
`privacy.html`, and `compare/` (a hub and one page per launcher), served at
pretty URLs (`/privacy`, `/compare/raycast`). `_headers` sets the CSP and
caching; `404.html` is Pages' not-found page. `media/` holds render-only
pages (the social card, the demo film), served at `/_media/` by
`scripts/serve.mjs` and never deployed.

Shared parts have one source (decision 033): `partials/` (head links, top
bar, footer) are stamped between `<!-- partial:NAME -->` markers in every
page, and `public/tokens.css` is generated from `src/app/theme.rs`, both by
`task web:sync`. A new page: copy a page's skeleton with its markers, add it
to `sitemap.xml` and `llms.txt`, and run the sync. The comparison pages are
generated: their facts, sources and copy live in `scripts/compare.py` (one
dict per tool, ten tools); edit there and run `task web:compare`, never the
HTML. Each page carries the date its facts were checked; recheck them when
the other products change. Facts a project does not publish read "Not
listed" or "Not published", never "No".

Design audit: the site follows Hallmark's floor (DESIGN.md, Taste), vendored
in `.agents/skills/hallmark/` (linked from `.claude/skills/`). The first
lines of `styles.css` are its stamp (`redesign: polish`, the locked system)
and any waivers. Its `VENDOR.md` says how to run `sloplint.mjs` and the
relative-stylesheet workaround the static pass needs.

Discovery: each page has a canonical URL, a description, Open Graph tags and
JSON-LD (`SoftwareApplication` on home; `WebPage` or `CollectionPage` plus
`BreadcrumbList` elsewhere), and `llms.txt` summarizes every page. When the
Pages project is created, check that Cloudflare's AI-crawler blocking and
managed `robots.txt` are off for the domain, or AI search will not see the
site.

| Command | Does |
| --- | --- |
| `task web:dev` | Serve the site at http://127.0.0.1:8788 |
| `task web:fmt` / `task web:fmt-check` | Prettier over HTML, CSS, JS and config |
| `task web:lint` | ESLint, html-validate, and `sync --check` |
| `task web:compare` | Regenerate the comparison pages from the facts in `scripts/compare.py` (Python 3), then stamp and format them |
| `task web:sync` | Stamp `partials/` into every page and regenerate `tokens.css` from `theme.rs` |
| `task web:render` | Regenerate the PNG icons, `og.png` and every page's card in `public/og/` with Playwright (`-- icons`, `og` or `pages` to pick) |
| `task web:film` | Record the demo film from `media/film.html` in all three cuts (needs ffmpeg; about 12 minutes; `-- wide`, `square` or `vertical` to pick) |
| `task web:deploy` | `wrangler pages deploy public` (run `npx wrangler login` once) |

Tools are pinned in `website/package.json` and its lockfile; Playwright's
version must match an installed Chromium (`npx playwright install chromium`).
CI runs the format check, ESLint, html-validate and the sync check on Ubuntu.
After changing a color in `src/app/theme.rs`, run `task web:sync` (CI fails
until you do).

The demo film (`public/media/smolcast-demo.{mp4,webm,jpg}`, also the page's
`og:video`) is a faithful simulation (decision 031): `media/film.html`,
`film.css` and `film.js` draw a Windows 11 desktop at 150% with the app's
measurements, labels and results, using open Microsoft stand-ins for
Windows' fonts, icons and emoji (`media/film/`, licenses inside). The emoji
results in `media/film/emoji-search.json` come from `core::emoji::search`.
`scripts/film.mjs` records it with Chrome's screencast, which manages about 30
fps and ignores the device scale factor: the page is CSS-zoomed to 1080p and
runs 4× slower (timers via `media/slowmo.js`, animations via CDP
`Animation.setPlaybackRate`), and the frames are re-timed and encoded at 60
fps with the original music from `scripts/music.mjs` (synthesized,
deterministic, no third-party audio). `?format=` lays the same film out wide,
square or vertical; it writes the page's files and the square and vertical
social cuts in the ignored `media/out/`. Every cut loops: it opens and closes
on the same title-card frame, the page holds that card until the film is whole
12 s phrases (`PHRASE`), the backdrop drifts one tile repeat per phrase, and
the music's tail is folded onto its start, at a fixed gain (no loudnorm, which
would ride the level at the seam). After adding a chapter, update the JSON-LD
`duration` in `index.html`. Re-record after visible changes to the app.

Touch gestures cannot be synthesized in headless Chromium here, so the
palette's native touch scrolling is checked on a real phone.

## Agent workflow notes

- Develop in WSL; nothing is installed on the Windows system drive. Try
  builds through `task stage`.
- The maintainer works on the test desktop: no scripted input or screenshots
  without asking first; prefer a manual checklist.
- Never trigger GPG/passphrase prompts; hand signing commands to the user.
