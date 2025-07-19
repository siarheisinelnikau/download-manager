param(
    [switch]$Diagnose,
    [ValidateSet('Dark','Light')][string]$Theme = 'Dark',
    [switch]$SkipTray
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$fixture = Join-Path $repo ('.local/gui-smoke/design/' + $Theme.ToLowerInvariant())
$suffix = if ($Theme -eq 'Light') { '-light' } else { '' }
$downloads = Join-Path $fixture 'downloads'
$screenshots = Join-Path $repo 'docs/screenshots'
$previousData = $env:DOWNLOAD_MANAGER_DATA_DIR
Push-Location $repo
try {
    New-Item -ItemType Directory -Force $fixture, $downloads, $screenshots | Out-Null
    $settings = @{concurrency=2; bandwidth_kib=0; startup_update_check=$false; default_directory=$downloads}
    # Omission also verifies the dark default for existing settings without a theme.
    if ($Theme -eq 'Light') { $settings.theme = 'Light' }
    foreach ($scenario in @('empty', 'populated')) {
        $stateFolder = Join-Path $fixture $scenario
        New-Item -ItemType Directory -Force $stateFolder | Out-Null
        $jobs = @()
        if ($scenario -eq 'populated') {
            $jobs = @(
                @{id=1; url='https://example.test/reference-files.zip'; destination=(Join-Path $downloads 'reference-files.zip'); status='Paused'; bytes=40265318; total=268435456; error=$null},
                @{id=2; url='https://example.test/catalog-assets.zip'; destination=(Join-Path $downloads 'product-photography-and-catalog-assets-2026-10.zip'); status='Failed'; bytes=1310720; total=5242880; error='The remote file changed. Your partial data is preserved. Choose Restart from zero to download the new file.'}
            )
        }
        $state = @{schema=1; next_id=3; settings=$settings; downloads=$jobs}
        [System.IO.File]::WriteAllText((Join-Path $stateFolder 'state.json'), ($state | ConvertTo-Json -Depth 8))
    }
    function Invoke-Review([string]$Mode, [string]$View, [string]$Output, [string]$Scenario, [string[]]$Size=@()) {
        $resolvedOutput = [IO.Path]::GetFullPath($Output)
        if (-not $resolvedOutput.StartsWith(($repo.TrimEnd('\') + '\'), [StringComparison]::OrdinalIgnoreCase)) { throw 'Review output must remain inside this repository' }
        if (Test-Path -LiteralPath $resolvedOutput) { Remove-Item -LiteralPath $resolvedOutput -Force }
        $env:DOWNLOAD_MANAGER_DATA_DIR = Join-Path $fixture $Scenario
        $arguments = @($Mode, $View, $Output) + $Size
        $log = Join-Path $fixture (([IO.Path]::GetFileName($Output)) + '.stderr.log')
        # GDI self-capture needs a real native window; it closes itself after saving.
        $process = Start-Process -FilePath (Join-Path $repo 'dist/DownloadManager-v0.1.0.exe') -ArgumentList $arguments -WorkingDirectory $repo -WindowStyle Normal -RedirectStandardError $log -PassThru
        $timeout = if ($Mode -eq '--diagnose-ui') { 35000 } else { 8000 }
        if (-not $process.WaitForExit($timeout)) { Stop-Process -Id $process.Id -Force; throw "Owned diagnostic timed out: $View $Scenario" }
        if ($process.ExitCode -ne 0) { throw "Review failed: $(Get-Content -LiteralPath $log -Raw)" }
        if (-not (Test-Path -LiteralPath $Output)) { throw "No output: $Output" }
        if ($Mode -eq '--diagnose-ui') {
            $report = Get-Content -LiteralPath $Output -Raw | ConvertFrom-Json
            if (-not $report.complete -or -not $report.idle_hidden_render_gate) { throw "Rendering gate failed: $Output" }
            if ($View -eq 'tray') {
                if ((Get-Content -LiteralPath $log -Raw) -match 'Error removing system tray icon') { throw 'Native tray icon removal failed' }
                $registration = $report.samples | Where-Object { $_.state -eq 'tray_registration' }
                if (-not $registration.shell_icon_rectangle) { throw 'The Windows Shell did not report a registered tray icon' }
                $cleanup = $report.samples | Where-Object { $_.state -eq 'tray_cleanup' }
                if (-not $cleanup.tray_created -or -not $cleanup.tray_native_window_destroyed -or -not $cleanup.quit_requested_while_hidden -or $cleanup.duplicate_activation_exit -ne 0) { throw "Tray lifecycle failed: $Output" }
            }
        }
        Write-Output "Verified exit: $View $Scenario -> $Output"
    }
    if ($Diagnose) {
        Invoke-Review '--diagnose-ui' 'main' (Join-Path $fixture 'runtime-main.json') 'populated'
        Invoke-Review '--diagnose-ui' 'settings' (Join-Path $fixture 'runtime-settings.json') 'empty'
        Invoke-Review '--diagnose-ui' 'popup' (Join-Path $fixture 'runtime-popup.json') 'populated'
        if (-not $SkipTray) { Invoke-Review '--diagnose-ui' 'tray' (Join-Path $fixture 'runtime-tray.json') 'populated' }
    } else {
        Invoke-Review '--capture-ui' 'main' (Join-Path $screenshots "main-window$suffix.png") 'populated'
        Invoke-Review '--capture-ui' 'settings' (Join-Path $screenshots "settings$suffix.png") 'empty'
        Invoke-Review '--capture-ui' 'popup' (Join-Path $screenshots "tray-popup$suffix.png") 'populated'
        Invoke-Review '--capture-ui' 'main' (Join-Path $fixture 'empty.png') 'empty'
        Invoke-Review '--capture-ui' 'settings' (Join-Path $fixture 'settings-small.png') 'empty' @('760','540')
    }
    if (-not $Diagnose) {
        $iconProcess = Start-Process -FilePath (Join-Path $repo 'dist/DownloadManager-v0.1.0.exe') -ArgumentList @('--export-icon', (Join-Path $screenshots 'application-icon.png')) -WindowStyle Hidden -PassThru
        if (-not $iconProcess.WaitForExit(8000)) { Stop-Process -Id $iconProcess.Id -Force; throw 'Owned icon export timed out' }
        if ($iconProcess.ExitCode -ne 0) { throw 'Icon export failed' }
    }
} finally {
    $env:DOWNLOAD_MANAGER_DATA_DIR = $previousData
    Pop-Location
}
