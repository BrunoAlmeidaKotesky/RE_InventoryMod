# build.ps1 - Compila o mod para 32-bit com o ambiente MSVC correto.
#
# Por que este script existe:
#
#  1. O rustc nao localiza sozinho esta instalacao do Visual Studio, entao o
#     ambiente do MSVC precisa ser carregado explicitamente antes do cargo.
#  2. O `link` do Git Bash (GNU coreutils) sequestra o `link.exe` do MSVC se o
#     build sair de um shell POSIX. Compile sempre pelo PowerShell.
#
# Uso:
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1 -Debug
#   powershell -ExecutionPolicy Bypass -File tools\build.ps1 -Install

param(
    [switch]$Debug,
    [switch]$Install,
    [string]$GameDir = "D:\SteamLibrary\steamapps\common\Resident Evil 0"
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

# --- Ambiente MSVC (target x86, host x64) ---
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe nao encontrado. Visual Studio instalado?" }

$vsPath = & $vswhere -products * -latest -property installationPath
if (-not $vsPath) { throw "Nenhuma instalacao do Visual Studio encontrada." }

$devShell = Join-Path $vsPath "Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
if (-not (Test-Path $devShell)) { throw "DevShell nao encontrado em $devShell" }

Import-Module $devShell
$instanceId = & $vswhere -products * -latest -property instanceId
Enter-VsDevShell -VsInstanceId $instanceId -SkipAutomaticLocation `
    -DevCmdArguments '-arch=x86 -host_arch=x64' | Out-Null

Write-Host "MSVC carregado: $vsPath (target x86)" -ForegroundColor DarkGray

# --- cargo ---
$cargoBin = "$env:USERPROFILE\.cargo\bin"
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }

Set-Location $repo

$profileArgs = @('--release')
$outDir = 'release'
if ($Debug) { $profileArgs = @(); $outDir = 'debug' }

Write-Host "Compilando ($outDir, i686-pc-windows-msvc)..." -ForegroundColor Cyan
& cargo build @profileArgs --target=i686-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "cargo build falhou (exit $LASTEXITCODE)" }

$dll = Join-Path $repo "target\i686-pc-windows-msvc\$outDir\re0inv.dll"
if (-not (Test-Path $dll)) { throw "DLL nao encontrada em $dll" }

$info = Get-Item $dll
Write-Host ("OK: {0} ({1:N0} bytes)" -f $dll, $info.Length) -ForegroundColor Green

# --- Instalacao opcional no jogo ---
if ($Install) {
    if (-not (Test-Path $GameDir)) { throw "Diretorio do jogo nao encontrado: $GameDir" }

    $scripts = Join-Path $GameDir "scripts"
    New-Item -ItemType Directory -Force -Path $scripts | Out-Null

    # re0box hookeia o mesmo subsistema de inventario. Os dois juntos brigam.
    $re0box = Join-Path $scripts "re0box.asi"
    if (Test-Path $re0box) {
        Write-Host "AVISO: re0box.asi esta instalado e conflita com este mod." -ForegroundColor Yellow
        Write-Host "       Renomeie ou remova antes de testar." -ForegroundColor Yellow
    }

    $target = Join-Path $scripts "re0inv.asi"
    Copy-Item $dll $target -Force
    Write-Host "Instalado: $target" -ForegroundColor Green

    # O .ini so e copiado se ainda nao existir, para nao apagar ajustes locais.
    $iniSrc = Join-Path $repo "re0inv.ini"
    $iniDst = Join-Path $GameDir "re0inv.ini"
    if ((Test-Path $iniSrc) -and -not (Test-Path $iniDst)) {
        Copy-Item $iniSrc $iniDst
        Write-Host "Config instalada: $iniDst" -ForegroundColor Green
    }
}
