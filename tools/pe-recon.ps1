# pe-recon.ps1 - Leitura de cabecalhos PE 32-bit sem dependencias externas.
# Uso: powershell -File tools\pe-recon.ps1 -Path work\re0hd.exe

param(
    [Parameter(Mandatory = $true)][string]$Path
)

$ErrorActionPreference = 'Stop'

$bytes = [System.IO.File]::ReadAllBytes($Path)
Write-Output "Arquivo: $Path"
Write-Output "Tamanho: $($bytes.Length) bytes"

if ($bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) { throw "Nao e um arquivo MZ/PE" }

$peOff = [BitConverter]::ToInt32($bytes, 0x3C)
if ([BitConverter]::ToUInt32($bytes, $peOff) -ne 0x00004550) { throw "Assinatura PE ausente em 0x$($peOff.ToString('X'))" }

# --- COFF File Header ---
$coff = $peOff + 4
$machine = [BitConverter]::ToUInt16($bytes, $coff)
$numSections = [BitConverter]::ToUInt16($bytes, $coff + 2)
$timeStamp = [BitConverter]::ToUInt32($bytes, $coff + 4)
$sizeOptHdr = [BitConverter]::ToUInt16($bytes, $coff + 16)
$characteristics = [BitConverter]::ToUInt16($bytes, $coff + 18)

$machineName = switch ($machine) {
    0x014C { 'IMAGE_FILE_MACHINE_I386 (x86 32-bit)' }
    0x8664 { 'IMAGE_FILE_MACHINE_AMD64 (x64)' }
    default { "desconhecida (0x{0:X4})" -f $machine }
}

# --- Optional Header ---
$opt = $coff + 20
$magic = [BitConverter]::ToUInt16($bytes, $opt)
$magicName = switch ($magic) { 0x10B { 'PE32' } 0x20B { 'PE32+' } default { "0x{0:X4}" -f $magic } }
$entryPointRva = [BitConverter]::ToUInt32($bytes, $opt + 16)

if ($magic -eq 0x10B) {
    $imageBase = [BitConverter]::ToUInt32($bytes, $opt + 28)
    $dllChar = [BitConverter]::ToUInt16($bytes, $opt + 70)
    $sizeOfImage = [BitConverter]::ToUInt32($bytes, $opt + 56)
} else {
    $imageBase = [BitConverter]::ToUInt64($bytes, $opt + 24)
    $dllChar = [BitConverter]::ToUInt16($bytes, $opt + 70)
    $sizeOfImage = [BitConverter]::ToUInt32($bytes, $opt + 56)
}

Write-Output ""
Write-Output "=== COFF File Header ==="
Write-Output ("Machine            : {0}" -f $machineName)
Write-Output ("NumberOfSections   : {0}" -f $numSections)
Write-Output ("TimeDateStamp      : 0x{0:X8}  ({1} UTC)" -f $timeStamp, ([DateTimeOffset]::FromUnixTimeSeconds($timeStamp).UtcDateTime))
Write-Output ("Characteristics    : 0x{0:X4}" -f $characteristics)

Write-Output ""
Write-Output "=== Optional Header ==="
Write-Output ("Magic              : {0}" -f $magicName)
Write-Output ("ImageBase          : 0x{0:X8}" -f $imageBase)
Write-Output ("SizeOfImage        : 0x{0:X8}" -f $sizeOfImage)
Write-Output ("AddressOfEntryPoint: 0x{0:X8}  (VA 0x{1:X8})" -f $entryPointRva, ($imageBase + $entryPointRva))
Write-Output ("DllCharacteristics : 0x{0:X4}" -f $dllChar)

$flags = @{
    0x0020 = 'HIGH_ENTROPY_VA'
    0x0040 = 'DYNAMIC_BASE (ASLR)'
    0x0080 = 'FORCE_INTEGRITY'
    0x0100 = 'NX_COMPAT (DEP)'
    0x0200 = 'NO_ISOLATION'
    0x0400 = 'NO_SEH'
    0x0800 = 'NO_BIND'
    0x1000 = 'APPCONTAINER'
    0x2000 = 'WDM_DRIVER'
    0x4000 = 'GUARD_CF'
    0x8000 = 'TERMINAL_SERVER_AWARE'
}
foreach ($k in ($flags.Keys | Sort-Object)) {
    $set = if (($dllChar -band $k) -ne 0) { 'SIM' } else { 'nao ' }
    Write-Output ("  [{0}] {1}" -f $set, $flags[$k])
}

$aslr = ($dllChar -band 0x0040) -ne 0
Write-Output ""
if ($aslr) {
    Write-Output "VEREDITO ASLR: ATIVO -> enderecos absolutos exigem rebase em runtime."
} else {
    Write-Output "VEREDITO ASLR: DESATIVADO -> enderecos absolutos sao validos direto."
}

# --- Sections ---
$secStart = $opt + $sizeOptHdr
Write-Output ""
Write-Output "=== Secoes ==="
Write-Output ("{0,-10} {1,-12} {2,-12} {3,-12} {4,-12} {5}" -f 'Nome', 'VirtAddr', 'VirtSize', 'RawPtr', 'RawSize', 'Flags')

$sections = @()
for ($i = 0; $i -lt $numSections; $i++) {
    $s = $secStart + ($i * 40)
    $name = [System.Text.Encoding]::ASCII.GetString($bytes, $s, 8).TrimEnd([char]0)
    $vsize = [BitConverter]::ToUInt32($bytes, $s + 8)
    $vaddr = [BitConverter]::ToUInt32($bytes, $s + 12)
    $rsize = [BitConverter]::ToUInt32($bytes, $s + 16)
    $rptr = [BitConverter]::ToUInt32($bytes, $s + 20)
    $chr = [BitConverter]::ToUInt32($bytes, $s + 36)

    $sections += [PSCustomObject]@{
        Name = $name; VirtualAddress = $vaddr; VirtualSize = $vsize
        PointerToRawData = $rptr; SizeOfRawData = $rsize; Characteristics = $chr
    }

    $f = ''
    if (($chr -band 0x20000000) -ne 0) { $f += 'X' }
    if (($chr -band 0x40000000) -ne 0) { $f += 'R' }
    if (($chr -band 0x80000000) -ne 0) { $f += 'W' }
    Write-Output ("{0,-10} 0x{1:X8}   0x{2:X8}   0x{3:X8}   0x{4:X8}   {5}" -f $name, $vaddr, $vsize, $rptr, $rsize, $f)
}

# --- Mapa VA -> file offset ---
Write-Output ""
Write-Output "=== Conversao VA -> offset de arquivo ==="
foreach ($s in $sections) {
    $vaLo = $imageBase + $s.VirtualAddress
    $vaHi = $vaLo + $s.VirtualSize
    Write-Output ("{0,-10} VA 0x{1:X8} - 0x{2:X8}  =>  offset = VA - 0x{3:X8}" -f $s.Name, $vaLo, $vaHi, ($vaLo - $s.PointerToRawData))
}
