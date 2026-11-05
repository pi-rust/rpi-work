param(
    [ValidateSet('aarch64-linux-android', 'x86_64-linux-android')]
    [string]$Target = 'aarch64-linux-android',
    [string]$NdkPath = '',
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
# Compiler progress is stderr, not a PowerShell exception; check native exit codes.
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'android-toolchain.ps1')
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
Push-Location $repoRoot
try {
    Invoke-WithWorkAndroidToolchain -Target $Target -NdkPath $NdkPath -Action {
        $cargoArguments = @('check', '--locked', '--target', $Target)
        if ($Offline) { $cargoArguments += '--offline' }
        & cargo @cargoArguments -p work-client
        if ($LASTEXITCODE -ne 0) { throw "Android work-client check failed ($Target)." }
        & cargo @cargoArguments -p work-ui --features mobile
        if ($LASTEXITCODE -ne 0) { throw "Android mobile UI check failed ($Target)." }
    }
} finally { Pop-Location }
