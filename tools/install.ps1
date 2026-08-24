# install.ps1 - Installs the mod into the game, after snapshotting what it changes.
#
# Every install writes a manifest to backups\install\. uninstall.ps1 reads that
# manifest to put the game folder back exactly as it was, including re-enabling
# any other ASI mod this script disabled.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1
#   powershell -ExecutionPolicy Bypass -File tools\install.ps1 -Probe -DumpText

param(
    [string]$GameDir = 'D:\SteamLibrary\steamapps\common\Resident Evil 0',

    # Other ASI plugins hook the same subsystem and cannot run alongside this
    # one. They are renamed, never deleted, and uninstall.ps1 restores them.
    [switch]$DisableOtherAsi,

    [switch]$Probe,
    [switch]$DumpText,
    [ValidateSet('off', 'error', 'warn', 'info', 'debug', 'trace')]
    [string]$LogLevel = 'info',
    [int]$Slots = 8
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if (-not (Test-Path $GameDir)) { throw "Game directory not found: $GameDir" }

$dll = Join-Path $repo 'target\i686-pc-windows-gnu\release\re0inv.dll'
if (-not (Test-Path $dll)) { throw "Build the mod first: tools\build.ps1" }

$scripts = Join-Path $GameDir 'scripts'
New-Item -ItemType Directory -Force -Path $scripts | Out-Null

$backupRoot = Join-Path $repo 'backups\install'
New-Item -ItemType Directory -Force -Path $backupRoot | Out-Null

# The manifest records every path this script creates or renames, so the
# uninstall never has to guess.
$manifest = [ordered]@{
    GameDir = $GameDir
    Created = @()
    Renamed = @()
}

# --- Save backup, every time ---

$saveDir = "$env:ProgramFiles(x86)\Steam\userdata"
$saves = Get-ChildItem $saveDir -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName '339340\remote\data0.bin' } |
    Where-Object { Test-Path $_ }

foreach ($save in $saves) {
    $stamp = Get-Date -Format 'yyyy-MM-dd_HHmmss'
    $dest = Join-Path $repo "backups\saves\$stamp"
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    Copy-Item $save $dest
    Write-Host "Save backed up: $dest" -ForegroundColor Green
}

# --- Other ASI plugins ---

if ($DisableOtherAsi) {
    Get-ChildItem $scripts -Filter '*.asi' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne 're0inv.asi' } |
        ForEach-Object {
            $to = "$($_.FullName).disabled-by-re0inv"
            Move-Item $_.FullName $to -Force
            $manifest.Renamed += @{ From = $_.FullName; To = $to }
            Write-Host "Disabled: $($_.Name)" -ForegroundColor Yellow
        }
} else {
    $others = Get-ChildItem $scripts -Filter '*.asi' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne 're0inv.asi' }
    if ($others) {
        Write-Host 'Other ASI plugins are present and will stay loaded:' -ForegroundColor Yellow
        $others | ForEach-Object { Write-Host "  $($_.Name)" -ForegroundColor Yellow }
        Write-Host 'They hook the same subsystem. Pass -DisableOtherAsi to rename them.' -ForegroundColor Yellow
    }
}

# --- The mod ---

$asi = Join-Path $scripts 're0inv.asi'
if (-not (Test-Path $asi)) { $manifest.Created += $asi }
Copy-Item $dll $asi -Force
Write-Host "Installed: $asi" -ForegroundColor Green

$iniPath = Join-Path $GameDir 're0inv.ini'
if (-not (Test-Path $iniPath)) { $manifest.Created += $iniPath }

$ini = Get-Content (Join-Path $repo 're0inv.ini') -Raw
$ini = $ini -replace 'Slots=\d+', "Slots=$Slots"
$ini = $ini -replace 'Level=\w+', "Level=$LogLevel"
$ini = $ini -replace 'DumpText=\d', ("DumpText=" + [int][bool]$DumpText)
$ini = $ini -replace 'Probe=\d', ("Probe=" + [int][bool]$Probe)
Set-Content $iniPath -Value $ini -Encoding utf8
Write-Host "Installed: $iniPath" -ForegroundColor Green

# The mod creates these at runtime; record them so uninstall clears them too.
$manifest.Created += (Join-Path $GameDir 're0inv.log')
$manifest.Created += (Join-Path $GameDir 're0hd_text_dump.bin')

$manifestPath = Join-Path $backupRoot 'manifest.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content $manifestPath -Encoding utf8

Write-Host ''
Write-Host "Manifest: $manifestPath" -ForegroundColor DarkGray
Write-Host 'Undo everything with: tools\uninstall.ps1' -ForegroundColor Cyan
