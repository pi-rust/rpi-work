param([string]$BindgenPath = '', [switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  $sourcePaths = @('Cargo.toml', 'Cargo.lock') + @(Get-ChildItem -Path (Join-Path $repoRoot 'apps'),(Join-Path $repoRoot 'crates') -Recurse -File | Where-Object { $_.Extension -in '.rs','.css','.toml' } | ForEach-Object { $_.FullName })
  function Read-SourceHashes {
    $hashes = [ordered]@{}
    foreach ($sourcePath in ($sourcePaths | Sort-Object)) {
      $absoluteSource = (Resolve-Path -LiteralPath $sourcePath).Path
      $relativeSource = $absoluteSource.Substring($repoRoot.Length + 1).Replace('\','/')
      $hashes[$relativeSource] = (Get-FileHash -LiteralPath $absoluteSource -Algorithm SHA256).Hash
    }
    return $hashes
  }
  $sourceHashesBefore = Read-SourceHashes
  . (Join-Path $PSScriptRoot 'ci/wasm-bindgen-version.ps1')
  $bindgenVersion = Get-LockedWasmBindgenVersion -RepositoryRoot $repoRoot
  if (-not $BindgenPath) {
    $cachedBindgen = Join-Path ([System.IO.Path]::GetTempPath()) "rpi-work-bindgen/wasm-bindgen-$bindgenVersion-x86_64-pc-windows-msvc/wasm-bindgen.exe"
    if (Test-Path -LiteralPath $cachedBindgen) { $BindgenPath = $cachedBindgen }
    else { $BindgenPath = (Get-Command wasm-bindgen -ErrorAction Stop).Source }
  }
  $reportedBindgenVersion = & $BindgenPath --version
  if ($LASTEXITCODE -ne 0 -or "$reportedBindgenVersion".Trim() -ne "wasm-bindgen $bindgenVersion") {
    throw "wasm-bindgen CLI must match Cargo.lock ($bindgenVersion). Run scripts/ci/install-wasm-bindgen.ps1."
  }
  $buildArguments = @('build', '--locked', '--target', 'wasm32-unknown-unknown', '-p', 'work-ui')
  $buildProfile = 'debug'
  if (-not $DebugBuild) { $buildArguments += '--release'; $buildProfile = 'release' }
  & cargo @buildArguments
  if ($LASTEXITCODE -ne 0) { throw 'work-ui WASM build failed' }
  $metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
  if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
  $outputDirectory = Join-Path $repoRoot 'target/parity-web'
  New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
  $wasmPath = Join-Path $metadata.target_directory "wasm32-unknown-unknown/$buildProfile/work-ui.wasm"
  & $BindgenPath $wasmPath --target web --out-dir $outputDirectory --out-name work-ui
  if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen failed' }
  $sourceHashesAfter = Read-SourceHashes
  if (($sourceHashesBefore | ConvertTo-Json -Compress) -ne ($sourceHashesAfter | ConvertTo-Json -Compress)) { throw 'Sources changed while building; rebuild after the current edit completes.' }
  [ordered]@{ profile=$buildProfile; builtAt=[DateTime]::UtcNow.ToString('o'); sourceHashes=$sourceHashesAfter; wasmSha256=(Get-FileHash -LiteralPath (Join-Path $outputDirectory 'work-ui_bg.wasm') -Algorithm SHA256).Hash } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputDirectory 'build.json') -Encoding utf8
  @'
<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>RPI Work — Dioxus review</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from './work-ui.js'; await init();</script></body></html>
'@ | Set-Content -LiteralPath (Join-Path $outputDirectory 'index.html') -Encoding utf8
  # rpi-web already serves a custom index plus /assets. Package the same real
  # Dioxus build for that supported deployment path; no API proxy or prototype JS.
  $serverDirectory = Join-Path $repoRoot 'target/parity-web-server'
  $serverAssetsDirectory = Join-Path $serverDirectory 'assets'
  New-Item -ItemType Directory -Force -Path $serverAssetsDirectory | Out-Null
  foreach ($asset in (Get-ChildItem -LiteralPath $outputDirectory)) {
    if ($asset.Name -in 'index.html','build.json') { continue }
    Copy-Item -LiteralPath $asset.FullName -Destination $serverAssetsDirectory -Recurse -Force
  }
  Copy-Item -LiteralPath (Join-Path $outputDirectory 'build.json') -Destination (Join-Path $serverDirectory 'build.json') -Force
  @'
<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>RPI Work</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from '/assets/work-ui.js'; await init();</script></body></html>
'@ | Set-Content -LiteralPath (Join-Path $serverDirectory 'index.html') -Encoding utf8
  Write-Output 'Review URL: http://127.0.0.1:4173/target/parity-web/'
  Write-Output "rpi-web --assets directory: $serverDirectory"
} finally { Pop-Location }
