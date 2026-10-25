# Packages a release smolcast.exe as the Microsoft Store MSIX (plan §15):
# stages the exe, LICENSE, the logos and the manifest (Version from Cargo.toml
# or -Version, as Major.Minor.Patch.0), indexes the logos with makepri and
# packs with makeappx, both from the newest Windows SDK.
#
# The package is unsigned: the Store signs what it publishes. To try one on
# your own machine, turn on Developer Mode and register the staged layout:
#   Add-AppxPackage -Register dist\msix\layout\AppxManifest.xml
#
# -AllowPlaceholders packages even with the manifest's REPLACE-ME identity, so
# CI can check the packaging; such a package is never submitted.
#
# Usage: scripts/package_msix.ps1 -Exe <smolcast.exe> [-Version 1.2.3] [-OutDir dist/msix] [-AllowPlaceholders]
param(
    [Parameter(Mandatory)] [string]$Exe,
    [string]$Version,
    [string]$OutDir = 'dist/msix',
    [switch]$AllowPlaceholders
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$Exe = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Exe)
$OutDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutDir)

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program exited with code $LASTEXITCODE"
    }
}

if (-not $Version) {
    $line = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.+)"$' | Select-Object -First 1
    $Version = $line.Matches[0].Groups[1].Value
}
# MSIX versions are four numbers of at most 65535, and the Store wants the
# last one 0; a prerelease has no MSIX form and never goes to the Store.
if ($Version -cnotmatch '^(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})\.(0|[1-9]\d{0,4})$' -or
    @($Matches[1], $Matches[2], $Matches[3] | Where-Object { [int]$_ -gt 65535 }).Count) {
    throw "Version $Version is not major.minor.patch with each part at most 65535 (prereleases are not packaged)"
}

$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$sdk = Get-ChildItem $kits -Directory -Filter '10.*' -ErrorAction SilentlyContinue |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64' } |
    Where-Object { (Test-Path (Join-Path $_ 'makeappx.exe')) -and (Test-Path (Join-Path $_ 'makepri.exe')) } |
    Select-Object -First 1
if (-not $sdk) {
    throw "No Windows SDK with makeappx.exe and makepri.exe under $kits"
}

# Windows PowerShell 5.1 reads BOM-less files as ANSI without -Encoding.
$manifest = (Get-Content -Raw -Encoding UTF8 (Join-Path $root 'packaging/msix/AppxManifest.xml')).Replace('$(Version)', "$Version.0")
if ($manifest.Contains('REPLACE-ME') -and -not $AllowPlaceholders) {
    throw 'packaging/msix/AppxManifest.xml still has REPLACE-ME placeholders: copy Identity Name, Publisher and PublisherDisplayName from Partner Center > Product identity'
}

$layout = Join-Path $OutDir 'layout'
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force $layout | Out-Null
Copy-Item $Exe (Join-Path $layout 'smolcast.exe')
Copy-Item (Join-Path $root 'LICENSE') $layout
Copy-Item -Recurse (Join-Path $root 'packaging/msix/Assets') (Join-Path $layout 'Assets')
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)

# makepri maps Assets\Square44x44Logo.png and the others to their scale and
# target-size variants. Its default config also splits resources into
# per-scale packs, which only bundles use, so that part is dropped.
$priconfig = Join-Path $OutDir 'priconfig.xml'
Invoke-Checked (Join-Path $sdk 'makepri.exe') @('createconfig', '/cf', $priconfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
[xml]$config = Get-Content -Raw $priconfig
$packaging = $config.SelectSingleNode('/resources/packaging')
if ($packaging) { [void]$packaging.ParentNode.RemoveChild($packaging) }
$config.Save($priconfig)
Invoke-Checked (Join-Path $sdk 'makepri.exe') @(
    'new', '/pr', $layout, '/cf', $priconfig,
    '/mn', (Join-Path $layout 'AppxManifest.xml'),
    '/of', (Join-Path $layout 'resources.pri'), '/o'
)

# makeappx also validates the manifest against the schema.
$msix = Join-Path $OutDir "smolcast-$Version-x64.msix"
Invoke-Checked (Join-Path $sdk 'makeappx.exe') @('pack', '/d', $layout, '/p', $msix, '/o')
Write-Output "Packaged $msix"
