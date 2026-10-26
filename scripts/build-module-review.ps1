param([ValidateSet('all','meta','resources','trace','project','extras','conversation','settings','queue','scroll')][string]$Feature = 'all')
$ErrorActionPreference='Stop'
$repoRoot=Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  $staging=Join-Path $repoRoot 'target/parity-module-check'
  New-Item -ItemType Directory -Force -Path (Join-Path $staging 'src') | Out-Null
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'module-review/Cargo.toml') -Destination (Join-Path $staging 'Cargo.toml')
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'module-review/src/main.rs') -Destination (Join-Path $staging 'src/main.rs')
  if (-not (Test-Path -LiteralPath (Join-Path $staging 'Cargo.lock'))) { Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.lock') -Destination (Join-Path $staging 'Cargo.lock') }
  $buildArguments=@('build','--manifest-path',(Join-Path $staging 'Cargo.toml'),'--target','wasm32-unknown-unknown','--release')
  if ($Feature -ne 'all') { $buildArguments+=@('--no-default-features','--features',$Feature) }
  & cargo @buildArguments
  if ($LASTEXITCODE -ne 0) { throw 'Module review build failed' }
  $metadata=cargo metadata --manifest-path (Join-Path $staging 'Cargo.toml') --format-version 1 --no-deps | ConvertFrom-Json
  $bindgen=Join-Path $env:TEMP 'rpi-work-bindgen/wasm-bindgen-0.2.129-x86_64-pc-windows-msvc/wasm-bindgen.exe'
  if (-not (Test-Path -LiteralPath $bindgen)) { $bindgen=(Get-Command wasm-bindgen -ErrorAction Stop).Source }
  $outputDirectory=Join-Path $repoRoot "target/parity-modules-$Feature-web"
  New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
  & $bindgen (Join-Path $metadata.target_directory 'wasm32-unknown-unknown/release/parity-module-check.wasm') --target web --out-dir $outputDirectory --out-name work-ui
  if ($LASTEXITCODE -ne 0) { throw 'Module review bindgen failed' }
  @'
<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Test-only Dioxus module review</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from './work-ui.js'; await init();</script></body></html>
'@ | Set-Content -LiteralPath (Join-Path $outputDirectory 'index.html') -Encoding utf8
  Write-Output "Test-only module URL: http://127.0.0.1:4173/target/parity-modules-$Feature-web/"
} finally { Pop-Location }
