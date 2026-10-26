param(
    [Parameter(Mandatory)]
    [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc', 'x86_64-apple-darwin', 'aarch64-apple-darwin', 'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu', 'wasm32-unknown-unknown')]
    [string]$Target,
    [ValidateSet('release', 'debug')][string]$Profile = 'release'
)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repoRoot
try {
    $metadataText = & cargo metadata --locked --format-version 1 --no-deps
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -EQ 'work-ui' | Select-Object -First 1).version
    if (-not $version) { throw 'work-ui package version is missing.' }
    $commit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the source commit.' }
    $rustVersion = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the Rust compiler version.' }
    $profileSuffix = if ($Profile -eq 'debug') { '-debug' } else { '' }
    $packageName = "rpi-work-$version-$Target$profileSuffix"
    $stageRoot = Join-Path $repoRoot "target/ci-staging/$([guid]::NewGuid().ToString('N'))"
    $packageRoot = Join-Path $stageRoot $packageName
    $outputRoot = Join-Path $repoRoot 'target/ci-dist'
    New-Item -ItemType Directory -Force -Path $packageRoot, $outputRoot | Out-Null
    $runtimeNote = ''
    if ($Target -eq 'wasm32-unknown-unknown') {
        $standalone = Join-Path $repoRoot 'target/parity-web'
        $server = Join-Path $repoRoot 'target/parity-web-server'
        foreach ($required in @((Join-Path $standalone 'index.html'), (Join-Path $standalone 'work-ui_bg.wasm'), (Join-Path $server 'assets/work-ui_bg.wasm'))) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) { throw "Web build is missing: $required" }
        }
        $webBuild = Get-Content -LiteralPath (Join-Path $standalone 'build.json') -Raw | ConvertFrom-Json
        if ($webBuild.profile -ne $Profile) { throw 'Requested package profile differs from the Web build.' }
        Copy-Item -LiteralPath $standalone -Destination (Join-Path $packageRoot 'standalone') -Recurse
        Copy-Item -LiteralPath $server -Destination (Join-Path $packageRoot 'rpi-web') -Recurse
        $runtimeNote = @'
Browser build (also the current mobile client):
- standalone/: serve with an HTTP server; do not open index.html as file://.
- rpi-web/: pass this directory to a compatible rpi-web --assets <directory>.
Use the same origin for browser assets and the node API, or an explicitly
configured reverse proxy. A static file server alone does not provide the API.
This archive does not contain an Android APK or an iOS IPA.
'@
    } else {
        $binaryRoot = Join-Path $metadata.target_directory "$Target/$Profile"
        $extension = if ($Target -like '*windows*') { '.exe' } else { '' }
        $ui = Join-Path $binaryRoot "work-ui$extension"
        $cli = Join-Path $binaryRoot "work-cli$extension"
        foreach ($binary in @($ui, $cli)) {
            if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Build is missing: $binary" }
        }
        if ($Target -like '*apple-darwin') {
            $contents = Join-Path $packageRoot 'RPI Work.app/Contents'
            $macOs = Join-Path $contents 'MacOS'
            New-Item -ItemType Directory -Force -Path $macOs | Out-Null
            Copy-Item -LiteralPath $ui -Destination (Join-Path $macOs 'work-ui')
            $bundleVersion = ($version -split '-')[0]
            $plist = @"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>RPI Work</string>
<key>CFBundleDisplayName</key><string>RPI Work</string>
<key>CFBundleIdentifier</key><string>com.pi-rust.rpi-work</string>
<key>CFBundleExecutable</key><string>work-ui</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$bundleVersion</string>
<key>CFBundleVersion</key><string>$bundleVersion</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"@
            [IO.File]::WriteAllText((Join-Path $contents 'Info.plist'), $plist, [Text.UTF8Encoding]::new($false))
            & chmod 755 (Join-Path $macOs 'work-ui')
            if ($LASTEXITCODE -ne 0) { throw 'Setting macOS executable permissions failed.' }
            $runtimeNote = 'Unsigned macOS application bundle. Apple signing/notarization is not configured.'
        } else {
            Copy-Item -LiteralPath $ui -Destination $packageRoot
            if ($Target -like '*linux*') {
                & chmod 755 (Join-Path $packageRoot 'work-ui')
                if ($LASTEXITCODE -ne 0) { throw 'Setting Linux executable permissions failed.' }
                $runtimeNote = 'Built on Ubuntu 24.04. Requires compatible glibc, GTK 3, WebKitGTK 4.1 and libxdo runtime libraries.'
            } else {
                $runtimeNote = 'Requires the Microsoft Edge WebView2 Runtime. Windows binaries are unsigned.'
            }
        }
        Copy-Item -LiteralPath $cli -Destination $packageRoot
        if ($Target -notlike '*windows*') {
            & chmod 755 (Join-Path $packageRoot 'work-cli')
            if ($LASTEXITCODE -ne 0) { throw 'Setting CLI executable permissions failed.' }
        }
    }
    $buildInfo = [ordered]@{ version = $version; target = $Target; profile = $Profile; commit = "$commit".Trim(); rustc = "$rustVersion".Trim(); packagedAt = [DateTime]::UtcNow.ToString('o') }
    [IO.File]::WriteAllText((Join-Path $packageRoot 'build-info.json'), ($buildInfo | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    $readme = "RPI Work $version`nTarget: $Target ($Profile)`nCommit: $commit`n`n$runtimeNote`n`nA separately running compatible rpi-http/rpi-web node is required.`nThe node executable and model credentials are not included in this archive.`nSee https://github.com/pi-rust/rpi-work for connection and API documentation.`n"
    [IO.File]::WriteAllText((Join-Path $packageRoot 'README.txt'), $readme, [Text.UTF8Encoding]::new($false))
    if ($Target -like '*windows*' -or $Target -eq 'wasm32-unknown-unknown') {
        $archive = Join-Path $outputRoot "$packageName.zip"
        Compress-Archive -LiteralPath $packageRoot -DestinationPath $archive -Force
    } else {
        $archive = Join-Path $outputRoot "$packageName.tar.gz"
        & tar -czf $archive -C $stageRoot $packageName
        if ($LASTEXITCODE -ne 0) { throw 'Creating the native archive failed.' }
    }
    $checksum = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$archive.sha256", "$checksum  $([IO.Path]::GetFileName($archive))`n", [Text.UTF8Encoding]::new($false))
    Write-Output "Created $archive"
    Write-Output "SHA256 $checksum"
} finally { Pop-Location }
