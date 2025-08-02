function Get-DownloadManagerPackage([string]$RepositoryRoot) {
    $manifest = Join-Path $RepositoryRoot 'Cargo.toml'
    $metadataText = & cargo metadata --manifest-path $manifest --locked --offline --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the locked project version' }
    $package = ($metadataText | ConvertFrom-Json).packages | Where-Object { $_.name -eq 'download-manager' }
    if (-not $package.version) { throw 'DownloadManager package is missing from Cargo metadata' }
    $assetName = "DownloadManager-v$($package.version).exe"
    [PSCustomObject]@{
        Version = $package.version
        AssetName = $assetName
        AssetPath = Join-Path $RepositoryRoot "dist/$assetName"
    }
}
