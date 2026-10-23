# Signed releases will only come from GitHub.

Once signing is on, each release is built from a tagged commit on GitHub and signed through SignPath Foundation. Until then, releases are unsigned.

Updated 23 Oct 2026. Signing is being set up; releases so far are unsigned.

- **1** File to sign: smolcast’s own executable
- **0** Builds signed from a personal computer
- **1** Approval by hand for every signed release

## How a release will be signed

Once signing is on, every release goes through these three steps. What gets signed: `smolcast-<version>-x86_64.exe` on [GitHub Releases](https://github.com/Archetypically/smolcast/releases). smolcast bundles no other programs, so nothing else carries the signature.

### Built on GitHub

Pushing a version tag such as `v0.2.0` runs [the release workflow](https://github.com/Archetypically/smolcast/blob/main/.github/workflows/release.yml) on a fresh GitHub-hosted Windows machine. It builds from that commit and nothing else.

### Signed through SignPath

The workflow hands the executable to SignPath, which checks it came from this repository’s workflow before signing it with SignPath Foundation’s certificate. The signing key never leaves SignPath.

### Approved by hand

An approver (below) confirms each signing request. Only then is the release published, with its SHA-256 checksum beside it.

## Who does what

- **Committers and reviewers**: [Evan Lee](https://github.com/Archetypically). Changes from anyone else are reviewed before they merge.
- **Approvers**: [Evan Lee](https://github.com/Archetypically), for every signing request.

## Check a download

Every release has a checksum. The signature starts with the first signed release, and the build record once the repository is public; earlier releases have neither.

### The signature

Right-click the file, then **Properties** › **Digital Signatures**. A signed release names **SignPath Foundation**.

### The checksum

Each release lists a `.sha256` file. In PowerShell, `Get-FileHash smolcast-*.exe` should print the same value, in capitals.

### The build

`gh attestation verify` on the file, with `--repo Archetypically/smolcast`, confirms GitHub built it from this repository.

## Privacy

The signed program sends nothing about you. The only request it makes on its own is a daily update check, with an off switch. [Read the privacy policy](https://smolcast.evanlee.engineer/privacy) for everything it keeps and sends.

## Credits, changes and contact

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

Last updated 23 October 2026. Every earlier version is in [the project’s history](https://github.com/Archetypically/smolcast/commits/main/website/public/code-signing.html).

Found a file signed as smolcast that didn’t come from its releases? Open an issue on [GitHub](https://github.com/Archetypically/smolcast/issues) or reach Evan Lee through [evanlee.engineer](https://evanlee.engineer).

---

The markdown version of https://smolcast.evanlee.engineer/code-signing
