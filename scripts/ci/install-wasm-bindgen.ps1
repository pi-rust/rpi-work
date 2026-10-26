$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $PSScriptRoot 'wasm-bindgen-version.ps1')
$version = Get-LockedWasmBindgenVersion -RepositoryRoot $repoRoot
$installed = Get-Command wasm-bindgen -ErrorAction SilentlyContinue
if ($installed) {
    $reported = & $installed.Source --version
    if ($LASTEXITCODE -eq 0 -and "$reported".Trim() -eq "wasm-bindgen $version") {
        Write-Output "wasm-bindgen $version is already installed."
        return
    }
}
& cargo install wasm-bindgen-cli --version $version --locked --force
if ($LASTEXITCODE -ne 0) { throw "Installing wasm-bindgen-cli $version failed." }
