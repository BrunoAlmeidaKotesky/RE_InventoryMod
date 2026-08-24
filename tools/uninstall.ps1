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
    return
}

$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json

foreach ($path in $manifest.Created) {
    if (Test-Path $path) {
        Remove-Item $path -Force
        Write-Host "Removed: $path" -ForegroundColor Green
    }
}

# Restore renamed plugins last, so a half-finished uninstall still leaves the
# other mods working.
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
