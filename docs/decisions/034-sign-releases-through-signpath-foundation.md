---
id: 034
slug: sign-releases-through-signpath-foundation
author: archetypically
status: open
created: 2026-10-23
updated: 2026-10-23
tags: [release, signing, updates]
---

# Sign releases through SignPath Foundation

## Decision

Release executables get an Authenticode signature from SignPath Foundation's
free program for open-source projects. Only `release.yml` on a `v<semver>`
tag produces a signing request, built on a GitHub-hosted runner; SignPath
checks the request came from that workflow, and an approver confirms each
one by hand. Nothing built elsewhere is signed. The public policy SignPath
requires is `website/public/code-signing.html`: roles (committers and
reviewers, approvers: Evan Lee), the attribution line and a privacy link.
Before the update swap, the updater checks the downloaded exe with
`WinVerifyTrust` and requires the SignPath Foundation subject plus
smolcast's own `ProductName` and `OriginalFilename` from the signed version
resource (decision 026).

## Context

Decision 019 shipped self-update with a checksum only and left signing for
later. The user asked how to sign builds in GitHub Actions and chose
SignPath Foundation (2026-10-23). SmartScreen and a Microsoft Store exe
submission both want an Authenticode signature from a trusted CA, and the
checksum, published in the same release, cannot stop a compromised release.

## Alternatives considered

- Azure Artifact Signing (about $10 a month; the certificate names the
  developer; individuals only in the US and Canada).
- A certificate from a commercial CA (yearly cost and a hardware token or
  cloud HSM; no advantage over the above since EV lost its SmartScreen
  bypass).
- An embedded ECDSA or Ed25519 key that signs the checksum (no SmartScreen
  or Store benefit; kept as a fallback for the updater if signing stalls).

## Rationale

Free, and the key lives in SignPath's HSM, never on a laptop or in a GitHub
secret. SignPath's origin check and manual approval mean a stolen push token
alone cannot produce a signed build. The certificate is shared by every
SignPath Foundation project, so the subject alone is a weak pin; requiring
smolcast's own version resource inside the signed file narrows it to
binaries built as smolcast.

## Consequences

- Needs the repository public and the website deployed (the policy must be
  on the home page and linked from release notes), then an application at
  signpath.org. Until approval, releases stay unsigned and the policy page
  says so.
- `release.yml` gains SignPath's action before the checksum step, so the
  `.sha256` and the build attestation cover the signed bytes.
- Plan §13 holds the updater contract. An update from an unsigned release to
  a signed one is accepted; once a signed release has shipped, a later
  build may refuse unsigned updates.
- The signer shown in Windows is "SignPath Foundation", not the author.
