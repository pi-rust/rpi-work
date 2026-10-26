param([ValidateSet('meta','resources','trace','project','extras','conversation','queue','scroll','fmt','format','autofix','test','clippy','clippy-wasm','wasm')][string]$Check)
# Deliberately a DIAGNOSTIC transport for RPI's fail-fast shell harness.
# The real exit code is recorded and printed; nonzero never means verification passed.
$ErrorActionPreference='Continue'
$PSNativeCommandUseErrorActionPreference=$false
$repoRoot=Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  $diagnosticDirectory=Join-Path $repoRoot 'target/parity-diagnostics'
  New-Item -ItemType Directory -Force -Path $diagnosticDirectory | Out-Null
  $logPath=Join-Path $diagnosticDirectory "$Check.log"
  $arguments=switch($Check) {
    'meta' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','meta','--message-format','short') }
    'resources' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','resources','--message-format','short') }
    'trace' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','trace','--message-format','short') }
    'project' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','project','--message-format','short') }
    'extras' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','extras','--message-format','short') }
    'conversation' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','conversation','--message-format','short') }
    'queue' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','queue','--message-format','short') }
    'scroll' { @('check','--manifest-path','target/parity-module-check/Cargo.toml','--target','wasm32-unknown-unknown','--no-default-features','--features','scroll','--message-format','short') }
    'fmt' { @('fmt','--all','--','--check') }
    'format' { @('fmt','--all') }
    'autofix' { @('clippy','--fix','--workspace','--all-targets','--allow-dirty','--allow-staged','--message-format','short','--','-D','warnings') }
    'test' { @('test','--workspace') }
    'clippy' { @('clippy','--workspace','--all-targets','--','-D','warnings') }
    'clippy-wasm' { @('clippy','--target','wasm32-unknown-unknown','-p','work-ui','--message-format','short','--','-D','warnings') }
    'wasm' { @('check','--target','wasm32-unknown-unknown','-p','work-ui') }
  }
  & cargo @arguments 2>&1 | Tee-Object -FilePath $logPath | Select-Object -Last 100
  $reviewExitCode=$LASTEXITCODE
  [ordered]@{check=$Check;exitCode=$reviewExitCode;checkedAt=[DateTime]::UtcNow.ToString('o');log=$logPath} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $diagnosticDirectory "$Check.json") -Encoding utf8
  Write-Output "REVIEW DIAGNOSTIC EXIT CODE: $reviewExitCode ($Check). A nonzero code is a failed check; repair and rerun. Full log: $logPath"
} finally { Pop-Location }
exit 0
