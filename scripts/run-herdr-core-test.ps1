param([switch]$NoLaunch)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$testProfile = Join-Path $root 'target/herdr-verification/manual-profile'
$app = Join-Path $root 'target/debug/hitty-app.exe'
$hc = Join-Path $env:LOCALAPPDATA 'Programs/herdr-core/hc.exe'

if (-not (Test-Path -LiteralPath $app)) {
    throw "Hitty GUI build not found: $app"
}
if (-not (Test-Path -LiteralPath $hc)) {
    throw "herdr-core hc.exe not found: $hc"
}

New-Item -ItemType Directory -Force -Path $testProfile | Out-Null
$configPath = Join-Path $testProfile 'config.json'
if (-not (Test-Path -LiteralPath $configPath)) {
    $testConfig = @'
{"pane_engine":"herdr-core","check_for_updates":false,"auto_download_updates":false,"install_cli_on_path":false,"show_tray_icon":false,"shell":{"program":"powershell.exe","args":["-NoLogo","-NoProfile"]}}
'@
    [System.IO.File]::WriteAllText($configPath, $testConfig, [System.Text.UTF8Encoding]::new($false))
}

foreach ($entry in @(Get-ChildItem Env:)) {
    if ($entry.Name -like 'HITTY_*' -or $entry.Name -in @('HC_SESSION', 'HC_TERMINAL_ID')) {
        [Environment]::SetEnvironmentVariable($entry.Name, $null, 'Process')
    }
}
$env:HITTY_CONFIG_DIR = $testProfile
$env:HITTY_DATA_DIR = $testProfile
$env:HITTY_HC_BIN = $hc
$env:HITTY_PANE_ENGINE = 'herdr-core'

if ($NoLaunch) {
    Write-Host "Configuration prepared: $testProfile"
    exit 0
}

$process = Start-Process -FilePath $app -WorkingDirectory $testProfile -PassThru
Write-Host "Hitty herdr-core test started (PID $($process.Id))."
Write-Host "Code:    $root"
Write-Host "Profile: $testProfile"
Write-Host 'This profile keeps its sessions between launches and uses the installed shared hc core.'
