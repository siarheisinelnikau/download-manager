param([switch]$Offline, [switch]$Loopback)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previousJobs = $env:CARGO_BUILD_JOBS
$previousTargetDir = $env:CARGO_TARGET_DIR
Push-Location $repo
try {
    $env:CARGO_BUILD_JOBS = '2'
    $env:CARGO_TARGET_DIR = Join-Path $repo 'target'
    $cargoOptions = @('--locked')
    if ($Offline) { $cargoOptions += '--offline' }
    $features = @()
    if ($Loopback) { $features = @('--features', 'loopback-tests') }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed' }
    & cargo clippy @cargoOptions --all-targets @features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
    & cargo test @cargoOptions --all-targets @features -- --test-threads=2
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
} finally {
    $env:CARGO_BUILD_JOBS = $previousJobs
    $env:CARGO_TARGET_DIR = $previousTargetDir
    Pop-Location
}

