param([switch]$NativeProbe)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repoRoot 'scripts/ci/android-toolchain.ps1')
function Assert-True($Value, [string]$Message) { if (-not $Value) { throw $Message } }
function Assert-Throws([scriptblock]$Action, [string]$Message) {
    $failed = $false
    try { & $Action | Out-Null } catch { $failed = $true }
    Assert-True $failed $Message
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('work ndk fixture ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$names = @('PATH', 'NDK_HOME', 'ANDROID_NDK_HOME', 'CC', 'CFLAGS', 'CC_x86_64_linux_android', 'CFLAGS_x86_64_linux_android')
foreach ($target in @('aarch64-linux-android', 'x86_64-linux-android')) {
    $key = $target.Replace('-', '_')
    $names += "CARGO_TARGET_$($key.ToUpperInvariant())_LINKER"
    foreach ($kind in @('CC', 'CXX', 'AR', 'CFLAGS', 'CXXFLAGS')) { $names += "${kind}_$key", "$kind`_$target" }
}
$before = @{}
foreach ($name in ($names | Sort-Object -Unique)) { $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
try {
    $sdk = Join-Path $root 'sdk with spaces'
    $ndk = Join-Path $sdk 'ndk/28.2.13676358'
    $hostTag = if ($env:OS -eq 'Windows_NT') { 'windows-x86_64' } elseif ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([Runtime.InteropServices.OSPlatform]::OSX)) { 'darwin-x86_64' } else { 'linux-x86_64' }
    $bin = Join-Path $ndk "toolchains/llvm/prebuilt/$hostTag/bin"
    New-Item -ItemType Directory -Path $bin -Force | Out-Null
    $exe = if ($env:OS -eq 'Windows_NT') { '.exe' } else { '' }
    $wrapper = if ($env:OS -eq 'Windows_NT') { '.cmd' } else { '' }
    [IO.File]::WriteAllText((Join-Path $ndk 'source.properties'), 'Pkg.Revision = 28.2.13676358')
    foreach ($name in @("clang$exe", "clang++$exe", "llvm-ar$exe", "aarch64-linux-android24-clang$wrapper", "x86_64-linux-android24-clang$wrapper")) {
        [IO.File]::WriteAllText((Join-Path $bin $name), 'fixture: never execute')
    }
    $env:ANDROID_NDK_HOME = $null; $env:NDK_HOME = $null
    $tools = Get-WorkAndroidToolchain -SdkRoot $sdk
    Assert-True ($tools.Revision -eq '28.2.13676358' -and $tools.Bin -eq $bin) 'Pinned SDK discovery failed with spaces.'
    $env:NDK_HOME = $ndk
    Assert-True ((Get-WorkAndroidToolchain).Ndk -eq $ndk) 'NDK_HOME not resolved.'
    $env:ANDROID_NDK_HOME = Join-Path $root 'missing explicit NDK'
    Assert-Throws { Get-WorkAndroidToolchain -SdkRoot $sdk } 'Invalid explicit NDK silently fell back.'
    Assert-True ((Get-WorkAndroidToolchain -NdkPath $ndk).Ndk -eq $ndk) 'NdkPath must override stale env.'
    $env:ANDROID_NDK_HOME = $ndk
    $env:CC = 'host-compiler-must-not-change'
    $env:CFLAGS = '-DFIXTURE_GLOBAL=1'
    $env:CFLAGS_aarch64_linux_android = '-DFIXTURE_TARGET=1'
    [Environment]::SetEnvironmentVariable('CFLAGS_aarch64-linux-android', '-DFIXTURE_HYPHEN=1', 'Process')
    $env:CXXFLAGS_aarch64_linux_android = '-DFIXTURE_CXX_UNDERSCORE=1'
    [Environment]::SetEnvironmentVariable('CXXFLAGS_aarch64-linux-android', '-DFIXTURE_CXX_HYPHEN=1', 'Process')
    [Environment]::SetEnvironmentVariable('CC_aarch64-linux-android', 'stale-high-priority-cc', 'Process')
    $saved = @{}
    foreach ($name in $before.Keys) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    $checkEnvironment = {
        Assert-True ($env:CC_aarch64_linux_android -eq (Join-Path $bin "clang$exe")) 'Target compiler missing.'
        Assert-True ([Environment]::GetEnvironmentVariable('CC_aarch64-linux-android', 'Process') -eq $env:CC_aarch64_linux_android) 'cc-rs high-priority spelling not overridden.'
        Assert-True ($env:CFLAGS_aarch64_linux_android -eq '-DFIXTURE_TARGET=1') 'Underscore C flags changed.'
        Assert-True ([Environment]::GetEnvironmentVariable('CFLAGS_aarch64-linux-android', 'Process') -eq '-DFIXTURE_HYPHEN=1 --target=aarch64-linux-android24') 'Hyphen C flags lost or target missing.'
        Assert-True ($env:CXXFLAGS_aarch64_linux_android -eq '-DFIXTURE_CXX_UNDERSCORE=1') 'Underscore C++ flags changed.'
        Assert-True ([Environment]::GetEnvironmentVariable('CXXFLAGS_aarch64-linux-android', 'Process') -eq '-DFIXTURE_CXX_HYPHEN=1 --target=aarch64-linux-android24') 'Hyphen C++ flags lost or target missing.'
        Assert-True ($env:CC -eq $saved.CC -and $env:CFLAGS -eq $saved.CFLAGS) 'Global host compiler/flags changed.'
        Assert-True ($env:CC_x86_64_linux_android -eq $saved.CC_x86_64_linux_android) 'Other target compiler changed.'
        Assert-True ($env:PATH.StartsWith($bin + [IO.Path]::PathSeparator)) 'NDK bin not prepended.'
        Assert-True ($env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER -eq $tools.Linker) 'Rust linker not API 24 wrapper.'
    }
    Invoke-WithWorkAndroidToolchain -NdkPath $ndk -Action $checkEnvironment
    foreach ($name in $saved.Keys) { Assert-True ([Environment]::GetEnvironmentVariable($name, 'Process') -eq $saved[$name]) "Success leaked environment $name" }
    Assert-Throws { Invoke-WithWorkAndroidToolchain -NdkPath $ndk -Action { throw 'fixture action failure' } } 'Action failure swallowed.'
    foreach ($name in $saved.Keys) { Assert-True ([Environment]::GetEnvironmentVariable($name, 'Process') -eq $saved[$name]) "Failure leaked environment $name" }
    Invoke-WithWorkAndroidToolchain -Target x86_64-linux-android -NdkPath $ndk -Action {
        Assert-True ([Environment]::GetEnvironmentVariable('CFLAGS_x86_64-linux-android', 'Process') -match '--target=x86_64-linux-android24$') 'x86_64 target not selected.'
        Assert-True ($env:CC_aarch64_linux_android -eq $saved.CC_aarch64_linux_android) 'ARM64 compiler changed by x86_64.'
    }
    $clang = Join-Path $bin "clang$exe"
    Remove-Item -LiteralPath $clang
    Assert-Throws { Invoke-WithWorkAndroidToolchain -NdkPath $ndk -Action { } } 'Incomplete NDK accepted.'
    foreach ($name in $saved.Keys) { Assert-True ([Environment]::GetEnvironmentVariable($name, 'Process') -eq $saved[$name]) "Resolution failure leaked environment $name" }
    Write-Host 'PASS isolated toolchain discovery, explicit precedence, paths with spaces, missing compiler, both target spellings/ABIs, flags and environment restoration'
} finally {
    foreach ($name in $before.Keys) { [Environment]::SetEnvironmentVariable($name, $before[$name], 'Process') }
    Remove-Item -LiteralPath $root -Recurse -Force
}
if ($NativeProbe) {
    # Use the installed NDK only on explicit opt-in; no Agent, credentials or user sessions.
    $probe = Join-Path $repoRoot ('target/android-toolchain-probe/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $probe -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $probe 'probe.c'), 'int fixture_probe(void) { return 24; }')
    [IO.File]::WriteAllText((Join-Path $probe 'probe.rs'), '#[unsafe(no_mangle)] pub extern "C" fn fixture_probe() -> u32 { 24 }')
    foreach ($target in @('aarch64-linux-android', 'x86_64-linux-android')) {
        Invoke-WithWorkAndroidToolchain -Target $target -Action {
            param($toolchain)
            $object = Join-Path $probe "$target.o"
            & $toolchain.CC "--target=$($toolchain.CompilerTarget)" -c (Join-Path $probe 'probe.c') -o $object
            if ($LASTEXITCODE -ne 0) { throw 'Android C compile probe failed.' }
            $library = Join-Path $probe "$target.so"
            & rustc --edition=2024 --crate-name ndk_probe --crate-type cdylib --target $target (Join-Path $probe 'probe.rs') -C "linker=$($toolchain.Linker)" -o $library
            if ($LASTEXITCODE -ne 0) { throw 'Android Rust link probe failed.' }
            $reader = Join-Path $toolchain.Bin $(if ($env:OS -eq 'Windows_NT') { 'llvm-readelf.exe' } else { 'llvm-readelf' })
            $header = & $reader --file-header $library
            if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect Android probe ELF.' }
            $machine = if ($target -eq 'aarch64-linux-android') { 'AArch64' } else { 'X86-64' }
            Assert-True (($header -join "`n") -match "Machine:\s+.*$machine") 'Wrong Android ELF machine.'
            Write-Host "PASS installed NDK C compile and Rust cdylib link: $target ($library)"
        }
    }
}
