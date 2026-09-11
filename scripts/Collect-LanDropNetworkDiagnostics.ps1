#requires -Version 5.1
<#
.SYNOPSIS
Captures local network diagnostics without controlling LanDrop or any VPN.
.DESCRIPTION
Run explicitly while the connection problem is happening. Results stay on this
computer. No Tailscale CLI, LocalAPI, log files, packet capture, or network changes.
#>
[CmdletBinding()]
param(
    [ValidateRange(10, 120)]
    [int]$DurationSeconds = 120,

    [ValidateRange(1, 30)]
    [int]$SampleIntervalSeconds = 10,

    [string]$OutputDirectory = (Join-Path $env:LOCALAPPDATA 'LanDrop\Diagnostics')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$captureStarted = [DateTime]::UtcNow
$captureTimer = [System.Diagnostics.Stopwatch]::StartNew()
$captureRecords = New-Object 'System.Collections.Generic.List[object]'
$captureFailed = $false
$outputFolder = [System.IO.Path]::GetFullPath($OutputDirectory)
[void][System.IO.Directory]::CreateDirectory($outputFolder)
$fileName = 'network-{0}-{1}.json' -f $captureStarted.ToString('yyyyMMddTHHmmssfffZ'), ([Guid]::NewGuid().ToString('N').Substring(0, 8))
$outputPath = Join-Path $outputFolder $fileName
$powershellPath = Join-Path $PSHOME 'powershell.exe'
$curlCommand = Get-Command 'curl.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$curlPath = if ($null -ne $curlCommand) { $curlCommand.Source } else { $null }

function Get-RemainingMilliseconds {
    return [Math]::Max(0, [int](($DurationSeconds * 1000) - $captureTimer.ElapsedMilliseconds))
}

function Add-CaptureRecord {
    param([string]$Kind, [string]$Status, [object]$Data)
    $captureRecords.Add([ordered]@{
        timestampUtc = [DateTime]::UtcNow.ToString('o')
        kind = $Kind
        status = $Status
        data = $Data
    })
}

function Save-CaptureReport {
    param([ValidateSet('in_progress', 'complete', 'failed')][string]$CompletionStatus)
    $report = [ordered]@{
        schemaVersion = 1
        completionStatus = $CompletionStatus
        startedUtc = $captureStarted.ToString('o')
        finishedUtc = if ($CompletionStatus -eq 'in_progress') { $null } else { [DateTime]::UtcNow.ToString('o') }
        requestedDurationSeconds = $DurationSeconds
        elapsedMilliseconds = $captureTimer.ElapsedMilliseconds
        powerShellVersion = $PSVersionTable.PSVersion.ToString()
        notes = @(
            'Local capture only. No upload or packet capture.'
            'No Tailscale CLI, LocalAPI, VPN controls, or log files were accessed.'
            'DNS and HTTPS checks contact only github.com and tailscale.com.'
            'DNS answers belong to the target websites, not a public-IP lookup service.'
            'A timeout may reflect diagnostic helper startup overhead as well as a network problem.'
        )
        records = $captureRecords.ToArray()
    }
    $json = $report | ConvertTo-Json -Depth 10
    $temporaryPath = $outputPath + '.tmp'
    [System.IO.File]::WriteAllText($temporaryPath, $json, (New-Object System.Text.UTF8Encoding($false)))
    if ([System.IO.File]::Exists($outputPath)) {
        # Windows PowerShell otherwise binds $null to an empty backup filename.
        [System.IO.File]::Replace($temporaryPath, $outputPath, [NullString]::Value)
    }
    else {
        [System.IO.File]::Move($temporaryPath, $outputPath)
    }
}

function Invoke-BoundedDiagnosticProcess {
    param([string]$Executable, [string]$Arguments, [int]$TimeoutMilliseconds)

    # Only this function's own diagnostic helper is terminated on timeout.
    # Existing LanDrop, Tailscale, VPN, and other application processes are untouched.
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = New-Object System.Diagnostics.ProcessStartInfo
    $process.StartInfo.FileName = $Executable
    $process.StartInfo.Arguments = $Arguments
    $process.StartInfo.UseShellExecute = $false
    $process.StartInfo.CreateNoWindow = $true
    $process.StartInfo.RedirectStandardOutput = $true
    $process.StartInfo.RedirectStandardError = $true
    $process.StartInfo.StandardOutputEncoding = New-Object System.Text.UTF8Encoding($false)
    $probeTimer = [System.Diagnostics.Stopwatch]::StartNew()
    $timedOut = $false
    $exitCode = $null
    $stdout = ''
    $failure = $null
    try {
        [void]$process.Start()
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutMilliseconds)) {
            $timedOut = $true
            $process.Kill()
            [void]$process.WaitForExit(500)
        }
        if ($process.HasExited) {
            $exitCode = $process.ExitCode
            if ($stdoutTask.Wait(250)) { $stdout = $stdoutTask.Result }
        }
        # Drain stderr to avoid blocking the child, but never record raw error text:
        # proxy configuration, paths, or other sensitive details may appear there.
        if ($stderrTask.IsCompleted) { [void]$stderrTask.Exception }
    }
    catch {
        $failure = $_.Exception.GetType().Name
    }
    finally {
        $probeTimer.Stop()
        $process.Dispose()
    }
    return [pscustomobject]@{
        timedOut = $timedOut
        exitCode = $exitCode
        elapsedMilliseconds = $probeTimer.ElapsedMilliseconds
        output = $stdout
        errorType = $failure
    }
}

