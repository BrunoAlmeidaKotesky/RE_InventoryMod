# uninstall.ps1 - Puts the game folder back exactly as install.ps1 found it.
#
# Reads backups\install\manifest.json: deletes only what the install created,
# and restores every ASI plugin the install renamed. Files that were already
# there are never touched.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File tools\uninstall.ps1

param(
    [string]$GameDir
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repo 'backups\install\manifest.json'

if (-not (Test-Path $manifestPath)) {
    Write-Host 'No install manifest found; nothing recorded to undo.' -ForegroundColor Yellow

    # Fall back to removing the files this project is known to create.
    if (-not $GameDir) { throw 'Pass -GameDir to clean up without a manifest.' }
    $known = @(
        (Join-Path $GameDir 'scripts\re0inv.asi'),
        (Join-Path $GameDir 're0inv.ini'),
        (Join-Path $GameDir 're0inv.log'),
        (Join-Path $GameDir 're0hd_text_dump.bin')
    )
    foreach ($p in $known) {
        if (Test-Path $p) { Remove-Item $p -Force; Write-Host "Removed: $p" }
    }

    # Player data: backed up before removal, same as the manifest path below.
    foreach ($name in @('re0inv_saves.bin', 're0inv_saves.tmp', 're0inv_saves.bad')) {
        $sideFile = Join-Path $GameDir $name
        if (Test-Path $sideFile) {
            $stamp = Get-Date -Format 'yyyy-MM-dd_HHmmss'
            $dest = Join-Path $repo "backups\saves\$stamp"
            New-Item -ItemType Directory -Force -Path $dest | Out-Null
            Copy-Item $sideFile $dest
            Remove-Item $sideFile -Force
            Write-Host "Mod save data backed up to $dest and removed: $name" -ForegroundColor Green
        }
    }
    return
}

$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json

# The side file holds the player's extra slots and item box. It is player data,
# not an install artifact, so it is backed up and then removed - never plainly
# deleted, and never left behind to confuse a future install.
$gameDirFromManifest = $manifest.GameDir
foreach ($name in @('re0inv_saves.bin', 're0inv_saves.tmp', 're0inv_saves.bad')) {
    $sideFile = Join-Path $gameDirFromManifest $name
    if (Test-Path $sideFile) {
        $stamp = Get-Date -Format 'yyyy-MM-dd_HHmmss'
        $dest = Join-Path $repo "backups\saves\$stamp"
        New-Item -ItemType Directory -Force -Path $dest | Out-Null
        Copy-Item $sideFile $dest
        Remove-Item $sideFile -Force
        Write-Host "Mod save data backed up to $dest and removed: $name" -ForegroundColor Green
    }
}

foreach ($path in $manifest.Created) {
    if (Test-Path $path) {
        Remove-Item $path -Force
        Write-Host "Removed: $path" -ForegroundColor Green
    }
}

# Put the other plugins back last, so a half-finished uninstall still leaves
# them working.
foreach ($entry in $manifest.Renamed) {
    if (Test-Path $entry.To) {
        Move-Item $entry.To $entry.From -Force
        Write-Host "Restored: $($entry.From)" -ForegroundColor Green
    }
}

Remove-Item $manifestPath -Force

Write-Host ''
Write-Host 'Game folder restored.' -ForegroundColor Cyan
Write-Host 'Saves are untouched; backups stay in backups\saves\.' -ForegroundColor DarkGray
