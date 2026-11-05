# Shared by direct Cargo checks and the Dioxus APK build. No machine-wide changes.
function Get-WorkAndroidToolchain {
    param(
        [ValidateSet('aarch64-linux-android', 'x86_64-linux-android')]
        [string]$Target = 'aarch64-linux-android',
        [string]$SdkRoot = '',
        [string]$NdkPath = ''
    )
    if (-not $SdkRoot) { $SdkRoot = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { $env:ANDROID_SDK_ROOT } }
    if (-not $NdkPath) {
        $NdkPath = if ($env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME } elseif ($env:NDK_HOME) { $env:NDK_HOME } elseif ($SdkRoot) { Join-Path $SdkRoot 'ndk/28.2.13676358' } else { '' }
    }
    if (-not $NdkPath -or -not (Test-Path -LiteralPath $NdkPath -PathType Container)) {
        throw 'Android NDK missing. Install ndk;28.2.13676358 in ANDROID_HOME, or set ANDROID_NDK_HOME / NDK_HOME to a complete NDK.'
    }
    $NdkPath = (Resolve-Path -LiteralPath $NdkPath).Path
    $windowsHost = $env:OS -eq 'Windows_NT'
    $hostTag = if ($windowsHost) { 'windows-x86_64' } elseif ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::OSX)) { 'darwin-x86_64' } else { 'linux-x86_64' }
    $bin = Join-Path $NdkPath "toolchains/llvm/prebuilt/$hostTag/bin"
    $exe = if ($windowsHost) { '.exe' } else { '' }
    $wrapper = if ($windowsHost) { '.cmd' } else { '' }
    # API 24 matches the application minSdk; do not accidentally link a host binary.
    $linker = Join-Path $bin "$($Target)24-clang$wrapper"
    $tools = @{
        CC = Join-Path $bin "clang$exe"
        CXX = Join-Path $bin "clang++$exe"
        AR = Join-Path $bin "llvm-ar$exe"
        Linker = $linker
    }
    foreach ($name in $tools.Keys) {
        if (-not (Test-Path -LiteralPath $tools[$name] -PathType Leaf)) {
            throw "Android NDK $name missing: $($tools[$name]). Check the NDK installation and host platform ($hostTag)."
        }
    }
    $properties = Join-Path $NdkPath 'source.properties'
    if (-not (Test-Path -LiteralPath $properties -PathType Leaf)) { throw "Android NDK source.properties missing: $NdkPath" }
    $revision = [regex]::Match([IO.File]::ReadAllText($properties), '(?m)^Pkg\.Revision\s*=\s*(\S+)').Groups[1].Value
    if (-not $revision) { throw "Invalid Android NDK revision: $properties" }
    [pscustomobject]@{
        Target = $Target; Ndk = $NdkPath; Revision = $revision; Bin = $bin
        CC = $tools.CC; CXX = $tools.CXX; AR = $tools.AR; Linker = $linker
        CompilerTarget = "$($Target)24"
    }
}

function Invoke-WithWorkAndroidToolchain {
    param(
        [ValidateSet('aarch64-linux-android', 'x86_64-linux-android')]
        [string]$Target = 'aarch64-linux-android',
        [string]$SdkRoot = '',
        [string]$NdkPath = '',
        [Parameter(Mandatory = $true)][scriptblock]$Action
    )
    $toolchain = Get-WorkAndroidToolchain -Target $Target -SdkRoot $SdkRoot -NdkPath $NdkPath
    $key = $Target.Replace('-', '_')
    $values = @{
        NDK_HOME = $toolchain.Ndk
        ANDROID_NDK_HOME = $toolchain.Ndk
        PATH = $toolchain.Bin + [IO.Path]::PathSeparator + $env:PATH
        "CC_$key" = $toolchain.CC
        "CXX_$key" = $toolchain.CXX
        "AR_$key" = $toolchain.AR
        "CARGO_TARGET_$($key.ToUpperInvariant())_LINKER" = $toolchain.Linker
    }
    foreach ($kind in @('CFLAGS', 'CXXFLAGS')) {
        # cc-rs accumulates global, TARGET_, underscore and hyphen flags in
        # that order. Append the target only to the last layer; leave every
        # other layer intact, including distinct flags in both spellings.
        $name = "$kind`_$Target"
        $flags = [Environment]::GetEnvironmentVariable($name, 'Process')
        $values[$name] = "$flags --target=$($toolchain.CompilerTarget)".Trim()
    }
    # Compiler/archiver selection is single-valued (unlike flags).
    # Override both spellings rather than retaining a stale high-priority tool.
    foreach ($kind in @('CC', 'CXX', 'AR')) { $values["$kind`_$Target"] = $values["${kind}_$key"] }
    $original = @{}
    foreach ($name in $values.Keys) { $original[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    try {
        foreach ($name in $values.Keys) { [Environment]::SetEnvironmentVariable($name, $values[$name], 'Process') }
        Write-Host "Android toolchain: NDK $($toolchain.Revision); target $Target; API 24; $($toolchain.Bin)"
        & $Action $toolchain
    } finally {
        foreach ($name in $original.Keys) { [Environment]::SetEnvironmentVariable($name, $original[$name], 'Process') }
    }
}