function Invoke-BoundedPowerShell {
    param([string]$Source, [int]$TimeoutMilliseconds)
    $prefix = '[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); '
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($prefix + $Source))
    return Invoke-BoundedDiagnosticProcess -Executable $powershellPath -Arguments ('-NoLogo -NoProfile -NonInteractive -EncodedCommand ' + $encoded) -TimeoutMilliseconds $TimeoutMilliseconds
}

$snapshotSource = @'
$ErrorActionPreference = 'Stop'
function Emit-Snapshot {
    param([string]$Kind, [scriptblock]$Collect)
    try {
        $record = [ordered]@{ kind = $Kind; status = 'ok'; data = @(& $Collect) }
    }
    catch {
        $record = [ordered]@{ kind = $Kind; status = 'unavailable'; data = @{ errorType = $_.Exception.GetType().Name } }
    }
    [Console]::Out.WriteLine(($record | ConvertTo-Json -Depth 7 -Compress))
}
Emit-Snapshot 'processes' {
    Get-Process -Name 'landrop', 'tailscaled', 'tailscale-ipn' -ErrorAction SilentlyContinue |
        Select-Object @{Name='name'; Expression={$_.ProcessName}}, @{Name='pid'; Expression={$_.Id}}
}
Emit-Snapshot 'adapters' {
    Get-NetAdapter -ErrorAction Stop |
        Select-Object InterfaceAlias, InterfaceDescription, ifIndex, Status, LinkSpeed
}
Emit-Snapshot 'ipInterfaces' {
    Get-NetIPInterface -ErrorAction Stop |
        Select-Object InterfaceAlias, InterfaceIndex, AddressFamily, ConnectionState, InterfaceMetric, Dhcp
}
Emit-Snapshot 'defaultRoutes' {
    Get-NetRoute -ErrorAction Stop |
        Where-Object { $_.DestinationPrefix -in @('0.0.0.0/0', '0.0.0.0/1', '128.0.0.0/1', '::/0', '::/1', '8000::/1') } |
        Select-Object InterfaceAlias, InterfaceIndex, AddressFamily, DestinationPrefix, NextHop, RouteMetric, State
}
Emit-Snapshot 'dnsConfiguration' {
    Get-DnsClientServerAddress -ErrorAction Stop |
        Select-Object InterfaceAlias, InterfaceIndex, AddressFamily, ServerAddresses
}
'@

function Collect-Snapshot {
    $remaining = Get-RemainingMilliseconds
    if ($remaining -le 0) { return }
    $result = Invoke-BoundedPowerShell -Source $snapshotSource -TimeoutMilliseconds ([Math]::Min(5000, $remaining))
    $collectedKinds = New-Object 'System.Collections.Generic.List[string]'
    foreach ($line in ($result.output -split '\r?\n')) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try {
            $record = $line | ConvertFrom-Json -ErrorAction Stop
            Add-CaptureRecord -Kind $record.kind -Status $record.status -Data $record.data
            $collectedKinds.Add([string]$record.kind)
        }
        catch {
            Add-CaptureRecord -Kind 'snapshotOutput' -Status 'invalid_output' -Data @{ errorType = $_.Exception.GetType().Name }
        }
    }
    foreach ($kind in @('processes', 'adapters', 'ipInterfaces', 'defaultRoutes', 'dnsConfiguration')) {
        if (-not $collectedKinds.Contains($kind)) {
            $status = if ($result.timedOut) { 'timeout' } else { 'unavailable' }
            Add-CaptureRecord -Kind $kind -Status $status -Data @{ elapsedMilliseconds = $result.elapsedMilliseconds; errorType = $result.errorType }
        }
    }
}

