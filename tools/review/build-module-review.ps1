param([ValidateSet('all','meta','resources','trace','project','extras','conversation','settings','queue','scroll')][string]$Feature = 'all')
$ErrorActionPreference='Stop'
$repoRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repoRoot
try {
  $manifest=Join-Path $repoRoot 'tests/harness/ui-modules/Cargo.toml'
  $buildArguments=@('build','--locked','--manifest-path',$manifest,'--target','wasm32-unknown-unknown','--release')
  if ($Feature -eq 'all') { $buildArguments+=@('--all-features') }
  else { $buildArguments+=@('--no-default-features','--features',$Feature) }
  & cargo @buildArguments
  if ($LASTEXITCODE -ne 0) { throw 'Module review build failed' }
  $metadata=cargo metadata --manifest-path $manifest --format-version 1 --no-deps | ConvertFrom-Json
  . (Join-Path $repoRoot 'scripts/ci/wasm-bindgen-version.ps1')
  $version=Get-LockedWasmBindgenVersion -RepositoryRoot $repoRoot
  $bindgen=Join-Path $env:TEMP "rpi-work-bindgen/wasm-bindgen-$version-x86_64-pc-windows-msvc/wasm-bindgen.exe"
  if (-not (Test-Path -LiteralPath $bindgen)) { $bindgen=(Get-Command wasm-bindgen -ErrorAction Stop).Source }
  $outputDirectory=Join-Path $repoRoot "target/parity-modules-$Feature-web"
  New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
  & $bindgen (Join-Path $metadata.target_directory 'wasm32-unknown-unknown/release/parity-module-check.wasm') --target web --out-dir $outputDirectory --out-name work-ui
  if ($LASTEXITCODE -ne 0) { throw 'Module review bindgen failed' }
  @'
<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Test-only Dioxus module review</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from './work-ui.js'; await init();</script></body></html>
'@ | Set-Content -LiteralPath (Join-Path $outputDirectory 'index.html') -Encoding utf8
  Write-Output "Test-only module URL: http://127.0.0.1:4182/modules/$Feature/"
} finally { Pop-Location }
