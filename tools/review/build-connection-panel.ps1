$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repoRoot
try {
  $manifest = Join-Path $repoRoot 'tests/harness/ui-modules/Cargo.toml'
  $buildDirectory = Join-Path $repoRoot 'target/parity-connection-panel-build'
  $outputDirectory = Join-Path $repoRoot 'target/parity-connection-panel-web'
  . (Join-Path $repoRoot 'scripts/ci/wasm-bindgen-version.ps1')
  $version = Get-LockedWasmBindgenVersion -RepositoryRoot $repoRoot
  $harnessLock = Get-Content -LiteralPath (Join-Path $repoRoot 'tests/harness/ui-modules/Cargo.lock') -Raw
  $harnessVersions = @([regex]::Matches($harnessLock, '(?m)^name = "wasm-bindgen"\r?\nversion = "([^"\r\n]+)"') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
  if ($harnessVersions.Count -ne 1 -or $harnessVersions[0] -ne $version) { throw 'Harness and repository wasm-bindgen locks must agree' }
  $bindgen = Join-Path $env:TEMP "rpi-work-bindgen/wasm-bindgen-$version-x86_64-pc-windows-msvc/wasm-bindgen.exe"
  if (-not (Test-Path -LiteralPath $bindgen)) { $bindgen = (Get-Command wasm-bindgen -ErrorAction Stop).Source }
  $actualVersion = & $bindgen --version
  if ($LASTEXITCODE -ne 0 -or $actualVersion.Trim() -ne "wasm-bindgen $version") { throw "Install the locked wasm-bindgen CLI $version; found $actualVersion" }
  & cargo build --locked --manifest-path $manifest --bin connection-panel --no-default-features --target wasm32-unknown-unknown --target-dir $buildDirectory --release
  if ($LASTEXITCODE -ne 0) { throw 'Connection panel review build failed' }
  New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
  & $bindgen (Join-Path $buildDirectory 'wasm32-unknown-unknown/release/connection-panel.wasm') --target web --out-dir $outputDirectory --out-name work-ui
  if ($LASTEXITCODE -ne 0) { throw 'Connection panel review bindgen failed' }
  @'
<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Test-only shared connection panel</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from './work-ui.js'; await init();</script></body></html>
'@ | Set-Content -LiteralPath (Join-Path $outputDirectory 'index.html') -Encoding utf8
  Write-Output "Test-only bundle: $outputDirectory"
  Write-Output 'Run node tests/e2e/verify-connection-panel.cjs (owns its random-port static server). Synthetic callbacks only, not native networking.'
} finally { Pop-Location }
