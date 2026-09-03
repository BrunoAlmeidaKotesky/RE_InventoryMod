# mdmp.ps1 - Reads a Windows minidump without a debugger: modules, threads
# (eip/esp), exception, a return-address scan of the crashing stack, and its
# printable strings. Usage:
#   powershell -File tools\mdmp.ps1 -Path $env:LOCALAPPDATA\CrashDumps\re0hd.exe.NNN.dmp
#   ... -StringsTid <tid>   printable runs on that thread's stack
#   ... -ScanTid <tid>      scan only that thread (a hang dump has no exception)
param([Parameter(Mandatory)][string]$Path, [int]$StringsTid = -1, [int]$ScanTid = -1, [string[]]$Interesting = @('re0hd.exe','re0inv.asi','re0box.asi','dinput8.dll'))

$b = [System.IO.File]::ReadAllBytes($Path)
function U32($o) { [BitConverter]::ToUInt32($b, $o) }
function U64($o) { [BitConverter]::ToUInt64($b, $o) }
function U16($o) { [BitConverter]::ToUInt16($b, $o) }

if ([Text.Encoding]::ASCII.GetString($b, 0, 4) -ne 'MDMP') { throw 'not a minidump' }
$nStreams = U32 8; $dirRva = U32 12
"file: $Path ($($b.Length) bytes), $nStreams streams"

$streams = @{}
for ($i = 0; $i -lt $nStreams; $i++) {
    $e = $dirRva + 12 * $i
    $streams[[int](U32 $e)] = @{ Size = [int](U32 ($e + 4)); Rva = [int](U32 ($e + 8)) }
}

if ($streams[7]) { $arch = U16 $streams[7].Rva; "arch: $arch (0=x86, 9=x64)" }

# Modules
$mods = @()
if ($streams[4]) {
    $o = $streams[4].Rva; $n = U32 $o; $o += 4
    for ($i = 0; $i -lt $n; $i++) {
        $base = U64 $o; $size = U32 ($o + 8); $nameRva = U32 ($o + 20)
        $len = U32 $nameRva
        $name = [Text.Encoding]::Unicode.GetString($b, $nameRva + 4, $len)
        $mods += [pscustomobject]@{ Base = $base; Size = $size; Name = (Split-Path $name -Leaf); Path = $name }
        $o += 108
    }
}
"modules: $($mods.Count)"
$mods | Where-Object { $Interesting -contains $_.Name -or $_.Name -match '^(ntdll|kernel32|KERNELBASE|xinput|steam)' } |
    ForEach-Object { ('  {0,-22} base 0x{1:X8} size 0x{2:X8}' -f $_.Name, $_.Base, $_.Size) }

function ModOf($addr) {
    foreach ($m in $mods) { if ($addr -ge $m.Base -and $addr -lt ($m.Base + $m.Size)) { return ('{0}+0x{1:X}' -f $m.Name, ($addr - $m.Base)) } }
    return $null
}

function Ctx($rva, $size) {
    if ($size -lt 0xCC) { return $null }
    [pscustomobject]@{ Eip = (U32 ($rva + 0xB8)); Esp = (U32 ($rva + 0xC4)); Ebp = (U32 ($rva + 0xB4)); Eax = (U32 ($rva + 0xB0)); Ecx = (U32 ($rva + 0xAC)); Edi = (U32 ($rva + 0x9C)); Esi = (U32 ($rva + 0xA0)) }
}

# Threads
$threads = @()
if ($streams[3]) {
    $o = $streams[3].Rva; $n = U32 $o; $o += 4
    for ($i = 0; $i -lt $n; $i++) {
        $tid = U32 $o
        $stackStart = U64 ($o + 24); $stackSize = U32 ($o + 32); $stackRva = U32 ($o + 36)
        $ctxSize = U32 ($o + 40); $ctxRva = U32 ($o + 44)
        $threads += [pscustomobject]@{ Tid = $tid; StackStart = $stackStart; StackSize = $stackSize; StackRva = $stackRva; Ctx = (Ctx $ctxRva $ctxSize) }
        $o += 48
    }
}
"threads: $($threads.Count)"
foreach ($t in $threads) {
    $c = $t.Ctx
    if ($c) { ('  tid {0,-6} eip 0x{1:X8} {2,-28} esp 0x{3:X8} stack {4} bytes' -f $t.Tid, $c.Eip, (ModOf $c.Eip), $c.Esp, $t.StackSize) }
}

# Exception
$excTid = $null
if ($streams[6]) {
    $o = $streams[6].Rva
    $excTid = U32 $o
    $code = U32 ($o + 8); $addr = U64 ($o + 24); $nParams = U32 ($o + 32)
    $p0 = U64 ($o + 40); $p1 = U64 ($o + 48)
    ('exception: code 0x{0:X8} at 0x{1:X8} {2} on tid {3}; params {4}: 0x{5:X} 0x{6:X}' -f $code, $addr, (ModOf $addr), $excTid, $nParams, $p0, $p1)
    $c = Ctx (U32 ($o + 164)) (U32 ($o + 160))
    if ($c) { ('  ctx eip 0x{0:X8} esp 0x{1:X8} ebp 0x{2:X8} eax 0x{3:X8} ecx 0x{4:X8} esi 0x{5:X8} edi 0x{6:X8}' -f $c.Eip, $c.Esp, $c.Ebp, $c.Eax, $c.Ecx, $c.Esi, $c.Edi) }
}

# Stack scan: every dword on the stack that falls inside an interesting module.
function ScanStack($t, $limit) {
    $c = $t.Ctx
    if (-not $c -or $t.StackSize -eq 0) { return }
    $from = [Math]::Max(0, [int64]$c.Esp - [int64]$t.StackStart)
    $end = $t.StackSize
    $shown = 0
    for ($p = $from; $p + 4 -le $end -and $shown -lt $limit; $p += 4) {
        $v = U32 ($t.StackRva + $p)
        $m = ModOf $v
        if ($m -and ($Interesting | Where-Object { $m.StartsWith($_) })) {
            ('    esp+0x{0:X4}  0x{1:X8}  {2}' -f ($p - $from), $v, $m); $shown++
        }
    }
}

$targets = if ($ScanTid -ge 0) { $threads | Where-Object Tid -eq $ScanTid } elseif ($excTid -ne $null) { $threads | Where-Object Tid -eq $excTid } else { $threads }
foreach ($t in $targets) {
    "stack scan tid $($t.Tid):"
    ScanStack $t 40
}

# Printable ASCII runs on one thread's stack, for reading formatted messages.
if ($StringsTid -ge 0) {
    $t = $threads | Where-Object Tid -eq $StringsTid
    if ($t) {
        "strings on stack of tid $StringsTid (stack 0x{0:X8}, {1} bytes):" -f $t.StackStart, $t.StackSize
        $run = ''; $runAt = 0
        for ($p = 0; $p -lt $t.StackSize; $p++) {
            $c = $b[$t.StackRva + $p]
            if ($c -ge 0x20 -and $c -le 0x7E) { if ($run -eq '') { $runAt = $p }; $run += [char]$c }
            else { if ($run.Length -ge 6) { ('  +0x{0:X4}  {1}' -f $runAt, $run) }; $run = '' }
        }
    }
}
