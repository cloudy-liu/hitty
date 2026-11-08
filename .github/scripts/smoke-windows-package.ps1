# Run only on disposable Windows CI: the installer owns a per-user uninstall key.
param([Parameter(Mandatory = $true)][string]$Arch)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'Run installer acceptance on a disposable GitHub Actions Windows runner.'
}
$Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$Name = "hitty-$Version-windows-$Arch"
$Evidence = Join-Path $PWD 'package-smoke'
$Portable = Join-Path $env:RUNNER_TEMP "hitty-portable-$([guid]::NewGuid())"
Expand-Archive -LiteralPath "dist/$Name.zip" -DestinationPath $Portable
& python scripts/bootstrap-native-smoke.py "$Portable/hitty-app.exe" "$Portable/hitty.exe" "$Evidence/portable" --phase after --no-capture --reconnect
if ($LASTEXITCODE -ne 0) { throw 'The portable application smoke check failed.' }

$Key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{C8AFFE59-EF17-4079-BD64-2560017505CB}_is1'
if (Test-Path -LiteralPath $Key) { throw 'Refusing to replace an existing Hitty installation.' }
$Install = Join-Path $env:RUNNER_TEMP "hitty-installed-$([guid]::NewGuid())"
$Setup = (Resolve-Path -LiteralPath "dist/$Name-setup.exe").Path
$InstallLog = Join-Path $Evidence 'install.log'
try {
    $Arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-',
        '/CURRENTUSER', '/NOICONS', '/TASKS=""', "/DIR=`"$Install`"", "/LOG=`"$InstallLog`"")
    $Process = Start-Process -FilePath $Setup -ArgumentList $Arguments -Wait -PassThru -WindowStyle Hidden
    if ($Process.ExitCode -ne 0) { throw "Installer exited with $($Process.ExitCode)." }
    $Entry = Get-ItemProperty -LiteralPath $Key
    if ($Entry.DisplayName -notlike 'Hitty*' -or $Entry.DisplayVersion -ne $Version) {
        throw 'The installed product name or version is wrong.'
    }
    if ([IO.Path]::GetFullPath($Entry.InstallLocation).TrimEnd('\') -ne [IO.Path]::GetFullPath($Install)) {
        throw 'The installer registered a different application directory.'
    }
    if ((Get-Content -LiteralPath "$Install/.hitty-inno-install" -Raw) -ne 'inno-v1') {
        throw 'The installed update marker is missing or incorrect.'
    }
    $Entry | Select-Object DisplayName,DisplayVersion,InstallLocation,DisplayIcon |
        ConvertTo-Json | Set-Content -LiteralPath "$Evidence/installed-identity.json" -Encoding utf8
    & python scripts/bootstrap-native-smoke.py "$Install/hitty-app.exe" "$Install/hitty.exe" "$Evidence/installed" --phase after --no-capture --reconnect
    if ($LASTEXITCODE -ne 0) { throw 'The installed application smoke check failed.' }
} finally {
    $Uninstaller = Join-Path $Install 'unins000.exe'
    if (Test-Path -LiteralPath $Uninstaller) {
        $Process = Start-Process -FilePath $Uninstaller -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Wait -PassThru -WindowStyle Hidden
        if ($Process.ExitCode -ne 0) { throw "Uninstaller exited with $($Process.ExitCode)." }
    }
}
if (Test-Path -LiteralPath $Key) { throw 'The uninstall registration was not removed.' }
if (Test-Path -LiteralPath "$Install/hitty-app.exe") { throw 'The installed application was not removed.' }
Write-Output 'Windows portable, install, session reconnect, and uninstall checks passed.'
