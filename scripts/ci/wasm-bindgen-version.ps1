# A library function shared by local builds and CI. Do not duplicate the version
# in workflow YAML: the generated JS must match the locked Rust crate exactly.
function Get-LockedWasmBindgenVersion {
    param([Parameter(Mandatory)][string]$RepositoryRoot)
    $lockText = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'Cargo.lock') -Raw
    $versionEntries = [regex]::Matches($lockText, '(?m)^name = "wasm-bindgen"\r?\nversion = "([^"\r\n]+)"')
    $versions = @($versionEntries | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
    if ($versions.Count -ne 1) {
        throw 'Cargo.lock must contain exactly one wasm-bindgen version.'
    }
    return $versions[0]
}
