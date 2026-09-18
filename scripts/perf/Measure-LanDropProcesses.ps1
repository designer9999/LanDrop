# Passive Windows PowerShell 5.1 collector. Does not launch, close, or interact
# with LanDrop; never calls Tailscale, changes settings, or captures command lines.
[CmdletBinding()]
param(
    [ValidateRange(1, 2147483647)][int]$AppProcessId,
    [ValidateRange(10, 300)][int]$DurationSeconds = 60,
    [ValidateRange(2, 30)][int]$IntervalSeconds = 5,
    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'LanDrop\Diagnostics\Performance')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
if (-not $AppProcessId) { throw 'Supply the PID of the existing LanDrop process.' }
$appProcess = Get-Process -Id $AppProcessId
if ($appProcess.ProcessName -ne 'landrop') { throw 'The selected process is not LanDrop.' }
$appStartUtc = $appProcess.StartTime.ToUniversalTime().ToString('o')
$appPath = $appProcess.Path
$appFile = Get-Item -LiteralPath $appPath
$logicalProcessors = [Environment]::ProcessorCount

function Read-GroupSample {
    param([double]$ElapsedSeconds)
    $processRows = @(Get-CimInstance Win32_Process -Filter "Name='landrop.exe' OR Name='msedgewebview2.exe'" -Property ProcessId,ParentProcessId,Name,ExecutablePath,CreationDate,ReadTransferCount,WriteTransferCount,OtherTransferCount)
    $root = $processRows | Where-Object { $_.ProcessId -eq $AppProcessId }
    if (-not $root -or $root.CreationDate.ToUniversalTime().ToString('o') -ne $appStartUtc) {
        # CIM and .NET may round creation time differently; validate by .NET too.
        $current = Get-Process -Id $AppProcessId -ErrorAction SilentlyContinue
        if (-not $current -or $current.StartTime.ToUniversalTime().ToString('o') -ne $appStartUtc) {
            throw 'LanDrop exited or the selected PID was reused; collection stopped.'
        }
    }
    $selected = New-Object 'System.Collections.Generic.HashSet[uint32]'
    [void]$selected.Add([uint32]$AppProcessId)
    do {
        $added = $false
        foreach ($row in $processRows) {
            if ($row.Name -eq 'msedgewebview2.exe' -and $selected.Contains([uint32]$row.ParentProcessId)) {
                if ($selected.Add([uint32]$row.ProcessId)) { $added = $true }
            }
        }
    } while ($added)
    $items = @()
    foreach ($row in $processRows) {
        if (-not $selected.Contains([uint32]$row.ProcessId)) { continue }
        $process = Get-Process -Id $row.ProcessId -ErrorAction SilentlyContinue
        if (-not $process) { continue }
        $items += [pscustomobject]@{
            Pid = [int]$row.ProcessId
            ParentPid = [int]$row.ParentProcessId
            Name = $process.ProcessName
            StartUtc = $process.StartTime.ToUniversalTime().ToString('o')
            CpuSeconds = $process.TotalProcessorTime.TotalSeconds
            PrivateBytes = $process.PrivateMemorySize64
            WorkingSetBytes = $process.WorkingSet64
            Handles = $process.HandleCount
            Threads = $process.Threads.Count
            IoReadBytes = [uint64]$row.ReadTransferCount
            IoWriteBytes = [uint64]$row.WriteTransferCount
            IoOtherBytes = [uint64]$row.OtherTransferCount
        }
    }
    [pscustomobject]@{
        ElapsedSeconds = $ElapsedSeconds
        Utc = [DateTime]::UtcNow.ToString('o')
        Processes = $items
    }
}

