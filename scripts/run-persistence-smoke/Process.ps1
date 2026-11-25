# Process identity and readiness for the persistence smoke. The entry owns cleanup order.
# Dot-sourced to share its redaction function and daemon authorization headers.

function Test-SamePath {
    param(
        [AllowNull()][string]$Left,
        [AllowNull()][string]$Right
    )

    if ([string]::IsNullOrWhiteSpace($Left) -or [string]::IsNullOrWhiteSpace($Right)) {
        return $false
    }
    $leftFull = [System.IO.Path]::GetFullPath($Left)
    $rightFull = [System.IO.Path]::GetFullPath($Right)
    return $leftFull.Equals($rightFull, [System.StringComparison]::OrdinalIgnoreCase)
}

function Get-ProcessSnapshotById {
    param([int]$ProcessId)

    $process = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        return $null
    }
    return [pscustomobject][ordered]@{
        processId = [int]$process.ProcessId
        parentProcessId = [int]$process.ParentProcessId
        name = [string]$process.Name
        ExecutablePath = [string]$process.ExecutablePath
        commandLine = Redact-Text ([string]$process.CommandLine)
    }
}

function Get-CandidateProcessSnapshot {
    param([string[]]$ExecutablePaths)

    $paths = @($ExecutablePaths | ForEach-Object { [System.IO.Path]::GetFullPath($_) })
    $result = @()
    foreach ($process in @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)) {
        if ([string]::IsNullOrWhiteSpace([string]$process.ExecutablePath)) {
            continue
        }
        $matched = $false
        foreach ($path in $paths) {
            if (Test-SamePath -Left ([string]$process.ExecutablePath) -Right $path) {
                $matched = $true
                break
            }
        }
        if ($matched) {
            $result += [pscustomobject][ordered]@{
                processId = [int]$process.ProcessId
                parentProcessId = [int]$process.ParentProcessId
                name = [string]$process.Name
                ExecutablePath = [string]$process.ExecutablePath
                commandLine = Redact-Text ([string]$process.CommandLine)
            }
        }
    }
    return @($result | Sort-Object processId)
}

function Test-ExactProcessAlive {
    param(
        [int]$ProcessId,
        [string]$ExpectedExecutablePath
    )

    $snapshot = Get-ProcessSnapshotById -ProcessId $ProcessId
    return ($null -ne $snapshot -and (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath))
}

function Stop-ExactProcessById {
    param(
        [AllowNull()][object]$ProcessId,
        [string]$ExpectedExecutablePath
    )

    if ($null -eq $ProcessId) {
        return $true
    }
    $id = [int]$ProcessId
    $snapshot = Get-ProcessSnapshotById -ProcessId $id
    if ($null -eq $snapshot) {
        return $true
    }
    if (-not (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath)) {
        return $false
    }
    Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    $deadline = (Get-Date).AddSeconds(8)
    while ((Get-Date) -lt $deadline) {
        if ($null -eq (Get-ProcessSnapshotById -ProcessId $id)) {
            return $true
        }
        Start-Sleep -Milliseconds 100
    }
    return $false
}

function Start-IsolatedProcess {
    param(
        [string]$FilePath,
        [string]$WorkingDirectory,
        [string[]]$Arguments,
        [string]$StdoutPath,
        [string]$StderrPath,
        [hashtable]$EnvironmentValues
    )

    $oldEnvironment = @{}
    foreach ($name in $EnvironmentValues.Keys) {
        $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
        [Environment]::SetEnvironmentVariable($name, $EnvironmentValues[$name], "Process")
    }
    try {
        $start = @{
            FilePath = $FilePath
            WorkingDirectory = $WorkingDirectory
            RedirectStandardOutput = $StdoutPath
            RedirectStandardError = $StderrPath
            WindowStyle = "Hidden"
            PassThru = $true
        }
        if ($Arguments.Count -gt 0) {
            $start.ArgumentList = $Arguments
        }
        return Start-Process @start
    }
    finally {
        foreach ($name in $oldEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
        }
    }
}

function Wait-ForDaemonStatus {
    param(
        [string]$BaseUrl,
        [AllowNull()][object]$ProcessId,
        [AllowNull()][string]$ExpectedExecutablePath,
        [int]$TimeoutSeconds = 45
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ($null -ne $ProcessId -and -not [string]::IsNullOrWhiteSpace($ExpectedExecutablePath)) {
            if (-not (Test-ExactProcessAlive -ProcessId ([int]$ProcessId) -ExpectedExecutablePath $ExpectedExecutablePath)) {
                throw "Loom process $ProcessId exited before status became ready."
            }
        }
        try {
            $health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -Headers $script:DaemonAuthHeaders -TimeoutSec 2
            $status = Invoke-RestMethod -Uri "$BaseUrl/status" -Method Get -Headers $script:DaemonAuthHeaders -TimeoutSec 2
            if ([string]$health.status -eq "ok" -and [string]$status.status -eq "ready") {
                return $status
            }
        }
        catch {
        }
        Start-Sleep -Milliseconds 200
    }
    throw "Timed out waiting for Loom daemon at $BaseUrl"
}

function Wait-ForSiblingDaemon {
    param(
        [int]$DesktopPid,
        [string]$DaemonExecutablePath,
        [hashtable]$BaselinePidSet,
        [int]$TimeoutSeconds = 60
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        foreach ($candidate in @(Get-CandidateProcessSnapshot -ExecutablePaths @($DaemonExecutablePath))) {
            if ($BaselinePidSet.ContainsKey([string]$candidate.processId)) {
                continue
            }
            if ($candidate.parentProcessId -eq $DesktopPid) {
                return $candidate
            }
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for the desktop runtime loom-daemon.exe sidecar"
}
