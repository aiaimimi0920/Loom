<# Owns exact process identity, bounded cleanup, and isolated daemon launch helpers. #>

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
    $runtimeProcess = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    $startTimeUtcTicks = $null
    if ($null -ne $runtimeProcess) {
        try {
            $startTimeUtcTicks = $runtimeProcess.StartTime.ToUniversalTime().Ticks
        }
        catch {
        }
    }
    return [pscustomobject][ordered]@{
        processId = [int]$process.ProcessId
        parentProcessId = [int]$process.ParentProcessId
        name = [string]$process.Name
        ExecutablePath = [string]$process.ExecutablePath
        startTimeUtcTicks = $startTimeUtcTicks
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
            $snapshot = Get-ProcessSnapshotById -ProcessId ([int]$process.ProcessId)
            if ($null -ne $snapshot) {
                $result += $snapshot
            }
        }
    }
    return @($result | Sort-Object processId)
}

function Stop-ExactProcessById {
    param(
        [AllowNull()][object]$ProcessId,
        [string]$ExpectedExecutablePath,
        [AllowNull()][object]$ExpectedStartTimeUtcTicks
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
    if ($null -ne $ExpectedStartTimeUtcTicks -and $snapshot.startTimeUtcTicks -ne [long]$ExpectedStartTimeUtcTicks) {
        return $false
    }
    Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    $deadline = (Get-Date).AddSeconds(8)
    while ((Get-Date) -lt $deadline) {
        $current = Get-ProcessSnapshotById -ProcessId $id
        if ($null -eq $current) {
            return $true
        }
        if ($null -ne $ExpectedStartTimeUtcTicks -and $current.startTimeUtcTicks -ne [long]$ExpectedStartTimeUtcTicks) {
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
        [string]$StdoutPath,
        [string]$StderrPath,
        [hashtable]$EnvironmentValues
    )

    $environmentMutex = [System.Threading.Mutex]::new($false, "Local\LoomConcurrencySmokeEnvironmentMutation")
    $mutexAcquired = $false
    $oldEnvironment = @{}
    try {
        try {
            $mutexAcquired = $environmentMutex.WaitOne([TimeSpan]::FromSeconds(30))
        }
        catch [System.Threading.AbandonedMutexException] {
            $mutexAcquired = $true
        }
        if (-not $mutexAcquired) {
            throw "Timed out waiting to isolate daemon environment inheritance."
        }
        foreach ($name in $EnvironmentValues.Keys) {
            $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
            [Environment]::SetEnvironmentVariable($name, $EnvironmentValues[$name], "Process")
        }
        return Start-Process `
            -FilePath $FilePath `
            -WorkingDirectory $WorkingDirectory `
            -RedirectStandardOutput $StdoutPath `
            -RedirectStandardError $StderrPath `
            -WindowStyle Hidden `
            -PassThru
    }
    finally {
        foreach ($name in $oldEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
        }
        if ($mutexAcquired) {
            $environmentMutex.ReleaseMutex()
        }
        $environmentMutex.Dispose()
    }
}
