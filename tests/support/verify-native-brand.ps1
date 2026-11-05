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
$expected = [System.Drawing.Bitmap]::new((Join-Path $root 'assets/branding/rpi-256.png'))
function Test-BrandBitmap([System.Drawing.Bitmap]$Bitmap) {
    # Samples are well inside the three Web brand bars and its cream backdrop,
    # not antialiased boundaries. Shell extraction may select a 16/32/48px frame.
    foreach ($point in @(@(4,16), @(8,20), @(16,12), @(24,21))) {
        $a = $Bitmap.GetPixel([Math]::Min($Bitmap.Width-1,[int][Math]::Floor($point[0]*$Bitmap.Width/32)), [Math]::Min($Bitmap.Height-1,[int][Math]::Floor($point[1]*$Bitmap.Height/32)))
        $b = $expected.GetPixel($point[0]*8,$point[1]*8)
        if ([Math]::Abs([int]$a.R-$b.R) -gt 8 -or [Math]::Abs([int]$a.G-$b.G) -gt 8 -or [Math]::Abs([int]$a.B-$b.B) -gt 8 -or $a.A -lt 240) {
            throw 'Native icon differs from the Web brand samples.'
        }
    }
    return @{ width=$Bitmap.Width; height=$Bitmap.Height; brandSamplesMatch=$true }
}
try {
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
} finally { $expected.Dispose() }
