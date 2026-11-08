# Scroll one top-level window of a process by posting WM_MOUSEWHEEL at a client
# point (physical pixels), after a WM_MOUSEMOVE there so the target knows where
# the pointer is. The real pointer never moves; no other window receives input.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/post-mouse-wheel.ps1 `
#     -ProcId <pid> -X <px> -Y <px> [-Notches 10] [-DelayMs 60]
param([int]$ProcId, [int]$X, [int]$Y, [int]$Notches = 10, [int]$DelayMs = 60)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Wheel {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
}
"@
[void][Wheel]::SetProcessDPIAware()
$hwnd = (Get-Process -Id $ProcId).MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { throw "no main window for pid $ProcId" }
$p = New-Object Wheel+POINT; $p.X = $X; $p.Y = $Y; [void][Wheel]::ClientToScreen($hwnd, [ref]$p)
$screen = [IntPtr](($p.Y -shl 16) -bor ($p.X -band 0xFFFF))
$client = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))
[void][Wheel]::PostMessage($hwnd, 0x200, [IntPtr]0, $client); Start-Sleep -Milliseconds 100
$wp = [IntPtr]([int](-120 -shl 16))
for ($i = 0; $i -lt $Notches; $i++) {
  [void][Wheel]::PostMessage($hwnd, 0x20A, $wp, $screen); Start-Sleep -Milliseconds $DelayMs
}
"wheel=$Notches down at client $X,$Y"
