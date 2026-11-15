param(
    [Parameter(Mandatory)][string]$Executable,
    [int]$WindowProcessId = 0
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class WorkBrandIcon {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageTimeoutW(IntPtr h, uint m, IntPtr w, IntPtr l, uint flags, uint ms, out IntPtr result);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
'@
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
function Test-BrandBitmap([System.Drawing.Bitmap]$Bitmap) {
    # Compare every pixel with the corresponding committed ICO frame, including
    # the gear's negative spaces. Shell extraction chooses its own frame size.
    $referenceIcon = [System.Drawing.Icon]::new((Join-Path $root 'assets/branding/rpi.ico'), $Bitmap.Width, $Bitmap.Height)
    $expected = $referenceIcon.ToBitmap()
    $maxDifference = 0
    try {
        for ($y = 0; $y -lt $Bitmap.Height; $y++) {
            for ($x = 0; $x -lt $Bitmap.Width; $x++) {
                $actualPixel = $Bitmap.GetPixel($x, $y)
                $expectedPixel = $expected.GetPixel($x, $y)
                if ($actualPixel.A -eq 0 -and $expectedPixel.A -eq 0) { continue }
                foreach ($channel in @('R','G','B','A')) {
                    $difference = [Math]::Abs([int]$actualPixel.$channel - [int]$expectedPixel.$channel)
                    $maxDifference = [Math]::Max($maxDifference, $difference)
                }
            }
        }
        if ($maxDifference -gt 8) { throw 'Native icon differs from the supplied gear logo.' }
        return @{ width=$Bitmap.Width; height=$Bitmap.Height; brandSamplesMatch=$true; maxDifference=$maxDifference }
    } finally {
        $expected.Dispose()
        $referenceIcon.Dispose()
    }
}
$fileIcon = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path -LiteralPath $Executable).Path)
if (!$fileIcon) { throw 'No executable shell icon.' }
try {
    $bitmap = $fileIcon.ToBitmap()
    try { $result = @{ executable = (Test-BrandBitmap $bitmap) } } finally { $bitmap.Dispose() }
} finally { $fileIcon.Dispose() }
if ($WindowProcessId -gt 0) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $process = Get-Process -Id $WindowProcessId
        $handle = $process.MainWindowHandle
        if ($handle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($handle -eq [IntPtr]::Zero) { throw 'Owned desktop window unavailable.' }
    [uint32]$owner = 0
    $null = [WorkBrandIcon]::GetWindowThreadProcessId($handle,[ref]$owner)
    if ($owner -ne $WindowProcessId) { throw 'Window process identity mismatch.' }
    [IntPtr]$iconHandle = [IntPtr]::Zero
    $response = [WorkBrandIcon]::SendMessageTimeoutW($handle,0x7F,[IntPtr]1,[IntPtr]::Zero,2,2000,[ref]$iconHandle)
    if ($response -eq [IntPtr]::Zero -or $iconHandle -eq [IntPtr]::Zero) { throw 'Window did not return its application icon.' }
    # Borrow the window-owned HICON; do not call DestroyIcon on that handle.
    $windowIcon = [System.Drawing.Icon]::FromHandle($iconHandle)
    try {
        $bitmap = $windowIcon.ToBitmap()
        try { $result.window = Test-BrandBitmap $bitmap } finally { $bitmap.Dispose() }
    } finally { $windowIcon.Dispose() }
}
$result | ConvertTo-Json -Depth 4 -Compress
