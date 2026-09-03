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
    [int]$Slots = 12,

    # The optional features, off unless asked for.
    [switch]$ItemBox,
    [int]$BoxSlots = 24,
    [switch]$SkipDoors
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if (-not (Test-Path $GameDir)) { throw "Game directory not found: $GameDir" }

$dll = Join-Path $repo 'target\i686-pc-windows-gnu\release\re0inv.dll'
if (-not (Test-Path $dll)) { throw "Build the mod first: tools\build.ps1" }

$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }

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

# Braces are required: without them PowerShell reads $env:ProgramFiles and
# leaves a literal "(x86)" behind, and no save is ever found to back up.
$saveDir = "${env:ProgramFiles(x86)}\Steam\userdata"
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
    # Moved out of the folder entirely, not renamed in place. The loader scans
    # this directory, and leaving a disabled plugin sitting in it relies on the
    # scan matching exactly what we assume. Somewhere else it cannot be found at
    # all, whatever the loader does.
    $parked = Join-Path $repo 'backups\install\disabled-asi'
    New-Item -ItemType Directory -Force -Path $parked | Out-Null

    Get-ChildItem $scripts -Filter '*.asi' -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne 're0inv.asi' } |
        ForEach-Object {
            $to = Join-Path $parked $_.Name
            Move-Item $_.FullName $to -Force
            $manifest.Renamed += @{ From = $_.FullName; To = $to }
            Write-Host "Moved out of scripts: $($_.Name)" -ForegroundColor Yellow
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

# --- Message archives for the typewriter prompt ---

# The box needs a third choice on the typewriter, and what a prompt offers comes
# from the game's message files. re0msg reads the player's own archive, edits a
# copy in memory and writes msg_<lang>_inv.arc next to it. The originals are
# never touched, so verifying the game files reports nothing.
if ($ItemBox) {
    $messageDir = Join-Path $GameDir 'nativePC\arc\message'

    if (-not (Test-Path $messageDir)) {
        Write-Host "No message archives at $messageDir; the typewriter keeps its own prompt." -ForegroundColor Yellow
    } else {
        Write-Host 'Checking the message format before writing anything...' -ForegroundColor DarkGray
        & $cargo run -q -p msgtool --release -- verify $messageDir
        if ($LASTEXITCODE -ne 0) { throw 'Message archives did not rebuild unchanged; refusing to write.' }

        & $cargo run -q -p msgtool --release -- build $messageDir
        if ($LASTEXITCODE -ne 0) { throw 'Could not build the message archives.' }

        Get-ChildItem $messageDir -Filter 'msg_*_inv.arc' |
            ForEach-Object { $manifest.Created += $_.FullName }
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

# Anchored to the start of a line. Unanchored, "Slots=" also matches inside
# "BoxSlots=", which silently rewrote the wrong setting.
$ini = $ini -replace '(?m)^Slots=\d+', "Slots=$Slots"
$ini = $ini -replace '(?m)^BoxSlots=\d+', "BoxSlots=$BoxSlots"
$ini = $ini -replace '(?m)^Level=\w+', "Level=$LogLevel"
$ini = $ini -replace '(?m)^DumpText=\d', ("DumpText=" + [int][bool]$DumpText)
$ini = $ini -replace '(?m)^Probe=\d', ("Probe=" + [int][bool]$Probe)
$ini = $ini -replace '(?m)^ItemBox=\d', ("ItemBox=" + [int][bool]$ItemBox)
$ini = $ini -replace '(?m)^SkipDoors=\d', ("SkipDoors=" + [int][bool]$SkipDoors)
Set-Content $iniPath -Value $ini -Encoding utf8
Write-Host "Installed: $iniPath" -ForegroundColor Green

# The mod creates these at runtime; record them so uninstall clears them too.
$manifest.Created += (Join-Path $GameDir 're0inv.log')
$manifest.Created += (Join-Path $GameDir 're0hd_text_dump.bin')
$manifest.Created += (Join-Path $GameDir 're0inv_hang.dmp')
$manifest.Created += (Join-Path $GameDir 're0inv_hang.txt')

$manifestPath = Join-Path $backupRoot 'manifest.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content $manifestPath -Encoding utf8

Write-Host ''
Write-Host "Manifest: $manifestPath" -ForegroundColor DarkGray
Write-Host 'Undo everything with: tools\uninstall.ps1' -ForegroundColor Cyan
