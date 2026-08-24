# hexdump-va.ps1 - Dump de bytes de um VA do PE, lendo do arquivo em disco.
# Uso: powershell -File tools\hexdump-va.ps1 -Path work\re0hd.exe -Va 0x0050DC70 -Count 64

param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Va,
    [int]$Count = 64
)

$ErrorActionPreference = 'Stop'
$vaVal = [Convert]::ToUInt32($Va, 16)

$bytes = [System.IO.File]::ReadAllBytes($Path)
$peOff = [BitConverter]::ToInt32($bytes, 0x3C)
$coff = $peOff + 4
$numSections = [BitConverter]::ToUInt16($bytes, $coff + 2)
$sizeOptHdr = [BitConverter]::ToUInt16($bytes, $coff + 16)
$opt = $coff + 20
$imageBase = [BitConverter]::ToUInt32($bytes, $opt + 28)
$secStart = $opt + $sizeOptHdr

$fileOff = $null
$secName = $null
for ($i = 0; $i -lt $numSections; $i++) {
    $s = $secStart + ($i * 40)
    $name = [System.Text.Encoding]::ASCII.GetString($bytes, $s, 8).TrimEnd([char]0)
    $vsize = [BitConverter]::ToUInt32($bytes, $s + 8)
    $vaddr = [BitConverter]::ToUInt32($bytes, $s + 12)
    $rptr = [BitConverter]::ToUInt32($bytes, $s + 20)
    $lo = $imageBase + $vaddr
    if ($vaVal -ge $lo -and $vaVal -lt ($lo + $vsize)) {
        $fileOff = $rptr + ($vaVal - $lo)
        $secName = $name
        break
    }
}

if ($null -eq $fileOff) { throw ("VA 0x{0:X8} nao cai em nenhuma secao" -f $vaVal) }

Write-Output ("VA 0x{0:X8}  secao {1}  file offset 0x{2:X8}" -f $vaVal, $secName, $fileOff)
Write-Output ""

$printable = ''
for ($i = 0; $i -lt $Count; $i += 16) {
    $line = ("0x{0:X8}  " -f ($vaVal + $i))
    $ascii = ''
    for ($j = 0; $j -lt 16; $j++) {
        if (($i + $j) -lt $Count) {
            $b = $bytes[$fileOff + $i + $j]
            $line += ("{0:X2} " -f $b)
            if ($b -ge 0x20 -and $b -lt 0x7F) { $ascii += [char]$b } else { $ascii += '.' }
        } else { $line += '   ' }
        if ($j -eq 7) { $line += ' ' }
    }
    Write-Output ("{0} |{1}|" -f $line, $ascii)
}
