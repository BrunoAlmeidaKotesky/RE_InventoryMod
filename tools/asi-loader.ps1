# asi-loader.ps1 - Fetches Ultimate ASI Loader (MIT, ThirteenAG) into third_party\
# and verifies it, so the mod can ship it and the dev install can use it.
#
# Dot-source this file, then call Get-AsiLoader; it returns @{ Dll; License }.
# Pinned by tag and by hash: anything other than what was reviewed fails here.

$AsiLoaderTag = 'v9.7.4'
$AsiLoaderZipSha256 = '14b3a1ad018899571ac9aa01482977f3c6d49e6cba99f552d01c5acacd1315e1'
$AsiLoaderDllSha256 = 'ec2f4824eca58dd40f425756a4a7cec77b8e381f21d11d7c846ec4b339b617ab'

function Get-AsiLoader {
    param([string]$Repo = (Split-Path -Parent $PSScriptRoot))

    $dir = Join-Path $Repo "third_party\asi-loader\$AsiLoaderTag"
    $zip = Join-Path $dir 'Ultimate-ASI-Loader-NoPDB.zip'
    $dll = Join-Path $dir 'extracted\dinput8.dll'
    $license = Join-Path $dir 'license'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null

    if (-not (Test-Path $zip)) {
        Write-Host "Downloading Ultimate ASI Loader $AsiLoaderTag..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri "https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases/download/$AsiLoaderTag/Ultimate-ASI-Loader-NoPDB.zip" -OutFile $zip
    }
    $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $AsiLoaderZipSha256) { throw "Ultimate ASI Loader zip hash mismatch: $actual" }

    if (-not (Test-Path $dll)) {
        Expand-Archive -Path $zip -DestinationPath (Join-Path $dir 'extracted') -Force
    }
    $actual = (Get-FileHash $dll -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $AsiLoaderDllSha256) { throw "Ultimate ASI Loader dinput8.dll hash mismatch: $actual" }

    if (-not (Test-Path $license)) {
        Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/ThirteenAG/Ultimate-ASI-Loader/master/license' -OutFile $license
    }
    if (-not (Select-String -Path $license -Pattern 'MIT License' -Quiet)) { throw 'Ultimate ASI Loader license file is not the MIT text expected.' }

    return @{ Dll = $dll; License = $license }
}
