param([switch]$Offline, [switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$target = 'x86_64-pc-windows-msvc'
$previousFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
$previousTargetDir = $env:CARGO_TARGET_DIR
$previousJobs = $env:CARGO_BUILD_JOBS
Push-Location $repo
try {
    if ($env:OS -ne 'Windows_NT') { throw 'Packaging requires Windows and the MSVC Rust target' }
    $env:CARGO_BUILD_JOBS = '2'
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    $cargoOptions = @('--locked')
    if ($Offline) { $cargoOptions += '--offline' }
    if (-not $SkipChecks) {
        & (Join-Path $PSScriptRoot 'check.ps1') -Offline:$Offline
    }
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "$previousFlags -C target-feature=+crt-static".Trim()
    & cargo build @cargoOptions --release --target $target --bin download-manager -j 2
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    . (Join-Path $PSScriptRoot 'project-info.ps1')
    $package = Get-DownloadManagerPackage $repo
    $assetName = $package.AssetName
    $dist = Join-Path $repo 'dist'
    New-Item -ItemType Directory -Force $dist | Out-Null
    $asset = Join-Path $dist $assetName
    Copy-Item -LiteralPath (Join-Path $repo "target/$target/release/download-manager.exe") -Destination $asset -Force
    & (Join-Path $PSScriptRoot 'inspect-package.ps1') -Path $asset -ExpectedVersion $package.Version
    $digest = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS.txt'), "$digest  $assetName`n", [System.Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination (Join-Path $dist 'LICENSE.txt') -Force
    Copy-Item -LiteralPath (Join-Path $repo 'docs/RELEASE_NOTES.md') -Destination $dist -Force
    & (Join-Path $PSScriptRoot 'third-party-notices.ps1') -Offline:$Offline
    Write-Output "Prepared $asset"
    Write-Output "SHA-256: $digest"
    Write-Output 'Nothing was published. Manual GUI, network and updater validation is still required.'
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousFlags
    $env:CARGO_TARGET_DIR = $previousTargetDir
    $env:CARGO_BUILD_JOBS = $previousJobs
    Pop-Location
}

