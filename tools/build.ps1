# build.ps1 - Builds the 32-bit mod DLL.
#
# Run this from PowerShell, not from Git Bash: bash puts GNU coreutils `link`
# ahead of the toolchain linker on PATH and the build fails with a confusing
# "extra operand" error.
#
# Target is i686-pc-windows-gnu. The MSVC target would need the Windows SDK,
# which is not installed here; the gnu target ships its own linker and import
# libraries, and produces an equally valid 32-bit DLL.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1 -Configuration Debug
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1 -Install

param(
    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release',

    [switch]$Install,

    [string]$Target = 'i686-pc-windows-gnu',

    # Pinned because the msvc host toolchain has no dlltool, which the gnu
    # target needs to resolve raw-dylib imports.
    [string]$Toolchain = '+stable-i686-pc-windows-gnu',

    [string]$GameDir = 'D:\SteamLibrary\steamapps\common\Resident Evil 0'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Install Rust from https://rustup.rs"
}

Set-Location $repo

$cargoArgs = @($Toolchain, 'build', '--target', $Target)
$outputDir = 'debug'
if ($Configuration -eq 'Release') {
    $cargoArgs += '--release'
    $outputDir = 'release'
}

Write-Host "Building $Configuration for $Target..." -ForegroundColor Cyan
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }

$dll = Join-Path $repo "target\$Target\$outputDir\re0inv.dll"
if (-not (Test-Path $dll)) { throw "DLL not found at $dll" }

Write-Host ("Built {0} ({1:N0} bytes)" -f $dll, (Get-Item $dll).Length) -ForegroundColor Green

if (-not $Install) { return }

# --- Install into the game ---

if (-not (Test-Path $GameDir)) { throw "Game directory not found: $GameDir" }

$scripts = Join-Path $GameDir 'scripts'
New-Item -ItemType Directory -Force -Path $scripts | Out-Null

# re0box hooks the same inventory subsystem; the two cannot both be loaded.
$re0box = Join-Path $scripts 're0box.asi'
if (Test-Path $re0box) {
    Write-Host 'WARNING: re0box.asi is installed and conflicts with this mod.' -ForegroundColor Yellow
    Write-Host '         Rename or remove it before testing.' -ForegroundColor Yellow
}

Copy-Item $dll (Join-Path $scripts 're0inv.asi') -Force
Write-Host "Installed: $(Join-Path $scripts 're0inv.asi')" -ForegroundColor Green

# Only seed the ini if absent, so local tweaks survive a reinstall.
$iniSource = Join-Path $repo 're0inv.ini'
$iniTarget = Join-Path $GameDir 're0inv.ini'
if ((Test-Path $iniSource) -and -not (Test-Path $iniTarget)) {
    Copy-Item $iniSource $iniTarget
    Write-Host "Installed: $iniTarget" -ForegroundColor Green
}