$operatingSystem = Get-CimInstance Win32_OperatingSystem
$computer = Get-CimInstance Win32_ComputerSystem
$windowsVersion = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$processorRows = @(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
$graphicsRows = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,DriverDate,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate)
$powerScheme = (& powercfg.exe /GETACTIVESCHEME | Out-String).Trim()
Add-Type -AssemblyName System.Windows.Forms
$powerLineStatus = [System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus.ToString()
$runtimeRows = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" -Property ProcessId,ParentProcessId,ExecutablePath | Where-Object { $_.ParentProcessId -eq $AppProcessId } | ForEach-Object {
    $runtimeFile = if ($_.ExecutablePath) { Get-Item -LiteralPath $_.ExecutablePath }
    [pscustomobject]@{
        BrowserPid = [int]$_.ProcessId
        Executable = $(if ($runtimeFile) { $runtimeFile.FullName })
        ProductVersion = $(if ($runtimeFile) { $runtimeFile.VersionInfo.ProductVersion })
        FileVersion = $(if ($runtimeFile) { $runtimeFile.VersionInfo.FileVersion })
    }
})
$report = [ordered]@{
    SchemaVersion = 1
    State = 'collecting'
    StartedUtc = [DateTime]::UtcNow.ToString('o')
    DurationSecondsRequested = $DurationSeconds
    IntervalSecondsRequested = $IntervalSeconds
    Conditions = 'Passive observation of an already running shipping application; interaction, visibility, GPU load, and competing activity are not controlled. No startup timing.'
    Attribution = 'LanDrop PID plus recursive msedgewebview2 descendants in each process snapshot. No command lines or unrelated process counters are recorded. Shared/reparented WebView processes may not be fully attributable.'
    CounterNotes = 'CPU is cumulative processor seconds; normalize deltas by monotonic elapsed seconds and logical processor count. PrivateBytes is private commit. WorkingSetBytes includes shared resident pages: do not sum it as unique physical RAM. Process I/O counters are not specifically disk or network bytes. Processes born/exited between samples may be missed.'
    Environment = [ordered]@{
        WindowsCaption = $operatingSystem.Caption
        WindowsVersion = $operatingSystem.Version
        WindowsBuild = $operatingSystem.BuildNumber
        WindowsRevision = $windowsVersion.UBR
        WindowsDisplayVersion = $windowsVersion.DisplayVersion
        LastBootUtc = $operatingSystem.LastBootUpTime.ToUniversalTime().ToString('o')
        LogicalProcessors = $logicalProcessors
        PhysicalMemoryBytes = [uint64]$computer.TotalPhysicalMemory
        Processors = $processorRows
        Graphics = $graphicsRows
        DisplayNotes = 'GPU WMI mode values are reported, not verified per-window display timing. Per-monitor scaling and effective refresh rate not measured.'
        ActivePowerScheme = $powerScheme
        PowerLineStatus = $powerLineStatus
        EffectivePowerMode = 'Not measured; active power scheme is not the Windows performance overlay mode.'
        PowerShell = $PSVersionTable.PSVersion.ToString()
        Application = [ordered]@{
            Pid = $AppProcessId
            StartUtc = $appStartUtc
            Executable = $appPath
            ProductVersion = $appFile.VersionInfo.ProductVersion
            Bytes = $appFile.Length
            Sha256 = (Get-FileHash -LiteralPath $appPath -Algorithm SHA256).Hash
        }
        WebView2 = $runtimeRows
    }
    Samples = @()
}

[void][System.IO.Directory]::CreateDirectory($OutputDirectory)
$outputName = 'processes-{0}-{1}.json' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'), [Guid]::NewGuid().ToString('N').Substring(0,8)
$outputPath = Join-Path $OutputDirectory $outputName
$timer = [System.Diagnostics.Stopwatch]::StartNew()
try {
    while ($true) {
        $sampleStart = $timer.Elapsed.TotalSeconds
        $report.Samples += Read-GroupSample -ElapsedSeconds $sampleStart
        $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $outputPath -Encoding UTF8
        if ($sampleStart -ge $DurationSeconds) { break }
        $remaining = [Math]::Min($IntervalSeconds, $DurationSeconds - $timer.Elapsed.TotalSeconds)
        if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int]($remaining * 1000)) }
    }
    $report.State = 'complete'
} catch {
    $report.State = 'incomplete'
    $report['Failure'] = $_.Exception.GetType().FullName
    throw
} finally {
    $timer.Stop()
    $report['CollectorElapsedSeconds'] = $timer.Elapsed.TotalSeconds
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $outputPath -Encoding UTF8
    Write-Output $outputPath
}
