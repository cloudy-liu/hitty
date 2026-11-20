# Capture matching commit/worktree scenes without moving the real pointer.
# Build with --features markdown-visual-tests, then prepare the fixture with
# scripts/prepare-diff-toolbar-captures.py. Use a fresh ConfigDirectory per run.
param(
  [Parameter(Mandatory=$true)][string]$Binary,
  [Parameter(Mandatory=$true)][string]$FixtureDirectory,
  [Parameter(Mandatory=$true)][string]$ConfigDirectory,
  [Parameter(Mandatory=$true)][string]$EvidenceDirectory,
  [ValidateSet('light', 'dark')][string]$Theme = 'light',
  [ValidateSet('before', 'after')][string]$Phase = 'after'
)
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $ConfigDirectory) { throw 'Use a fresh isolated ConfigDirectory' }
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
[void][IO.Directory]::CreateDirectory($evidence)
$options = Get-Content -LiteralPath (Join-Path $FixtureDirectory "$Theme.json") -Raw | ConvertFrom-Json
$options.output = Join-Path $evidence "$Phase-$Theme-ready.json"
if (Test-Path -LiteralPath $options.output) { Remove-Item -LiteralPath $options.output }
$optionsPath = Join-Path $evidence "$Phase-$Theme-options.json"
[IO.File]::WriteAllText($optionsPath, ($options | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
$app = Start-Process -FilePath $Binary -ArgumentList @('--config-dir', "`"$ConfigDirectory`"", '--document-visual', "`"$optionsPath`"") -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $evidence "$Phase-$Theme.stdout.log") -RedirectStandardError (Join-Path $evidence "$Phase-$Theme.stderr.log")
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class DiffFixtureWindow {
  public delegate bool Callback(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback c, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder b, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  public static IntPtr Show(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h,p) => {
      uint owner; GetWindowThreadProcessId(h, out owner);
      if (owner != pid) return true;
      var title = new StringBuilder(256); GetWindowText(h, title, 256);
      if (!title.ToString().Contains("acceptance")) return true;
      ShowWindow(h, 4); found = h; return false;
    }, IntPtr.Zero);
    return found;
  }
}
"@
$window = [IntPtr]::Zero
for ($attempt = 0; $attempt -lt 100; $attempt++) {
  Start-Sleep -Milliseconds 300
  $window = [DiffFixtureWindow]::Show($app.Id)
  if ($window -ne [IntPtr]::Zero -and (Test-Path -LiteralPath $options.output)) { break }
  if ($app.HasExited) { throw "Fixture exited: inspect $Phase-$Theme.stderr.log" }
}
if ($window -eq [IntPtr]::Zero -or !(Test-Path -LiteralPath $options.output)) { throw 'Fixture did not become ready' }
$ready = Get-Content -LiteralPath $options.output -Raw | ConvertFrom-Json
$scale = $ready.scale_factor
if (!$scale) { throw 'Fixture did not report its display scale' }
$scratch = Join-Path $evidence "$Phase-$Theme-click.png"
function ClickAt([double]$X, [double]$Y) {
  $point = "$([int]($X * $scale)),$([int]($Y * $scale))"
  & "$PSScriptRoot/capture-native-window.ps1" -ProcId $app.Id -Out $scratch -Click -MoveTo $point -SettleMs 800
}
function SaveScene([string]$Scene) {
  & "$PSScriptRoot/capture-native-window.ps1" -ProcId $app.Id -Out (Join-Path $evidence "$Phase-$Scene-$Theme.png") -MoveTo '1,1' -SettleMs 600
}
ClickAt 1173 45
ClickAt 1153 320
SaveScene 'commit'
ClickAt 179 181
SaveScene 'worktree'
$app.Id | Set-Content -LiteralPath (Join-Path $evidence "$Phase-$Theme.pid")
"Fixture pid=$($app.Id); expires after 600 seconds. Native images: $evidence"
