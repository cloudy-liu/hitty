param(
    [Parameter(Mandatory = $true)][int]$RootProcessId,
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$Output,
    [ValidateRange(2, 120)][int]$Samples = 5
)
$ErrorActionPreference = 'Stop'
$rows = @()
$previousCpu = @{}
$clock = [System.Diagnostics.Stopwatch]::StartNew()
$previousSeconds = 0.0
for ($sample = 0; $sample -lt $Samples; $sample++) {
    $all = @(Get-CimInstance Win32_Process)
    if (-not ($all | Where-Object { $_.ProcessId -eq $RootProcessId })) {
        throw "Root process $RootProcessId is no longer running"
    }
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootProcessId)
    do {
        $added = $false
        foreach ($process in $all) {
            if ($ids.Contains([int]$process.ParentProcessId) -and $ids.Add([int]$process.ProcessId)) {
                $added = $true
            }
        }
    } while ($added)
    $stats = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
        Where-Object { $ids.Contains([int]$_.IDProcess) })
    $processes = @($all | Where-Object { $ids.Contains([int]$_.ProcessId) } | ForEach-Object {
        $process = $_
        $stat = $stats | Where-Object { $_.IDProcess -eq $process.ProcessId } | Select-Object -First 1
        [pscustomobject]@{
            id = $process.ProcessId
            parent = $process.ParentProcessId
            name = $process.Name
            private_working_set_bytes = [long]$stat.WorkingSetPrivate
            working_set_bytes = [long]$stat.WorkingSet
            private_committed_bytes = [long]$stat.PrivateBytes
            cpu_one_core_percent = [double]$stat.PercentProcessorTime
            cpu_seconds = (Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue).TotalProcessorTime.TotalSeconds
        }
    })
    $elapsedSeconds = $clock.Elapsed.TotalSeconds
    $cpuDelta = 0.0
    foreach ($process in $processes) {
        if ($previousCpu.ContainsKey([int]$process.id)) {
            $cpuDelta += [math]::Max(0, $process.cpu_seconds - $previousCpu[[int]$process.id])
        }
        $previousCpu[[int]$process.id] = $process.cpu_seconds
    }
    $cpuPercent = if ($sample -eq 0) { $null } else { [math]::Round(100 * $cpuDelta / ($elapsedSeconds - $previousSeconds), 2) }
    $previousSeconds = $elapsedSeconds
    $rows += [pscustomobject]@{
        utc = [DateTime]::UtcNow.ToString('o')
        processes = $processes
        private_working_set_mib = [math]::Round(($processes | Measure-Object private_working_set_bytes -Sum).Sum / 1MB, 2)
        working_set_mib = [math]::Round(($processes | Measure-Object working_set_bytes -Sum).Sum / 1MB, 2)
        private_committed_mib = [math]::Round(($processes | Measure-Object private_committed_bytes -Sum).Sum / 1MB, 2)
        cpu_one_core_percent = $cpuPercent
    }
    if ($sample + 1 -lt $Samples) { Start-Sleep -Seconds 1 }
}
$result = [pscustomobject]@{ label = $Label; root_process_id = $RootProcessId; samples = $rows }
$json = ($result | ConvertTo-Json -Depth 8) -replace [char]13, ''
[IO.File]::WriteAllText($Output, $json + [char]10, [Text.UTF8Encoding]::new($false))
$rows | Select-Object utc, private_working_set_mib, working_set_mib, private_committed_mib, cpu_one_core_percent
