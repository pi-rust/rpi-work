param(
    [ValidateSet('aarch64-linux-android', 'x86_64-linux-android')]
    [string]$Target = 'aarch64-linux-android',
    [string]$DxPath = ''
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
if (-not $DxPath) { $DxPath = (Get-Command dx -ErrorAction Stop).Source }
$dxVersion = & $DxPath --version
if ($LASTEXITCODE -ne 0 -or "$dxVersion" -notmatch '^dioxus 0\.7\.10(?:\s|$)') {
    throw 'Android builds require Dioxus CLI 0.7.10, matching the locked Dioxus libraries.'
}
$sdkRoot = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { $env:ANDROID_SDK_ROOT }
if (-not $sdkRoot -or -not (Test-Path -LiteralPath $sdkRoot -PathType Container)) {
    throw 'Set ANDROID_HOME or ANDROID_SDK_ROOT to an installed Android SDK.'
}
$outDir = Join-Path $repoRoot "target/android-bundle/$Target/$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Push-Location (Join-Path $repoRoot 'apps/work-ui')
try {
    & $DxPath bundle --android --release --target $Target --package-types apk --cargo-args=--locked --out-dir $outDir
    if ($LASTEXITCODE -ne 0) { throw 'Dioxus Android APK build failed.' }
} finally { Pop-Location }
# A fresh output directory prevents accidentally selecting a previous build's APK.
$apks = @(Get-ChildItem -LiteralPath $outDir -File -Filter '*.apk' -Recurse)
if ($apks.Count -ne 1) { throw "Expected one newly built APK, found $($apks.Count)." }
$buildTools = Get-ChildItem -LiteralPath (Join-Path $sdkRoot 'build-tools') -Directory | Where-Object Name -Match '^\d+\.\d+\.\d+$' | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $buildTools) { throw 'Android SDK build-tools are missing.' }
$signerName = if ($IsWindows) { 'apksigner.bat' } else { 'apksigner' }
& (Join-Path $buildTools.FullName $signerName) verify --verbose --print-certs $apks[0].FullName
if ($LASTEXITCODE -ne 0) { throw 'The APK signature is missing or invalid.' }
$metadataText = & cargo metadata --locked --format-version 1 --no-deps --manifest-path (Join-Path $repoRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
$metadata = $metadataText | ConvertFrom-Json
$version = ($metadata.packages | Where-Object name -EQ 'work-ui' | Select-Object -First 1).version
$dist = Join-Path $repoRoot 'target/ci-dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$destination = Join-Path $dist "rpi-work-$version-$Target-dev.apk"
Copy-Item -LiteralPath $apks[0].FullName -Destination $destination -Force
$digest = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$destination.sha256", "$digest  $([IO.Path]::GetFileName($destination))`n", [Text.UTF8Encoding]::new($false))
Write-Output "Installable development-signed APK: $destination"
Write-Output 'Rust is release-optimized; the Android package uses the development signing key, not a Play Store release certificate.'