function Collect-DnsProbe {
    param([ValidateSet('github.com', 'tailscale.com')][string]$TargetHost)
    $remaining = Get-RemainingMilliseconds
    if ($remaining -le 0) {
        Add-CaptureRecord -Kind 'dnsProbe' -Status 'deadline_reached' -Data @{ target = $TargetHost }
        return
    }
    $dnsSource = @'
try {
    $addresses = @([System.Net.Dns]::GetHostAddresses('TARGET_HOST') | ForEach-Object { $_.IPAddressToString })
    @{ status = 'resolved'; addresses = $addresses } | ConvertTo-Json -Compress
}
catch {
    @{ status = 'failed'; errorType = $_.Exception.GetType().Name } | ConvertTo-Json -Compress
}
'@
    $result = Invoke-BoundedPowerShell -Source $dnsSource.Replace('TARGET_HOST', $TargetHost) -TimeoutMilliseconds ([Math]::Min(2000, $remaining))
    $data = [ordered]@{ target = $TargetHost; elapsedMilliseconds = $result.elapsedMilliseconds }
    if ($result.timedOut) { $status = 'timeout' }
    elseif ($null -ne $result.errorType) { $status = 'unavailable'; $data.errorType = $result.errorType }
    else {
        try {
            $answer = $result.output | ConvertFrom-Json -ErrorAction Stop
            $status = $answer.status
            if ($status -eq 'resolved') { $data.addresses = @($answer.addresses) }
            else { $data.errorType = $answer.errorType }
        }
        catch { $status = 'invalid_output' }
    }
    Add-CaptureRecord -Kind 'dnsProbe' -Status $status -Data $data
}

function Collect-HttpsProbe {
    param([ValidateSet('github.com', 'tailscale.com')][string]$TargetHost)
    $remaining = Get-RemainingMilliseconds
    if ($remaining -le 0) {
        Add-CaptureRecord -Kind 'httpsProbe' -Status 'deadline_reached' -Data @{ target = $TargetHost }
        return
    }
    if ([string]::IsNullOrEmpty($curlPath)) {
        Add-CaptureRecord -Kind 'httpsProbe' -Status 'curl_unavailable' -Data @{ target = $TargetHost }
        return
    }
    # --disable must be first: do not load a user's .curlrc. TLS verification
    # remains enabled. No redirects, response body, credentials, or public-IP service.
    $arguments = '--disable --silent --head --output NUL --write-out "%{http_code}" --connect-timeout 2 --max-time 3 --proto "=https" --url "https://' + $TargetHost + '/"'
    $result = Invoke-BoundedDiagnosticProcess -Executable $curlPath -Arguments $arguments -TimeoutMilliseconds ([Math]::Min(3500, $remaining))
    $code = 0
    [void][int]::TryParse($result.output.Trim(), [ref]$code)
    $status = if ($result.timedOut) { 'timeout' } elseif ($result.exitCode -eq 0 -and $code -ge 100 -and $code -le 599) { 'http_response' } else { 'failed' }
    Add-CaptureRecord -Kind 'httpsProbe' -Status $status -Data @{
        target = $TargetHost
        elapsedMilliseconds = $result.elapsedMilliseconds
        httpStatus = $code
        curlExitCode = $result.exitCode
        errorType = $result.errorType
    }
}

Write-Host ('Collecting local diagnostics for up to {0} seconds. No network settings will be changed.' -f $DurationSeconds)
try {
    Save-CaptureReport -CompletionStatus 'in_progress'
    do {
        $sampleStartedAt = $captureTimer.ElapsedMilliseconds
        Collect-Snapshot
        foreach ($targetHost in @('github.com', 'tailscale.com')) {
            foreach ($probe in @('Collect-DnsProbe', 'Collect-HttpsProbe')) {
                try { & $probe -TargetHost $targetHost }
                catch { Add-CaptureRecord -Kind $probe -Status 'failed' -Data @{ target = $targetHost; errorType = $_.Exception.GetType().Name } }
            }
        }
        Save-CaptureReport -CompletionStatus 'in_progress'
        $waitMilliseconds = [Math]::Min((Get-RemainingMilliseconds), [Math]::Max(0, ($SampleIntervalSeconds * 1000) - ($captureTimer.ElapsedMilliseconds - $sampleStartedAt)))
        if ($waitMilliseconds -gt 0) { Start-Sleep -Milliseconds ([int]$waitMilliseconds) }
    } while ((Get-RemainingMilliseconds) -gt 0)
}
catch {
    $captureFailed = $true
    Add-CaptureRecord -Kind 'capture' -Status 'failed' -Data @{ errorType = $_.Exception.GetType().Name }
}
finally {
    $captureTimer.Stop()
    $completion = if ($captureFailed) { 'failed' } else { 'complete' }
    Save-CaptureReport -CompletionStatus $completion
    Write-Host ('Saved local diagnostics: {0}' -f $outputPath)
    Write-Output $outputPath
}
