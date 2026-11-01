param([switch]$Diagnose)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'project-info.ps1')
$package = Get-DownloadManagerPackage $repo
$fixtures = Join-Path $repo ".local/gui-smoke/localization-v$($package.Version)"
$screenshots = Join-Path $repo 'docs/screenshots'
$verification = Join-Path $repo 'docs/verification'
$previousData = $env:DOWNLOAD_MANAGER_DATA_DIR
New-Item -ItemType Directory -Force $fixtures, $screenshots, $verification | Out-Null

function Invoke-LocalizedReview([string]$Locale, [string]$Theme, [string]$View, [string]$Name, [string[]]$Size = @()) {
    $fixture = Join-Path $fixtures "$Locale-$($Theme.ToLowerInvariant())-$View"
    $downloads = Join-Path $fixture 'Downloads'
    New-Item -ItemType Directory -Force $fixture, $downloads | Out-Null
    $state = @{
        schema = 1
        next_id = 3
        settings = @{ language=$Locale; theme=$Theme; concurrency=2; bandwidth_kib=0; startup_update_check=$false; close_to_tray=$true; default_directory=$downloads }
        downloads = @(
            @{id=1; url='https://example.test/reference-files.zip'; destination=(Join-Path $downloads 'reference-files.zip'); status='Paused'; bytes=40265318; total=268435456; error=$null},
            @{id=2; url='https://example.test/catalog.zip'; destination=(Join-Path $downloads 'catalog.zip'); status='Failed'; bytes=1310720; total=5242880; error='The remote file changed. Your partial data is preserved. Choose Restart from zero to download the new file.'}
        )
    }
    [IO.File]::WriteAllText((Join-Path $fixture 'state.json'), ($state | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    $env:DOWNLOAD_MANAGER_DATA_DIR = $fixture
    $mode = if ($Diagnose) { '--diagnose-ui' } else { '--capture-ui' }
    $output = if ($Diagnose) { Join-Path $verification "runtime-$Name-v$($package.Version).json" } else { Join-Path $screenshots "$Name.png" }
    $log = Join-Path $fixture 'stderr.log'
    $arguments = @($mode, $View, $output) + $Size
    # This renderer needs its own short-lived native window. No OS input is injected.
    $process = Start-Process -FilePath $package.AssetPath -ArgumentList $arguments -WorkingDirectory $repo -WindowStyle Normal -RedirectStandardError $log -PassThru
    $timeout = if ($Diagnose) { 35000 } else { 8000 }
    if (-not $process.WaitForExit($timeout)) {
        Stop-Process -Id $process.Id -Force
        throw "Owned review process timed out: $Locale $Theme $View"
    }
    if ($process.ExitCode -ne 0) { throw "Review failed: $(Get-Content -LiteralPath $log -Raw)" }
    if (-not (Test-Path -LiteralPath $output)) { throw "No review output: $output" }
    if ($Diagnose) {
        $report = Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
        if (-not $report.complete -or -not $report.idle_hidden_render_gate) { throw "Rendering gate failed: $output" }
    }
    Write-Output "Verified process exit: $Locale $Theme $View -> $output"
}

try {
    if ($Diagnose) {
        Invoke-LocalizedReview 'ru' 'Dark' 'main' 'localization-main-ru'
        Invoke-LocalizedReview 'de' 'Light' 'settings' 'localization-settings-de-light'
    } else {
        Invoke-LocalizedReview 'en' 'Dark' 'main' 'main-window'
        Invoke-LocalizedReview 'en' 'Light' 'main' 'main-window-light'
        Invoke-LocalizedReview 'en' 'Dark' 'settings' 'settings'
        Invoke-LocalizedReview 'en' 'Light' 'settings' 'settings-light'
        Invoke-LocalizedReview 'ru' 'Dark' 'main' 'main-window-ru'
        Invoke-LocalizedReview 'de' 'Light' 'settings' 'settings-de-light' @('560','440')
        Invoke-LocalizedReview 'fr' 'Light' 'new' 'new-download-fr-light'
        Invoke-LocalizedReview 'es' 'Dark' 'popup' 'tray-popup-es'
        Invoke-LocalizedReview 'it' 'Light' 'main' 'main-window-it-light' @('760','540')
    }
} finally {
    $env:DOWNLOAD_MANAGER_DATA_DIR = $previousData
}
