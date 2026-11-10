# Launch the isolated theme evaluation app. Re-runs retain edits and sessions.
param(
    [switch]$Build,
    [switch]$PrepareOnly,
    [string]$TargetDir = ""
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$reviewRoot = Join-Path $repoRoot '.hitty-dev-otty-fidelity'
$binDir = Join-Path $reviewRoot 'bin'
$configDir = Join-Path $reviewRoot 'config'
$dataDir = Join-Path $reviewRoot 'data'
$logDir = Join-Path $reviewRoot 'logs'
$themesDir = Join-Path $configDir 'themes'
foreach ($directory in @($binDir, $configDir, $dataDir, $logDir, $themesDir)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
}
$app = Join-Path $binDir 'hitty-app.exe'
if ($Build -or -not (Test-Path -LiteralPath $app)) {
    if ($TargetDir -eq '') { $TargetDir = Join-Path $repoRoot 'target' }
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (-not (Test-Path -LiteralPath $cargo)) { $cargo = 'cargo' }
    Push-Location $repoRoot
    try {
        & $cargo build --locked --workspace --bins --features updater --target-dir $TargetDir
        if ($LASTEXITCODE -ne 0) { throw "Locked Cargo build failed ($LASTEXITCODE)." }
    } finally { Pop-Location }
    $debugDir = Join-Path $TargetDir 'debug'
    foreach ($name in @('hitty-app.exe', 'hitty.exe', 'hitty-server.exe', 'hitty-updater.exe', 'conpty.dll')) {
        $source = Join-Path $debugDir $name
        if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination (Join-Path $binDir $name) -Force }
    }
}
$utf8 = New-Object System.Text.UTF8Encoding $false
$configPath = Join-Path $configDir 'config.json'
if (-not (Test-Path -LiteralPath $configPath)) {
    $sample = (Join-Path $PSScriptRoot 'otty-theme-review-sample.ps1').Replace("'", "''")
    $config = [ordered]@{
        theme_preset = 'one_light'
        theme_follow_system = $false
        theme_preset_light = 'one_light'
        theme_preset_dark = 'one_dark_pro'
        theme_legible_palette = $true
        theme_style_legible_palette = $false
        window_opacity = $null
        window_blur = $null
        window_backdrop = 'auto'
        gui_language = 'zh-CN'
        check_for_updates = $false
        auto_download_updates = $false
        install_cli_on_path = $false
        show_tray_icon = $false
        shell = @{ program = 'powershell.exe'; args = @('-NoLogo', '-NoProfile', '-NoExit', '-ExecutionPolicy', 'Bypass', '-Command', "& '$sample'") }
    }
    [System.IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 5), $utf8)
    [System.IO.File]::WriteAllText((Join-Path $configDir 'window.json'), '{"x":80,"y":80,"width":1200,"height":800}', $utf8)
}
Write-Output "Theme review: 13 original themes + Solarized Light."
Write-Output "Configuration: $configDir"
Write-Output "Experience guide: $(Join-Path $repoRoot 'docs\testing\otty-theme-fidelity.md')"
if ($PrepareOnly) { exit 0 }

# The app and child daemon inherit only this profile's routing. Restore this
# PowerShell process's environment after launch so dot-sourcing is also safe.
$routing = @(@('HITTY_CONFIG_DIR', 'HITTY_DATA_DIR') + @(Get-ChildItem Env: | Where-Object { $_.Name -eq 'HITTY' -or $_.Name.StartsWith('HITTY_') } | ForEach-Object { $_.Name }) | Select-Object -Unique)
$saved = @{}
foreach ($key in $routing) {
    $saved[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
    [Environment]::SetEnvironmentVariable($key, $null, 'Process')
}
try {
    $env:HITTY_CONFIG_DIR = $configDir
    $env:HITTY_DATA_DIR = $dataDir
    # This is the interactive GUI the user is opening, not a background helper.
    $process = Start-Process -FilePath $app -ArgumentList @('--config-dir', ('"' + $configDir + '"')) -WorkingDirectory $repoRoot -WindowStyle Normal -PassThru
    Write-Output "Started theme review, PID $($process.Id)."
} finally {
    foreach ($key in $routing) { [Environment]::SetEnvironmentVariable($key, $saved[$key], 'Process') }
}
