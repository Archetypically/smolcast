# Security policy

## Supported versions

Only the latest release of smolcast gets security fixes. The app checks for a
new release once a day and installs it in place when you say so, so updating
is one click in the palette.

## Reporting a vulnerability

Email **smolcast@evanlee.engineer** with the details. Please do not open a
public issue or pull request for a vulnerability.

Include what you can of:

- the smolcast version ("About smolcast" in the palette shows it) and your Windows build,
- what the problem lets an attacker do, and what they need to already have,
- steps or a proof of concept that reproduce it.

You will get a reply, and a fix is released before the details are made
public. Say in your report whether you want to be credited.

## What counts

smolcast runs as your own user, reads your clipboard, launches programs and
runs scripts you chose, so the interesting problems are the ones that cross a
boundary you would not expect, for example:

- clipboard history, snippets or settings readable by another user or
  process,
- a crafted file, shortcut, URL, script argument or update payload that runs
  code or reads files without you asking,
- the self-updater installing something that is not a release of this
  repository.

The only request smolcast makes on its own is the daily update check (and
the download when you accept an update). Web searches and quicklinks open in
your browser.

Reports about behavior that needs you to already be running attacker code as
your own user are generally out of scope.

## Verifying a download

Each release lists a SHA-256 checksum beside the exe (`sha256sum -c` accepts
the `.sha256` file). Releases built after the repository went public also
carry a build provenance attestation:

```
gh attestation verify smolcast-<version>-x86_64.exe --repo Archetypically/smolcast
```

Builds are not Authenticode-signed yet, so Windows SmartScreen may warn on
first run.
