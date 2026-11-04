param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    $options = @('--locked', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc')
    if ($Offline) { $options += '--offline' }
    $metadataText = & cargo metadata @options
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve third-party license metadata' }
    $metadata = $metadataText | ConvertFrom-Json
    # Include every resolved package as a conservative superset, including build dependencies.
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('DownloadManager third-party notices')
    $lines.Add('Generated from Cargo.lock and the local crate sources. Includes build-time dependencies.')
    foreach ($package in ($metadata.packages | Where-Object { $_.name -ne 'download-manager' } | Sort-Object name, version)) {
        $lines.Add("`n===== $($package.name) $($package.version) =====")
        $lines.Add("License expression: $($package.license)")
        $lines.Add("Source: $($package.repository)")
        $root = Split-Path -Parent $package.manifest_path
        $licenses = Get-ChildItem -LiteralPath $root -File -Recurse | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|OFL|UFL)([._-]|$)|license.*\.(txt|md)$' }
        if ($package.name -eq 'rust-auto-updator') {
            $workspaceLicense = Join-Path (Split-Path -Parent (Split-Path -Parent $root)) 'LICENSE'
            if (Test-Path -LiteralPath $workspaceLicense -PathType Leaf) { $licenses = @($licenses) + (Get-Item -LiteralPath $workspaceLicense) }
        }
        if ($package.license_file) {
            $licensePath = Join-Path $root $package.license_file
            if (Test-Path -LiteralPath $licensePath -PathType Leaf) { $licenses = @($licenses) + (Get-Item -LiteralPath $licensePath) }
        }
        foreach ($license in ($licenses | Sort-Object FullName -Unique)) {
            $lines.Add("--- $($license.Name) ---")
            $lines.Add([System.IO.File]::ReadAllText($license.FullName))
        }
        if (-not $licenses) { $lines.Add('No license text bundled by this crate; review its declared license and source before publication.') }
    }
    $dist = Join-Path $repo 'dist'
    New-Item -ItemType Directory -Force $dist | Out-Null
    [System.IO.File]::WriteAllLines((Join-Path $dist 'THIRD_PARTY_NOTICES.txt'), $lines, [System.Text.UTF8Encoding]::new($false))
} finally { Pop-Location }

