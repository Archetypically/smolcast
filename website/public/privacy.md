# smolcast collects nothing.

Your stuff stays on your PC. The app makes one request a day, an update check you can switch off. This site uses Google Analytics to count visits.

Updated 23 Oct 2026.

- **0** Accounts, trackers, telemetry or crash reports sent
- **1** Request a day: the update check, with an off switch
- **0** Analytics on the app (the website uses Google Analytics)

## Five files and a folder, all yours

Everything lives in your own Windows profile. Delete the two `smolcast` folders and it’s gone.

- `%APPDATA%\smolcast\config.toml`: Your settings, scripts, snippets and quicklinks
- `%APPDATA%\smolcast\favorites.json`: The results you pinned
- `%APPDATA%\smolcast\hidden.json`: The results you hid
- `%LOCALAPPDATA%\smolcast\usage.json`: What you open and when, so results rank well
- `%LOCALAPPDATA%\smolcast\clipboard\`: smolcast’s own clipboard history, encrypted (below)
- `%LOCALAPPDATA%\smolcast\update.json`: When it last checked, and any version you skipped
- `%LOCALAPPDATA%\smolcast\crash.log`: Only after a crash: the version, Windows build and error. Kept small, never sent

That crash log is the only log. When something fails, the notice names the operation and an error code, never what you copied or typed.

## What leaves, and when

### The update check

Automatic, once a day

Asks `api.github.com` for smolcast’s latest release. It sends smolcast’s version and nothing about you. GitHub sees your IP, as with any visit ([GitHub’s privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)). Updates install only when you say so.

Off switch: **Check GitHub for a new version once a day** in settings, or `check_for_updates = false`.

### Only when you ask

You’re in control

- **Web searches and quicklinks** open in your browser when you pick them. That site gets your query.
- **File search** asks the Windows Search index on your PC. No index of its own.
- **Scripts** run the programs you set up, as you.
- **Report a Bug** opens a new GitHub issue in your browser with smolcast’s version and your Windows build. The crash log goes in only if you tick it, and nothing is posted until you submit.

### Clipboard history

Encrypted, on your PC

- Windows clipboard history (`Win`+`V`) on? smolcast just shows it and keeps no copy.
- Otherwise it keeps its own: 100 items by default, encrypted so only your Windows account can read them. `clipboard_persist = false` keeps it in memory; `clipboard_capture = false` keeps nothing; `clipboard_max_age_days` forgets items after that many days.
- Skips anything an app marks as private, the flag password managers can set on what they copy. It can’t spot an unmarked password.

## Google Analytics on this website only

### Google Analytics

This website loads Google Analytics (Google tag, gtag.js), which counts visits and may set cookies and send your IP address and page views to Google. Its advertising features (Google signals and ad personalization) are off. The app never loads it. Everything else on a page comes from this site. Light or dark is remembered in your browser's local storage and never leaves it.

### Hosting and downloads

Cloudflare Pages serves the site, so Cloudflare handles each request, including your IP and user agent ([Cloudflare’s privacy policy](https://www.cloudflare.com/privacypolicy/)). Google Analytics is on; see above. Downloads come from GitHub.

## Changes and contact

Last updated 23 October 2026. Every earlier version is in [the project’s history](https://github.com/Archetypically/smolcast/commits/main/website/public/privacy.html), and since smolcast is open source, you can check all of this against [its code](https://github.com/Archetypically/smolcast).

Questions? Open an issue on [GitHub](https://github.com/Archetypically/smolcast/issues) or reach Evan Lee through [evanlee.engineer](https://evanlee.engineer).

---

The markdown version of https://smolcast.evanlee.engineer/privacy
