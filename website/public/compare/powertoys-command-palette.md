# smolcast vs PowerToys Command Palette

Command Palette, the successor to PowerToys Run, reaches deep into Windows: WinGet, services, the registry. smolcast adds clipboard history, emoji and snippets instead.

Updated 23 Oct 2026. Checked against [Microsoft’s docs](https://learn.microsoft.com/windows/powertoys/command-palette/overview) (PowerToys 0.102). [Sources](https://smolcast.evanlee.engineer/compare/powertoys-command-palette#sources)

## smolcast

Open source

- Free
- Open source
- 7 MB
- Standalone

**Best for** clipboard, emoji and snippets in one tiny app

## PowerToys Command Palette

Microsoft

- Free
- Open source
- Part of PowerToys
- Extensions

**Best for** Windows admin and extensions

[Get PowerToys Command Palette](https://learn.microsoft.com/windows/powertoys/install?ref=smolcast)

## Feature by feature

Bold marks where smolcast comes out ahead, and only where the other side publishes the fact.

|  | smolcast | PowerToys Command Palette |
| --- | --- | --- |
| **The basics** |  |  |
| Price | Free | Free |
| Source | Open source (MIT) | Open source (MIT) |
| Runs on | Windows 11 | Windows 10 and 11 |
| Account | None | None |
| Default hotkey | `Ctrl`+`Alt`+`Space` | `Win`+`Alt`+`Space` |
| **Features** |  |  |
| Clipboard history | Built in (Text and images. Encrypted on disk) | Built in (Recent entries) |
| Snippets | **Built in** (Find by name or keyword. No auto-expand) (the better pick) | No (Bookmarks with {query} instead) |
| Emoji picker | **Built in** (Every emoji, in color) (the better pick) | No (Windows has `Win`+`.`) |
| File search | Built in (Through Windows Search) | Built in |
| Calculator | Built in (Units too. Enter pastes) | Built in (Math and functions) |
| Quicklinks | Built in (Keywords with {query}) | Bookmarks |
| Extensions | No (Scripts and quicklinks instead) | Extension Gallery |
| AI | No | No |
| Window management | No | Window switching |
| **Under the hood** |  |  |
| Download | **7 MB, one .exe** (No installer) (the better pick) | 309 MB installer (The whole PowerToys suite) |
| Idle | 0% CPU while hidden | Not published |
| Privacy | **No telemetry** ([One daily update check](https://smolcast.evanlee.engineer/privacy), can be turned off) (the better pick) | Diagnostic data (Can be turned off) |

## Pick the one that fits

### Go with smolcast if

- You want clipboard, emoji and snippets without add-ons
- You want unit conversions like 40cm to in
- You want one small app, not a suite
- You want no telemetry

### Go with Command Palette if

- You already run PowerToys
- You want WinGet, services, the registry and Settings in the launcher
- You want extensions or the Dock
- You’re on Windows 10

[Get PowerToys Command Palette](https://learn.microsoft.com/windows/powertoys/install?ref=smolcast)

For plugins without the whole suite there is [smolcast vs Flow Launcher](https://smolcast.evanlee.engineer/compare/flow-launcher), and for one search over everything, even text on screen, [smolcast vs Fluent Search](https://smolcast.evanlee.engineer/compare/fluent-search).

## Coming from Command Palette

- PowerToys RunCommand Palette replaced it, so this page covers both; smolcast’s `Ctrl`+`Alt`+`Space` won’t clash with Run’s `Alt`+`Space`
- BookmarksQuicklinks: `[quicklinks.<keyword>]` with a URL, file or folder and `{query}`
- Hotkey`Ctrl`+`Alt`+`Space` by default, so you can run both while you try it
- Shell commandsTracked script files, each with an optional hotkey

## Try smolcast

- Windows 11, x64
- 7 MB
- Free and open source

[Download](https://github.com/Archetypically/smolcast/releases/latest)

## More alternatives

- [Every alternative, side by sideAlternatives](https://smolcast.evanlee.engineer/compare/)
- [smolcast vs Raycast for WindowsAlternative](https://smolcast.evanlee.engineer/compare/raycast)
- [smolcast vs Flow LauncherAlternative](https://smolcast.evanlee.engineer/compare/flow-launcher)
- [smolcast vs TinycastAlternative](https://smolcast.evanlee.engineer/compare/tinycast)
- [smolcast vs Fluent SearchAlternative](https://smolcast.evanlee.engineer/compare/fluent-search)
- [smolcast vs ListaryAlternative](https://smolcast.evanlee.engineer/compare/listary)
- [smolcast vs WoxAlternative](https://smolcast.evanlee.engineer/compare/wox)
- [smolcast vs AlfredAlternative](https://smolcast.evanlee.engineer/compare/alfred)
- [smolcast vs KeypirinhaAlternative](https://smolcast.evanlee.engineer/compare/keypirinha)
- [smolcast vs UeliAlternative](https://smolcast.evanlee.engineer/compare/ueli)
- [What smolcast keeps on your PCPrivacy](https://smolcast.evanlee.engineer/privacy)

## Sources

1.  smolcast, main branch: [github.com/Archetypically/smolcast](https://github.com/Archetypically/smolcast)
2.  PowerToys Command Palette overview, updated October 2026: [learn.microsoft.com](https://learn.microsoft.com/windows/powertoys/command-palette/overview)
3.  PowerToys 0.102.2933.0 release (installer size): [github.com/microsoft/PowerToys](https://github.com/microsoft/PowerToys/releases/tag/v0.102.2933.0)

Checked against these versions on 23 Oct 2026, which is also when this page was last updated. Found something stale? [Open an issue](https://github.com/Archetypically/smolcast/issues).

---

The markdown version of https://smolcast.evanlee.engineer/compare/powertoys-command-palette
