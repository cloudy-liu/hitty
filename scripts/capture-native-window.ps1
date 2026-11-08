# Render one top-level window of a running process to a PNG without the
# pointer or overlapping windows: PrintWindow with PW_RENDERFULLCONTENT,
# cropped to the DWM extended frame bounds. Optionally posts a synthetic
# pointer move or left click to that window first; the real pointer never
# moves and no other window receives input.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/capture-native-window.ps1 `
#     -ProcId <pid> -Out <file.png> [-Hover | -Click | -MoveTo "x,y"] [-SettleMs 1500]
#
# -Hover moves the synthetic pointer to the centre of the client area, -Click
# also posts a left click there, and -MoveTo targets explicit client
# coordinates in physical pixels.
param([int]$ProcId, [string]$Out, [switch]$Click, [switch]$Hover, [string]$MoveTo = "", [int]$SettleMs = 1500)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class WinCap {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, int attr, out RECT pv, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
}
"@
[void][WinCap]::SetProcessDPIAware()
$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 50 -and $hwnd -eq [IntPtr]::Zero; $i++) { $hwnd = (Get-Process -Id $ProcId).MainWindowHandle; if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 200 } }
if ($hwnd -eq [IntPtr]::Zero) { throw "no main window for pid $ProcId" }
$c = New-Object WinCap+RECT; [void][WinCap]::GetClientRect($hwnd, [ref]$c)
$x = [int](($c.R - $c.L) / 2); $y = [int](($c.B - $c.T) / 2)
if ($MoveTo -ne "") { $parts = $MoveTo.Split(','); $x = [int]$parts[0]; $y = [int]$parts[1] }
$lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
$pointer = "none"
if ($Click -or $Hover -or $MoveTo -ne "") {
  [void][WinCap]::PostMessage($hwnd, 0x200, [IntPtr]0, $lp); Start-Sleep -Milliseconds 120
  $pointer = "move=$x,$y"
}
if ($Click) {
  [void][WinCap]::PostMessage($hwnd, 0x201, [IntPtr]1, $lp); Start-Sleep -Milliseconds 60
  [void][WinCap]::PostMessage($hwnd, 0x202, [IntPtr]0, $lp)
  $pointer = "click=$x,$y"
}
Start-Sleep -Milliseconds $SettleMs
$r = New-Object WinCap+RECT; [void][WinCap]::GetWindowRect($hwnd, [ref]$r)
$e = New-Object WinCap+RECT; if ([WinCap]::DwmGetWindowAttribute($hwnd, 9, [ref]$e, 16) -ne 0) { $e = $r }
$w = $r.R - $r.L; $h = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinCap]::PrintWindow($hwnd, $hdc, 2)
$g.ReleaseHdc($hdc); $g.Dispose(); if (-not $ok) { $bmp.Dispose(); throw "PrintWindow failed for hwnd $hwnd" }
$crop = New-Object System.Drawing.Rectangle ($e.L - $r.L), ($e.T - $r.T), ($e.R - $e.L), ($e.B - $e.T)
$final = $bmp.Clone($crop, $bmp.PixelFormat)
$final.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $final.Dispose(); $bmp.Dispose()
"hwnd=$hwnd client=$($c.R - $c.L)x$($c.B - $c.T) frame=$($crop.Width)x$($crop.Height) printwindow=$ok pointer=$pointer size=$((Get-Item $Out).Length)"
