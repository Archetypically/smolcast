---
id: 019
slug: self-update-from-github-releases
author: archetypically
status: resolved
created: 2026-10-22
updated: 2026-10-22
tags: [updates, network, release]
---

# Update from GitHub Releases, in place, with a checksum until builds are signed

## Decision

smolcast checks `releases/latest` of its GitHub repository once a day (setting
`check_for_updates`, on by default) and on request ("Check for Updates").
A newer release with `smolcast-<version>-x86_64.exe` and its `.sha256` is
offered under the root list; installing downloads both over WinHTTP, checks
size and SHA-256, renames the running exe to `.old`, moves the download into
place, quits, and starts the new build, which waits for the old process to
exit. Releases are published by `.github/workflows/release.yml` on a `v*` tag.
Signing (Authenticode or an embedded Ed25519 key) comes later.

## Context

The user asked for self-updating through GitHub Releases, and to get the
plumbing working before builds are signed. The project brief kept the core
network-free; an update check is the first request smolcast makes on its own.

## Alternatives considered

- An updater framework or appcast (Sparkle-style; rejected: a dependency and
  a second feed when GitHub's release API already is one; Tinycast made the
  same call).
- A Rust HTTP/TLS stack such as `ureq` + `rustls` (rejected: hundreds of KB
  and non-system code for TLS; WinHTTP is in every Windows install and uses
  the system proxy and certificate store).
- Replacing the exe through a helper process or a scheduled task (rejected:
  Windows allows renaming a running exe, so a rename swap needs neither).
- Waiting for signing before shipping updates (rejected by the user: plumbing
  first).
- winget or an installer (rejected for now: smolcast is a single portable exe;
  winget can be added later without changing the in-app path).

## Rationale

One small system-DLL client, one JSON request a day, and a swap that leaves
the running build untouched until every check passes. The request carries no
identifier beyond the user agent `smolcast/<version>`. A file the app
downloads itself carries no Mark of the Web, so SmartScreen does not prompt
on update (only the first, browser-downloaded copy can be flagged).

## Consequences

- The checksum is published beside the exe in the same release, so it catches
  corruption and truncation, not a compromised release or account. Signing
  closes that gap: verify an Ed25519 signature against a key embedded in the
  binary, or an Authenticode signature, before the swap.
- smolcast's folder must be writable by the user (a per-user location such as
  `%LOCALAPPDATA%\Programs\smolcast`); otherwise the update fails with a
  notice pointing at the release page. No elevation is attempted.
- The releases API answers 404 while the repository is private or has no
  release; that reads as "up to date". Updates start working once the repo is
  public (activeContext Next) and a tagged release exists.
- Debug builds never check on their own.
- `projectbrief.md` lists the update check as the one exception to the
  network-free core. Plan §13 holds the contract.
