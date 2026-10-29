param()
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repoRoot
try {
    $logRoot = Join-Path $repoRoot 'target/final-rust-validation'
    New-Item -ItemType Directory -Force -Path $logRoot | Out-Null
    $checks = @(
        @{ Name = 'fmt'; Arguments = @('fmt', '--all', '--', '--check') },
        @{ Name = 'test'; Arguments = @('test', '--workspace', '--locked') },
        @{ Name = 'clippy-native'; Arguments = @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings') },
        @{ Name = 'clippy-wasm'; Arguments = @('clippy', '-p', 'work-ui', '--target', 'wasm32-unknown-unknown', '--all-targets', '--locked', '--', '-D', 'warnings') },
        @{ Name = 'build-native'; Arguments = @('build', '--locked', '-p', 'work-ui', '-p', 'work-cli') }
    )
    $results = @()
    foreach ($check in $checks) {
        $log = Join-Path $logRoot ($check.Name + '.log')
        $arguments = $check.Arguments
        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        # Windows PowerShell turns stderr into ErrorRecord objects. Capture those
        # without mistaking successful compiler progress for an error exit code.
        $ErrorActionPreference = 'Continue'
        & cargo @arguments *> $log
        $code = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        $results += [ordered]@{ name = $check.Name; command = 'cargo ' + ($arguments -join ' '); exitCode = $code; passed = ($code -eq 0); elapsedMs = $timer.ElapsedMilliseconds; log = $log }
        Write-Output "$($check.Name): exit $code"
        if ($code -ne 0) { Get-Content -LiteralPath $log -Tail 30 }
    }
    $passed = @($results | Where-Object { -not $_.passed }).Count -eq 0
    [ordered]@{ checkedAt = [DateTime]::UtcNow.ToString('o'); passed = $passed; checks = $results } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $repoRoot 'artifacts/engineering-validation.json') -Encoding utf8
    if (-not $passed) { exit 1 }
} finally { Pop-Location }
