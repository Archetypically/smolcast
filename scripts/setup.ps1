$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$TOOLCHAIN = '1.98.1'

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program exited with code $LASTEXITCODE"
    }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
function Get-CppTools {
    if (Test-Path $vswhere) {
        & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
}

if (-not (Get-CppTools)) {
    if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
        throw 'Install App Installer (winget) from Microsoft, then rerun task up.'
    }
    # Interactive installer prompts retain license/elevation control with the user.
    & winget.exe install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --override '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    if ($LASTEXITCODE -eq 3010) {
        throw 'C++ tools installed; reboot Windows, then rerun task up.'
    }
    if ($LASTEXITCODE -ne 0) { throw "Visual Studio installer exited with code $LASTEXITCODE" }
    if (-not (Get-CppTools)) { throw 'C++ build tools not found after installation.' }
}

$sdk = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
$sdkLib = if ($sdk -and $sdk.KitsRoot10) { Join-Path $sdk.KitsRoot10 'Lib' }
if (-not $sdkLib -or -not (Get-ChildItem "$sdkLib\*\um\x64\kernel32.lib" -ErrorAction SilentlyContinue)) {
    throw 'Windows SDK missing. In Visual Studio Installer, Modify > Desktop development with C++ > Windows 11 SDK, then rerun task up.'
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$env:PATH = "$(Join-Path $cargoHome 'bin');$env:PATH"
$work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    if (-not (Get-Command rustup.exe -ErrorAction SilentlyContinue)) {
        $installer = Join-Path $work 'rustup-init.exe'
        Invoke-WebRequest -UseBasicParsing 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $installer
        Invoke-Checked $installer @('-y', '--profile', 'minimal', '--default-toolchain', "$TOOLCHAIN-x86_64-pc-windows-msvc")
    }
    # The repository pins the toolchain in rust-toolchain.toml; install that exact
    # named toolchain rather than tracking stable.
    Invoke-Checked rustup.exe @('toolchain', 'install', $TOOLCHAIN, '--profile', 'minimal', '--component', 'rustfmt', '--component', 'clippy')
    Invoke-Checked rustup.exe @('target', 'add', '--toolchain', $TOOLCHAIN, 'x86_64-pc-windows-msvc')
    $probe = Join-Path $work 'probe.rs'
    $exe = Join-Path $work 'probe.exe'
    @'
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
}
fn main() {
    println!("Windows process: {}", unsafe { GetCurrentProcessId() });
}
'@ | Set-Content -Encoding ASCII $probe
    Invoke-Checked rustc.exe @("+$TOOLCHAIN", '--edition', '2024', '--target', 'x86_64-pc-windows-msvc', $probe, '-o', $exe)
    Invoke-Checked $exe @()
    Write-Host 'Windows MSVC toolchain smoke: linked and ran a Windows API executable successfully.'
    if (Test-Path Cargo.toml) {
        Invoke-Checked cargo.exe @("+$TOOLCHAIN", 'fetch', '--locked')
    } else {
        Write-Host 'Dependencies ready. Application implementation is pending plan approval.'
    }
} finally {
    Remove-Item -Recurse -Force $work
}
