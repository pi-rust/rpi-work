# Dioxus 0.7.10 writes its default Android launcher after reading bundle.icon.
# On a repeated build it adds WebP beside our prior PNG before our final overlay
# can run. Remove only those five generated PNGs before invoking Dioxus again.
function Clear-WorkAndroidLegacyBrand {
    param([Parameter(Mandatory = $true)][string]$ResRoot)
    if (-not (Test-Path -LiteralPath $ResRoot)) { return }
    if (-not (Test-Path -LiteralPath (Join-Path $ResRoot '../AndroidManifest.xml'))) {
        throw 'Android launcher cleanup requires an already-generated app resource directory.'
    }
    foreach ($density in @('mdpi','hdpi','xhdpi','xxhdpi','xxxhdpi')) {
        $file = Join-Path $ResRoot "mipmap-$density/ic_launcher.png"
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
    }
}

# Overlay the canonical local brand before the final Gradle release package.
function Set-WorkAndroidBrand {
    param(
        [Parameter(Mandatory = $true)][string]$ResRoot,
        [string]$BrandRoot = (Join-Path $PSScriptRoot '../../assets/branding')
    )
    $ErrorActionPreference = 'Stop'
    if (-not (Test-Path -LiteralPath (Join-Path $ResRoot '../AndroidManifest.xml')) -or
        -not (Test-Path -LiteralPath (Join-Path $ResRoot 'values/strings.xml'))) {
        throw 'Android brand overlay requires an already-generated app resource directory.'
    }
    $metadata = Get-Content -Raw -LiteralPath (Join-Path $BrandRoot 'android-icons.json') | ConvertFrom-Json
    $sourceHash = (Get-FileHash -LiteralPath (Join-Path $BrandRoot 'rpi.svg') -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($metadata.source -ne 'rpi.svg' -or $metadata.sourceSha256 -ne $sourceHash) {
        throw 'Android brand source changed; run tools/dev/generate-brand-icons.cjs.'
    }
    # Fixed paths, not arbitrary metadata-controlled copy/delete targets.
    $files = @('mipmap-mdpi/ic_launcher.png','mipmap-hdpi/ic_launcher.png',
        'mipmap-xhdpi/ic_launcher.png','mipmap-xxhdpi/ic_launcher.png','mipmap-xxxhdpi/ic_launcher.png',
        'drawable-nodpi/rpi_launcher_foreground.png','values/rpi_launcher.xml','mipmap-anydpi-v26/ic_launcher.xml')
    if ($metadata.hashes.PSObject.Properties.Name.Count -ne $files.Count) { throw 'Unexpected Android brand resource catalog.' }
    foreach ($file in $files) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $BrandRoot "android/$file") -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($metadata.hashes.$file -ne $hash) { throw "Stale Android brand resource: $file" }
    }
    # Validate all sources before mutating generated output. Remove conflicting
    # PNG/WebP/XML default launcher variants, leaving every non-icon resource.
    Get-ChildItem -LiteralPath $ResRoot -Recurse -File | Where-Object {
        $_.BaseName -in @('ic_launcher','ic_launcher_round','ic_launcher_background','ic_launcher_foreground')
    } | Remove-Item -Force
    foreach ($file in $files) {
        $destination = Join-Path $ResRoot $file
        New-Item -ItemType Directory -Force -Path (Split-Path $destination -Parent) | Out-Null
        Copy-Item -LiteralPath (Join-Path $BrandRoot "android/$file") -Destination $destination -Force
    }
    Write-Output 'Official rpi Android brand applied: five legacy densities and adaptive safe-zone foreground.'
}
