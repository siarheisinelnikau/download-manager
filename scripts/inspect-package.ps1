param([string]$Path = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist/DownloadManager-v0.1.0.exe'))
$ErrorActionPreference = 'Stop'
$bytes = [System.IO.File]::ReadAllBytes($Path)
$pe = [BitConverter]::ToInt32($bytes, 60)
if ([Text.Encoding]::ASCII.GetString($bytes, $pe, 4) -ne "PE`0`0") { throw 'Invalid PE signature' }
$machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
$optional = $pe + 24
if ($machine -ne 0x8664 -or [BitConverter]::ToUInt16($bytes, $optional) -ne 0x20b) { throw 'Expected Windows x64 PE32+ executable' }
if ([BitConverter]::ToUInt16($bytes, $optional + 68) -ne 2) { throw 'Expected Windows GUI subsystem' }
$sectionCount = [BitConverter]::ToUInt16($bytes, $pe + 6)
$sections = $optional + [BitConverter]::ToUInt16($bytes, $pe + 20)
function Convert-Rva([uint32]$rva) {
    for ($i = 0; $i -lt $sectionCount; $i++) {
        $at = $sections + $i * 40
        $virtualSize = [BitConverter]::ToUInt32($bytes, $at + 8)
        $address = [BitConverter]::ToUInt32($bytes, $at + 12)
        $rawSize = [BitConverter]::ToUInt32($bytes, $at + 16)
        if ($rva -ge $address -and $rva -lt $address + [Math]::Max($virtualSize, $rawSize)) {
            return [int]([BitConverter]::ToUInt32($bytes, $at + 20) + $rva - $address)
        }
    }
    throw "RVA is outside PE sections: $rva"
}
$importRva = [BitConverter]::ToUInt32($bytes, $optional + 120)
$imports = [System.Collections.Generic.List[string]]::new()
if ($importRva -ne 0) {
    $descriptor = Convert-Rva $importRva
    while (($nameRva = [BitConverter]::ToUInt32($bytes, $descriptor + 12)) -ne 0) {
        $start = Convert-Rva $nameRva
        $end = $start
        while ($bytes[$end] -ne 0) { $end++ }
        $imports.Add([Text.Encoding]::ASCII.GetString($bytes, $start, $end - $start))
        $descriptor += 20
    }
}
if ($imports | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt)' }) { throw 'Unexpected dynamic MSVC CRT dependency' }
[PSCustomObject]@{ Path = $Path; Bytes = $bytes.Length; Machine = 'x64'; Subsystem = 'Windows GUI'; Imports = @($imports | Sort-Object -Unique) } | ConvertTo-Json -Depth 3

