# release.ps1 - Builds every distributable variant and packages it for a release.
#
# Four packages come out of one codebase, selected by Cargo features:
#
#   RE0-QoL-Bundle          expanded + itembox + doors
#   RE0-InventoryExpansion  expanded slots only
#   RE0-ItemBox             the item box only
#   RE0-DoorSkip            the door skip only
#
# Every package ships the DLL under the same name, re0inv.asi, on purpose:
# installing one variant over another replaces it, so two variants can never be
# loaded together fighting over the same hooks.
#
# Nothing from the game ships in any package, and no tool needs to be run: the
# mod itself writes the typewriter prompt's message archives on first launch,
# from the player's own files. Installing is copying the zip into the game
# folder.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1
#
# Then, to publish (asks nothing, uploads dist\*.zip to a tagged release):
#   gh release create v<version> dist\*.zip --title "..." --notes-file <notes>

param(
    [string]$Target = 'i686-pc-windows-gnu',
    [string]$Toolchain = '+stable-i686-pc-windows-gnu'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'cargo not found. Install Rust from https://rustup.rs'
}

Set-Location $repo

$version = (Select-String -Path 'crates\re0inv\Cargo.toml' -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

$dist = Join-Path $repo 'dist'
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist | Out-Null

$variants = @(
    @{
        Name = 'RE0-QoL-Bundle'
        Features = 'expanded,itembox,doors'
        Ini = @{ Slots = 12; ItemBox = 1; SkipDoors = 1 }
        Blurb = 'All three improvements: 12 inventory slots per character, an item box at every typewriter, and instant doors.'
        BlurbPt = 'As tres melhorias: 12 slots de inventario por personagem, bau de itens em toda maquina de escrever, e portas instantaneas.'
        Messages = $true
    },
    @{
        Name = 'RE0-InventoryExpansion'
        Features = 'expanded'
        Ini = @{ Slots = 12; ItemBox = 0; SkipDoors = 0 }
        Blurb = '12 inventory slots per character instead of 6. Press down past the last row (or click the right stick) to scroll.'
        BlurbPt = '12 slots de inventario por personagem em vez de 6. Pressione para baixo alem da ultima fileira (ou clique o analogico direito) para rolar.'
        Messages = $false
    },
    @{
        Name = 'RE0-ItemBox'
        Features = 'itembox'
        Ini = @{ Slots = 6; ItemBox = 1; SkipDoors = 0 }
        Blurb = 'An item box, offered on every typewriter prompt, like the rest of the series.'
        BlurbPt = 'Um bau de itens, oferecido na propria mensagem da maquina de escrever, como no resto da serie.'
        Messages = $true
    },
    @{
        Name = 'RE0-DoorSkip'
        Features = 'doors'
        Ini = @{ Slots = 6; ItemBox = 0; SkipDoors = 1 }
        Blurb = 'Door transitions cut from about three and a half seconds to well under one.'
        BlurbPt = 'Transicoes de porta cortadas de uns tres segundos e meio para bem menos de um.'
        Messages = $false
    }
)

$iniTemplate = Get-Content (Join-Path $repo 're0inv.ini') -Raw

foreach ($variant in $variants) {
    $name = $variant.Name
    Write-Host "Building $name ($($variant.Features))..." -ForegroundColor Cyan

    & cargo $Toolchain build -q --release --target $Target -p re0inv `
        --no-default-features --features $variant.Features
    if ($LASTEXITCODE -ne 0) { throw "$name build failed (exit $LASTEXITCODE)" }

    $stage = Join-Path $dist $name
    New-Item -ItemType Directory -Path (Join-Path $stage 'scripts') -Force | Out-Null

    Copy-Item (Join-Path $repo "target\$Target\release\re0inv.dll") `
        (Join-Path $stage 'scripts\re0inv.asi')

    # The ini preset: the template with this variant's switches applied.
    $ini = $iniTemplate
    $ini = $ini -replace '(?m)^Slots=\d+', "Slots=$($variant.Ini.Slots)"
    $ini = $ini -replace '(?m)^ItemBox=\d', "ItemBox=$($variant.Ini.ItemBox)"
    $ini = $ini -replace '(?m)^SkipDoors=\d', "SkipDoors=$($variant.Ini.SkipDoors)"
    Set-Content (Join-Path $stage 're0inv.ini') -Value $ini -Encoding utf8

    $readme = @"
$name v$version
$('=' * ($name.Length + $version.Length + 2))

$($variant.Blurb)

For Resident Evil 0 HD Remaster on Steam (build Jan 28 2025).

INSTALL
1. Copy everything in this zip into the game folder
   (the one containing re0hd.exe, usually
   ...\steamapps\common\Resident Evil 0).
2. You also need Ultimate ASI Loader: download dinput8.dll (Win32 build) from
   https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases
   and put it in the same folder. Skip this if you already have it.
$(if ($variant.Messages) { "
On the first launch the mod writes the typewriter prompt's text files
(msg_*_inv.arc) from your own game files. Your original files are never
modified, so verifying the game files on Steam reports nothing." })

HOW TO USE
- Inventory: with the cursor on the bottom row, press down again to scroll.
  The right stick click and Page Up / Page Down also scroll.
- Item box: use a typewriter and pick the new choice on its prompt. Move
  items with Exchange, both ways. Home or a left stick click also opens it
  near a typewriter.
- Combine: the first item's row stays on screen while the rest scrolls, so
  the second item can be on any page.

If the game freezes, wait about fifteen seconds before closing it: the mod
writes re0inv_hang.dmp and re0inv_hang.txt next to the game. Send those with
re0inv.log when reporting a problem.

Only one variant of this mod can be installed at a time; installing another
one over it replaces it.

UNINSTALL
Delete scripts\re0inv.asi, re0inv.ini, re0inv.log and re0inv_hang.* from the
game folder.
re0inv_saves.bin holds the mod's own saved items (extra slots and box) - keep
it if you plan to reinstall, delete it otherwise.
$(if ($variant.Messages) { "Also delete nativePC\arc\message\msg_*_inv.arc." })

Settings live in re0inv.ini. Set Mod=0 to disable without uninstalling.

-------------------------------------------------------------------------------

$($variant.BlurbPt)

Para Resident Evil 0 HD Remaster na Steam (build de 28 de janeiro de 2025).

INSTALAR
1. Copie tudo deste zip para a pasta do jogo
   (a que contem o re0hd.exe, normalmente
   ...\steamapps\common\Resident Evil 0).
2. Voce tambem precisa do Ultimate ASI Loader: baixe o dinput8.dll (build
   Win32) em https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases
   e coloque na mesma pasta. Pule se ja tiver.
$(if ($variant.Messages) { "
Na primeira vez que o jogo abrir, o mod gera os textos da maquina de
escrever (msg_*_inv.arc) a partir dos seus proprios arquivos. Os originais
nunca sao modificados; verificar os arquivos na Steam nao acusa nada." })

COMO USAR
- Inventario: com o cursor na ultima fileira, aperte para baixo de novo para
  rolar. Clique do analogico direito e Page Up / Page Down tambem rolam.
- Bau de itens: use uma maquina de escrever e escolha a opcao nova na
  mensagem dela. Mova itens com o Exchange, nos dois sentidos. Home ou clique
  do analogico esquerdo tambem abre perto de uma maquina.
- Combine: a fileira do primeiro item fica na tela enquanto o resto rola,
  entao o segundo item pode estar em qualquer pagina.

Se o jogo travar, espere uns quinze segundos antes de fechar: o mod grava
re0inv_hang.dmp e re0inv_hang.txt ao lado do jogo. Mande esses junto com o
re0inv.log ao relatar um problema.

So uma variante deste mod pode estar instalada por vez; instalar outra por
cima substitui.

DESINSTALAR
Apague scripts\re0inv.asi, re0inv.ini, re0inv.log e re0inv_hang.* da pasta
do jogo.
re0inv_saves.bin guarda os itens salvos pelo mod (slots extras e bau) -
mantenha se pretende reinstalar, apague caso contrario.
$(if ($variant.Messages) { "Apague tambem nativePC\arc\message\msg_*_inv.arc." })

Configuracoes em re0inv.ini. Mod=0 desativa sem desinstalar.
"@
    Set-Content (Join-Path $stage 'README.txt') -Value $readme -Encoding utf8

    $zip = Join-Path $dist "$name-v$version.zip"
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
    Write-Host ("Packaged {0} ({1:N0} bytes)" -f $zip, (Get-Item $zip).Length) -ForegroundColor Green
}

# The loop leaves the last variant's DLL in target\, and install.ps1 copies
# whatever is there: a dev install right after a release shipped the door skip
# alone. Rebuild the full mod so target\ holds what a developer expects.
Write-Host 'Restoring the default build...' -ForegroundColor Cyan
& cargo $Toolchain build -q --release --target $Target -p re0inv
if ($LASTEXITCODE -ne 0) { throw "default build failed (exit $LASTEXITCODE)" }

Write-Host ''
Write-Host "Packages in $dist" -ForegroundColor Cyan
Write-Host "Publish with: gh release create v$version dist\*.zip --title `"v$version`" --generate-notes" -ForegroundColor DarkGray
