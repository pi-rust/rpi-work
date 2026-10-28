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
$metadataText = & cargo metadata --locked --format-version 1 --no-deps --manifest-path (Join-Path $repoRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
$metadata = $metadataText | ConvertFrom-Json
$version = ($metadata.packages | Where-Object name -EQ 'work-ui' | Select-Object -First 1).version
$semver = [version]($version -split '-')[0]
if ($semver.Minor -ge 1000 -or $semver.Build -ge 1000) { throw 'Android version components must be below 1000.' }
$versionCode = $semver.Major * 1000000 + $semver.Minor * 1000 + $semver.Build
if ($versionCode -le 0 -or $versionCode -gt 2100000000) { throw 'Android versionCode is out of range.' }
Push-Location (Join-Path $repoRoot 'apps/work-ui')
try {
    & $DxPath build --android --release --target $Target --cargo-args=--locked
    if ($LASTEXITCODE -ne 0) { throw 'Dioxus Android APK build failed.' }
} finally { Pop-Location }
# Dioxus 0.7.10 hardcodes versionCode=1. Rebuild the generated Gradle project
# with a monotonic code, a release package and our persistent signing certificate.
$gradleRoot = Join-Path $metadata.target_directory 'dx/work-ui/release/android/app'
$gradleFile = Join-Path $gradleRoot 'app/build.gradle.kts'
$gradle = [IO.File]::ReadAllText($gradleFile)
if ([regex]::Matches($gradle, 'versionCode\s*=\s*\d+').Count -ne 1) { throw 'Unexpected Dioxus Gradle versionCode template.' }
$gradle = [regex]::Replace($gradle, 'versionCode\s*=\s*\d+', "versionCode = $versionCode")
[IO.File]::WriteAllText($gradleFile, $gradle, [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText((Join-Path $gradleRoot 'app/src/main/res/values/strings.xml'), '<resources><string name="app_name">RPI Work</string></resources>', [Text.UTF8Encoding]::new($false))
Push-Location $gradleRoot
try {
    $wrapper = if ($IsWindows) { './gradlew.bat' } else { './gradlew' }
    if (-not $IsWindows) { & chmod +x $wrapper; if ($LASTEXITCODE) { throw 'Cannot make Gradle wrapper executable.' } }
    & $wrapper assembleRelease --no-daemon
    if ($LASTEXITCODE -ne 0) { throw 'Android release package failed.' }
} finally { Pop-Location }
$unsigned = Join-Path $gradleRoot 'app/build/outputs/apk/release/app-release-unsigned.apk'
if (-not (Test-Path -LiteralPath $unsigned)) { throw 'Unsigned release APK is missing.' }
$buildTools = Get-ChildItem -LiteralPath (Join-Path $sdkRoot 'build-tools') -Directory | Where-Object Name -Match '^\d+\.\d+\.\d+$' | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $buildTools) { throw 'Android SDK build-tools are missing.' }
$signerName = if ($IsWindows) { 'apksigner.bat' } else { 'apksigner' }
$fixedSigning = [bool]$env:ANDROID_KEYSTORE
if (-not $fixedSigning -and $env:GITHUB_REF -like 'refs/tags/v*') { throw 'Tagged releases require the persistent Android signing key.' }
if ($fixedSigning) {
    if (-not $env:ANDROID_KEYSTORE_PASSWORD) { throw 'ANDROID_KEYSTORE_PASSWORD is missing.' }
    $keystore = $env:ANDROID_KEYSTORE
    $alias = 'rpi-work'
} else {
    $keystore = Join-Path $repoRoot 'target/android-development.p12'
    $alias = 'androiddebugkey'
    $env:ANDROID_KEYSTORE_PASSWORD = 'android'
    if (-not (Test-Path -LiteralPath $keystore)) {
        & (Join-Path $env:JAVA_HOME 'bin/keytool') -genkeypair -keystore $keystore -storetype PKCS12 -storepass:env ANDROID_KEYSTORE_PASSWORD -keypass:env ANDROID_KEYSTORE_PASSWORD -alias $alias -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=Android Development,O=RPI Work,C=CN'
        if ($LASTEXITCODE) { throw 'Development key generation failed.' }
    }
}
$signed = Join-Path $outDir 'signed.apk'
& (Join-Path $buildTools.FullName $signerName) sign --ks $keystore --ks-key-alias $alias --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEYSTORE_PASSWORD --out $signed $unsigned
if ($LASTEXITCODE) { throw 'APK signing failed.' }
& (Join-Path $buildTools.FullName $signerName) verify --verbose --print-certs $signed
if ($LASTEXITCODE -ne 0) { throw 'The APK signature is missing or invalid.' }
$aaptName = if ($IsWindows) { 'aapt.exe' } else { 'aapt' }
$apkManifest = & (Join-Path $buildTools.FullName $aaptName) dump xmltree $signed AndroidManifest.xml
if ($LASTEXITCODE -ne 0 -or ($apkManifest -join "`n") -notmatch 'com\.pirust\.rpiwork\.updates' -or ($apkManifest -join "`n") -notmatch 'android.permission.REQUEST_INSTALL_PACKAGES') {
    throw 'The APK must declare its update provider and installation permission.'
}
if (($apkManifest -join "`n") -notmatch 'android:windowSoftInputMode\([^\r\n]*\)=\(type 0x11\)0x10\b') {
    throw 'The APK must resize its window for the keyboard (adjustResize).'
}
$dist = Join-Path $repoRoot 'target/ci-dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$suffix = if ($fixedSigning) { '' } else { '-dev' }
$destination = Join-Path $dist "rpi-work-$version-$Target$suffix.apk"
Copy-Item -LiteralPath $signed -Destination $destination -Force
$digest = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$destination.sha256", "$digest  $([IO.Path]::GetFileName($destination))`n", [Text.UTF8Encoding]::new($false))
Write-Output "Installable release-optimized APK: $destination (versionCode $versionCode; persistent signing: $fixedSigning)"
