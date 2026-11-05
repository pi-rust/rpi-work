# Test-only real-component binary; does not build or replace the formal application.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
Push-Location $root
try {
    $manifest = 'tests/harness/ui-modules/Cargo.toml'
    & cargo build --locked --manifest-path $manifest --bin agent-backends --target wasm32-unknown-unknown --no-default-features
    if ($LASTEXITCODE -ne 0) { throw 'Agent backend harness build failed' }
    $metadata = cargo metadata --manifest-path $manifest --format-version 1 --no-deps | ConvertFrom-Json
    . (Join-Path $root 'scripts/ci/wasm-bindgen-version.ps1')
    $version = Get-LockedWasmBindgenVersion -RepositoryRoot $root
    $bindgen = Join-Path $env:TEMP "rpi-work-bindgen/wasm-bindgen-$version-x86_64-pc-windows-msvc/wasm-bindgen.exe"
    if (-not (Test-Path $bindgen)) { $bindgen = (Get-Command wasm-bindgen -ErrorAction Stop).Source }
    $output = Join-Path $root 'target/agent-backends-web'
    New-Item -ItemType Directory -Force $output | Out-Null
    & $bindgen (Join-Path $metadata.target_directory 'wasm32-unknown-unknown/debug/agent-backends.wasm') --target web --out-dir $output --out-name work-ui
    if ($LASTEXITCODE -ne 0) { throw 'Agent backend harness bindgen failed' }
    '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Backend test fixture</title><link rel="icon" href="data:,"></head><body><div id="main"></div><script type="module">import init from "./work-ui.js"; await init();</script></body></html>' | Set-Content (Join-Path $output 'index.html') -Encoding utf8
} finally { Pop-Location }
