param([string]$Path, [string]$ExpectedVersion)
$ErrorActionPreference = 'Stop'
if (-not $Path) {
    . (Join-Path $PSScriptRoot 'project-info.ps1')
    $package = Get-DownloadManagerPackage (Split-Path -Parent $PSScriptRoot)
    $Path = $package.AssetPath
    $ExpectedVersion = $package.Version
}
$versionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo([IO.Path]::GetFullPath($Path))
if ($ExpectedVersion -and ($versionInfo.FileVersion -ne $ExpectedVersion -or $versionInfo.ProductVersion -ne $ExpectedVersion)) { throw 'Packaged Windows version does not match Cargo.toml' }
if ($versionInfo.OriginalFilename -ne "DownloadManager-v$($versionInfo.ProductVersion).exe") { throw 'OriginalFilename does not match the packaged product version' }
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
if ($imports | Where-Object { $_ -match '^(opengl32|glu32|d3d[0-9]*|dxgi|vulkan-1)\.dll$' }) { throw 'Unexpected GPU graphics dependency in the CPU/GDI build' }
$resourceRva = [BitConverter]::ToUInt32($bytes, $optional + 128)
if ($resourceRva -eq 0) { throw 'Missing Windows application resources' }
$root = Convert-Rva $resourceRva
$entryCount = [BitConverter]::ToUInt16($bytes, $root + 12) + [BitConverter]::ToUInt16($bytes, $root + 14)
$resourceTypes = @()
for ($index=0; $index -lt $entryCount; $index++) {
    $id = [BitConverter]::ToUInt32($bytes, $root + 16 + $index * 8)
    if (($id -band 0x80000000) -eq 0) { $resourceTypes += $id }
}
if (3 -notin $resourceTypes -or 14 -notin $resourceTypes -or 16 -notin $resourceTypes) { throw 'Missing icon, icon group or version resource' }
if (-not ('DownloadManagerPackageIcons' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DownloadManagerPackageIcons {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] public static extern bool FreeLibrary(IntPtr module);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr LoadImageW(IntPtr module, IntPtr name, uint type, int x, int y, uint flags);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr icon);
}
'@
}
# Read resources as data: this never executes the packaged application.
$module = [DownloadManagerPackageIcons]::LoadLibraryExW([IO.Path]::GetFullPath($Path), [IntPtr]::Zero, 0x22)
if ($module -eq [IntPtr]::Zero) { throw 'Windows could not load the packaged icon resources' }
$iconSizes = @(16,20,24,32,48,64,256)
try {
    foreach ($size in $iconSizes) {
        $icon = [DownloadManagerPackageIcons]::LoadImageW($module, [IntPtr]1, 1, $size, $size, 0)
        if ($icon -eq [IntPtr]::Zero) { throw "Windows could not decode packaged icon at $size pixels" }
        [void][DownloadManagerPackageIcons]::DestroyIcon($icon)
    }
} finally { [void][DownloadManagerPackageIcons]::FreeLibrary($module) }
[PSCustomObject]@{ Path = $Path; Bytes = $bytes.Length; FileVersion = $versionInfo.FileVersion; ProductVersion = $versionInfo.ProductVersion; OriginalFilename = $versionInfo.OriginalFilename; Machine = 'x64'; Subsystem = 'Windows GUI'; IconResources = $true; IconDecodedSizes = $iconSizes; Imports = @($imports | Sort-Object -Unique) } | ConvertTo-Json -Depth 3

