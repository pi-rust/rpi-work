$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $root 'scripts/ci/android-brand.ps1')
function Assert-True($Value, [string]$Message) { if (-not $Value) { throw $Message } }
$fixture = Join-Path $root ('target/android-brand-tests/' + [guid]::NewGuid().ToString('N'))
$brand = Join-Path $fixture 'brand'
$res = Join-Path $fixture 'app/src/main/res'
try {
    New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Copy-Item -LiteralPath (Join-Path $root 'assets/branding') -Destination $brand -Recurse
    foreach ($directory in @('values','mipmap-mdpi','mipmap-anydpi-v26','drawable','drawable-v24')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $res $directory) | Out-Null
    }
    [IO.File]::WriteAllText((Join-Path $res '../AndroidManifest.xml'), '<manifest/>')
    [IO.File]::WriteAllText((Join-Path $res 'values/strings.xml'), '<resources/>')
    [IO.File]::WriteAllText((Join-Path $res 'drawable/keep.xml'), '<shape/>')
    foreach ($file in @('mipmap-mdpi/ic_launcher.webp','mipmap-anydpi-v26/ic_launcher.xml','drawable/ic_launcher_background.xml','drawable-v24/ic_launcher_foreground.xml')) {
        [IO.File]::WriteAllText((Join-Path $res $file), 'Dioxus default fixture, not a brand')
    }
    Set-WorkAndroidBrand -ResRoot $res -BrandRoot $brand
    Set-WorkAndroidBrand -ResRoot $res -BrandRoot $brand
    Assert-True (-not (Test-Path (Join-Path $res 'mipmap-mdpi/ic_launcher.webp'))) 'Conflicting default WebP survived.'
    Assert-True (-not (Test-Path (Join-Path $res 'drawable-v24/ic_launcher_foreground.xml'))) 'Dioxus default foreground survived.'
    Assert-True ([IO.File]::ReadAllText((Join-Path $res 'drawable/keep.xml')) -eq '<shape/>') 'Non-icon resource changed.'
    Assert-True ([IO.File]::ReadAllText((Join-Path $res 'values/strings.xml')) -eq '<resources/>') 'App label changed.'
    $metadata = Get-Content -Raw (Join-Path $brand 'android-icons.json') | ConvertFrom-Json
    foreach ($file in $metadata.hashes.PSObject.Properties) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $res $file.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
        Assert-True ($hash -eq $file.Value) "Incorrect copied resource: $($file.Name)"
    }
    # Simulate Dioxus's next generation step after a completed branded build.
    Clear-WorkAndroidLegacyBrand -ResRoot $res
    Clear-WorkAndroidLegacyBrand -ResRoot $res
    foreach ($density in @('mdpi','hdpi','xhdpi','xxhdpi','xxxhdpi')) {
        Assert-True (-not (Test-Path (Join-Path $res "mipmap-$density/ic_launcher.png"))) 'Prior branded PNG would conflict with regenerated WebP.'
    }
    Assert-True (Test-Path (Join-Path $res 'drawable-nodpi/rpi_launcher_foreground.png')) 'Pre-build cleanup removed an unrelated foreground.'
    Assert-True ([IO.File]::ReadAllText((Join-Path $res 'drawable/keep.xml')) -eq '<shape/>') 'Pre-build cleanup changed a non-icon resource.'
    [IO.File]::WriteAllText((Join-Path $res 'mipmap-mdpi/ic_launcher.webp'), 'regenerated Dioxus default')
    Set-WorkAndroidBrand -ResRoot $res -BrandRoot $brand
    Assert-True (-not (Test-Path (Join-Path $res 'mipmap-mdpi/ic_launcher.webp'))) 'Repeated build left duplicate launchers.'
    [IO.File]::AppendAllText((Join-Path $brand 'android/mipmap-mdpi/ic_launcher.png'), 'corruption')
    $failed = $false
    try { Set-WorkAndroidBrand -ResRoot $res -BrandRoot $brand } catch { $failed = $true }
    Assert-True $failed 'Stale/corrupt brand resource accepted.'
    Assert-True (Test-Path (Join-Path $res 'mipmap-mdpi/ic_launcher.png')) 'Validation failure mutated destination.'
    Write-Output 'PASS official Android resource overlay, duplicate cleanup, idempotence, non-icon preservation and corrupt source rejection'
} finally {
    if (Test-Path $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
