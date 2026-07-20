[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Parse("127.0.0.1"),
        0
    )
    $listener.Start()
    try {
        return [int]$listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Content
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Write-JsonEvidence {
    param(
        [string]$Path,
        [object]$Value
    )

    $json = ConvertTo-Json -InputObject $Value -Depth 40
    Write-Utf8NoBom -Path $Path -Content $json
}

function Get-FailureMessage {
    param([AllowNull()][object]$Failure)

    if ($null -eq $Failure) {
        return ""
    }
    if ($Failure -is [System.Management.Automation.ErrorRecord]) {
        return $Failure.Exception.ToString()
    }
    if ($Failure -is [System.Exception]) {
        return $Failure.ToString()
    }
    return [string]$Failure
}

function Redact-Text {
    param([AllowNull()][string]$Text)

    if ($null -eq $Text) {
        return ""
    }
    $redacted = $Text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)(LOOM_(?:DAEMON|GATEWAY)_TOKEN\s*[=:]\s*)[^\s\r\n]+', '$1<redacted>'
    return $redacted
}

function Write-RedactedFile {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (Test-Path -LiteralPath $SourcePath -PathType Leaf) {
        $content = Get-Content -Raw -LiteralPath $SourcePath
        Write-Utf8NoBom -Path $DestinationPath -Content (Redact-Text $content)
    }
}

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
            $health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -TimeoutSec 2
            $status = Invoke-RestMethod -Uri "$BaseUrl/status" -Method Get -TimeoutSec 2
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
    throw "Timed out waiting for desktop sibling loom-daemon.exe"
}

function Invoke-JsonGet {
    param([string]$Uri)

    return Invoke-RestMethod -Uri $Uri -Method Get -TimeoutSec 15
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body
    )

    $json = $Body | ConvertTo-Json -Depth 30 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -ContentType "application/json" -Body $json -TimeoutSec 30
}

function Invoke-Executable {
    param(
        [string]$FilePath,
        [string[]]$Arguments
    )

    $oldToken = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_TOKEN", "Process")
    [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $null, "Process")
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = @(& $FilePath @Arguments 2>&1 | ForEach-Object { $_.ToString() })
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $oldToken, "Process")
    }
    return [pscustomobject][ordered]@{
        exitCode = [int]$exitCode
        output = Redact-Text ($output -join [Environment]::NewLine)
    }
}

function New-DaemonEnvironment {
    param(
        [int]$Port,
        [string]$ControlPlaneRoot,
        [string]$ConfigurationRoot,
        [string]$AppDataRoot,
        [string]$LocalAppDataRoot
    )

    return @{
        LOOM_DAEMON_HOST = "127.0.0.1"
        LOOM_DAEMON_PORT = [string]$Port
        LOOM_DAEMON_TOKEN = $null
        LOOM_CONTROL_PLANE_ROOT = $ControlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $ConfigurationRoot
        LOOM_RUN_STORE_PATH = $null
        LOOM_CAPABILITY_MANIFEST_DIR = $null
        LOOM_GATEWAY_MODEL = $null
        LOOM_GATEWAY_BASE_URL = $null
        LOOM_GATEWAY_TOKEN = $null
        LOOM_GATEWAY_TIMEOUT_SECS = $null
        LOOM_DAEMON_URL = $null
        APPDATA = $AppDataRoot
        LOCALAPPDATA = $LocalAppDataRoot
    }
}

function New-DesktopEnvironment {
    param(
        [string]$DaemonUrl,
        [string]$ControlPlaneRoot,
        [string]$ConfigurationRoot,
        [string]$AppDataRoot,
        [string]$LocalAppDataRoot
    )

    return @{
        LOOM_DAEMON_HOST = $null
        LOOM_DAEMON_PORT = $null
        LOOM_DAEMON_TOKEN = $null
        LOOM_DAEMON_URL = $DaemonUrl
        LOOM_CONTROL_PLANE_ROOT = $ControlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $ConfigurationRoot
        LOOM_RUN_STORE_PATH = $null
        LOOM_CAPABILITY_MANIFEST_DIR = $null
        LOOM_GATEWAY_MODEL = $null
        LOOM_GATEWAY_BASE_URL = $null
        LOOM_GATEWAY_TOKEN = $null
        LOOM_GATEWAY_TIMEOUT_SECS = $null
        APPDATA = $AppDataRoot
        LOCALAPPDATA = $LocalAppDataRoot
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$loomRoot = Split-Path -Parent $scriptRoot
$repoRoot = $loomRoot
$packageFullPath = $PackageDir
$defaultEvidenceRoot = Join-Path $loomRoot "target\runtime-smoke"
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    $evidenceRootFullPath = $defaultEvidenceRoot
}
elseif ([System.IO.Path]::IsPathRooted($EvidenceRoot)) {
    $evidenceRootFullPath = [System.IO.Path]::GetFullPath($EvidenceRoot)
}
else {
    $evidenceRootFullPath = [System.IO.Path]::GetFullPath((Join-Path $loomRoot $EvidenceRoot))
}

$evidenceRunId = "{0}-{1}" -f (Get-Date -Format "yyyyMMdd-HHmmss"), ([Guid]::NewGuid().ToString("N").Substring(0, 8))
$evidenceRunDir = Join-Path $evidenceRootFullPath $evidenceRunId
$runtimeRoot = Join-Path $evidenceRunDir "runtime"
$controlPlaneRoot = Join-Path $runtimeRoot "control-plane"
$configurationRoot = Join-Path $runtimeRoot "configuration"
$appDataRoot = Join-Path $runtimeRoot "appdata"
$localAppDataRoot = Join-Path $runtimeRoot "localappdata"
New-Item -ItemType Directory -Force -Path $evidenceRunDir, $runtimeRoot, $controlPlaneRoot, $configurationRoot, $appDataRoot, $localAppDataRoot | Out-Null

$baselinePidSet = @{}
$loomExe = $null
$daemonExe = $null
$desktopExe = $null
$manifestPath = $null
$desktopExists = $false
$candidatePaths = @()
$firstPort = $null
$secondPort = $null
$desktopPort = $null
$firstBaseUrl = $null
$secondBaseUrl = $null
$desktopBaseUrl = $null
$databasePath = Join-Path $controlPlaneRoot "runs\loom-runs.sqlite3"
$summaryPath = Join-Path $evidenceRunDir "summary.json"

$firstStdout = Join-Path $runtimeRoot "daemon-a.stdout.log"
$firstStderr = Join-Path $runtimeRoot "daemon-a.stderr.log"
$secondStdout = Join-Path $runtimeRoot "daemon-b.stdout.log"
$secondStderr = Join-Path $runtimeRoot "daemon-b.stderr.log"
$desktopStdout = Join-Path $runtimeRoot "desktop.stdout.log"
$desktopStderr = Join-Path $runtimeRoot "desktop.stderr.log"

$firstProcess = $null
$secondProcess = $null
$desktopProcess = $null
$firstDaemonPid = $null
$secondDaemonPid = $null
$desktopPid = $null
$desktopDaemonPid = $null
$runId = $null
$firstRun = $null
$firstEvents = $null
$persistedRun = $null
$persistedEvents = $null
$cliResult = [pscustomobject]@{ exitCode = -1; output = "" }
$desktopAliveDuringAssertions = $false
$siblingParentMatched = $false
$desktopSkipped = $true
$candidateProcessesAfterCleanup = @()
$failure = $null
$cleanupErrors = @()
$startedAt = (Get-Date).ToString("o")

try {
    $packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
    $loomExe = Join-Path $packageFullPath "loom.exe"
    $daemonExe = Join-Path $packageFullPath "loom-daemon.exe"
    $desktopExe = Join-Path $packageFullPath "loom-desktop.exe"
    $manifestPath = Join-Path $packageFullPath "manifest.json"
    Assert-True (Test-Path -LiteralPath $loomExe -PathType Leaf) "Package is missing loom.exe: $loomExe"
    Assert-True (Test-Path -LiteralPath $daemonExe -PathType Leaf) "Package is missing loom-daemon.exe: $daemonExe"
    $desktopExists = Test-Path -LiteralPath $desktopExe -PathType Leaf
    $desktopSkipped = -not $desktopExists
    if (-not $desktopExists -and (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Formal Loom package is missing loom-desktop.exe: $desktopExe"
    }

    $candidatePaths = @($daemonExe)
    if ($desktopExists) {
        $candidatePaths += $desktopExe
    }
    $baselineCandidates = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    foreach ($candidate in $baselineCandidates) {
        $baselinePidSet[[string]$candidate.processId] = $true
    }
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "processes-baseline.json") -Value $baselineCandidates

    $firstPort = Get-FreeTcpPort
    do {
        $secondPort = Get-FreeTcpPort
    } while ($secondPort -eq $firstPort)
    do {
        $desktopPort = Get-FreeTcpPort
    } while ($desktopPort -eq $firstPort -or $desktopPort -eq $secondPort)

    $firstBaseUrl = "http://127.0.0.1:$firstPort"
    $secondBaseUrl = "http://127.0.0.1:$secondPort"
    $desktopBaseUrl = "http://127.0.0.1:$desktopPort"

    $daemonEnvironment = New-DaemonEnvironment `
        -Port $firstPort `
        -ControlPlaneRoot $controlPlaneRoot `
        -ConfigurationRoot $configurationRoot `
        -AppDataRoot $appDataRoot `
        -LocalAppDataRoot $localAppDataRoot
    $firstProcess = Start-IsolatedProcess `
        -FilePath $daemonExe `
        -WorkingDirectory $packageFullPath `
        -Arguments @() `
        -StdoutPath $firstStdout `
        -StderrPath $firstStderr `
        -EnvironmentValues $daemonEnvironment
    $firstDaemonPid = [int]$firstProcess.Id
    $firstStatus = Wait-ForDaemonStatus `
        -BaseUrl $firstBaseUrl `
        -ProcessId $firstDaemonPid `
        -ExpectedExecutablePath $daemonExe
    Assert-Equal "sqlite" ([string]$firstStatus.run_store.mode) "Daemon A run store mode mismatch."
    Assert-Equal $true ([bool]$firstStatus.run_store.persistent) "Daemon A run store persistence mismatch."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "daemon-a-status.json") -Value $firstStatus

    $invoke = Invoke-JsonPost -Uri "$firstBaseUrl/v1/invoke" -Body ([ordered]@{
        requestId = "run-persistence-smoke"
        caller = "release-smoke"
        capability = "brain.plan"
        input = [ordered]@{
            goal = "Persist packaged Loom run evidence across daemon restart"
            constraints = @("do not replay interrupted side effects")
        }
    })
    Assert-Equal "succeeded" ([string]$invoke.status) "Daemon A brain.plan invoke failed."
    $runId = [string]$invoke.output.runId
    Assert-True (-not [string]::IsNullOrWhiteSpace($runId)) "Daemon A invoke returned no runId."
    $firstRun = Invoke-JsonGet -Uri "$firstBaseUrl/v1/runs/$runId"
    $firstEvents = Invoke-JsonGet -Uri "$firstBaseUrl/v1/runs/$runId/events"
    Assert-Equal "succeeded" ([string]$firstRun.status) "Daemon A stored run status mismatch."
    Assert-Equal 2 @($firstEvents.events).Count "Daemon A event count mismatch."
    Assert-True ([int64]$firstEvents.events[0].sequence -lt [int64]$firstEvents.events[1].sequence) "Daemon A events are not ordered."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "daemon-a-invoke.json") -Value $invoke
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "daemon-a-run.json") -Value $firstRun
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "daemon-a-events.json") -Value $firstEvents

    Assert-True (Stop-ExactProcessById -ProcessId $firstDaemonPid -ExpectedExecutablePath $daemonExe) "Failed to stop daemon A by exact PID/path."
    $firstProcess = $null
    Assert-True (Test-Path -LiteralPath $databasePath -PathType Leaf) "Default run database was not created: $databasePath"

    $daemonEnvironment = New-DaemonEnvironment `
        -Port $secondPort `
        -ControlPlaneRoot $controlPlaneRoot `
        -ConfigurationRoot $configurationRoot `
        -AppDataRoot $appDataRoot `
        -LocalAppDataRoot $localAppDataRoot
    $secondProcess = Start-IsolatedProcess `
        -FilePath $daemonExe `
        -WorkingDirectory $packageFullPath `
        -Arguments @() `
        -StdoutPath $secondStdout `
        -StderrPath $secondStderr `
        -EnvironmentValues $daemonEnvironment
    $secondDaemonPid = [int]$secondProcess.Id
    $secondStatus = Wait-ForDaemonStatus `
        -BaseUrl $secondBaseUrl `
        -ProcessId $secondDaemonPid `
        -ExpectedExecutablePath $daemonExe
    Assert-Equal "sqlite" ([string]$secondStatus.run_store.mode) "Daemon B run store mode mismatch."
    Assert-Equal $true ([bool]$secondStatus.run_store.persistent) "Daemon B run store persistence mismatch."
    $persistedRun = Invoke-JsonGet -Uri "$secondBaseUrl/v1/runs/$runId"
    $persistedEvents = Invoke-JsonGet -Uri "$secondBaseUrl/v1/runs/$runId/events"
    Assert-Equal "succeeded" ([string]$persistedRun.status) "Persisted run status mismatch."
    Assert-Equal ($firstRun | ConvertTo-Json -Depth 40 -Compress) ($persistedRun | ConvertTo-Json -Depth 40 -Compress) "Persisted run JSON changed after restart."
    Assert-Equal ($firstEvents | ConvertTo-Json -Depth 40 -Compress) ($persistedEvents | ConvertTo-Json -Depth 40 -Compress) "Persisted events changed after restart."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "daemon-b-status.json") -Value $secondStatus
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "persisted-run.json") -Value $persistedRun
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "persisted-events.json") -Value $persistedEvents

    $cliResult = Invoke-Executable -FilePath $loomExe -Arguments @("status", "--daemon-url", $secondBaseUrl)
    Assert-Equal 0 $cliResult.exitCode "loom.exe status failed against daemon B."
    Assert-True ([string]$cliResult.output -match '"status"\s*:\s*"ready"') "loom.exe status output did not report ready."
    Write-Utf8NoBom -Path (Join-Path $evidenceRunDir "loom-status.txt") -Content ([string]$cliResult.output)

    Assert-True (Stop-ExactProcessById -ProcessId $secondDaemonPid -ExpectedExecutablePath $daemonExe) "Failed to stop daemon B by exact PID/path."
    $secondProcess = $null

    if (-not $desktopSkipped) {
        $desktopEnvironment = New-DesktopEnvironment `
            -DaemonUrl $desktopBaseUrl `
            -ControlPlaneRoot $controlPlaneRoot `
            -ConfigurationRoot $configurationRoot `
            -AppDataRoot $appDataRoot `
            -LocalAppDataRoot $localAppDataRoot
        $desktopProcess = Start-IsolatedProcess `
            -FilePath $desktopExe `
            -WorkingDirectory $packageFullPath `
            -Arguments @() `
            -StdoutPath $desktopStdout `
            -StderrPath $desktopStderr `
            -EnvironmentValues $desktopEnvironment
        $desktopPid = [int]$desktopProcess.Id
        $sibling = Wait-ForSiblingDaemon `
            -DesktopPid $desktopPid `
            -DaemonExecutablePath $daemonExe `
            -BaselinePidSet $baselinePidSet
        $desktopDaemonPid = [int]$sibling.processId
        $siblingParentMatched = ($sibling.parentProcessId -eq $desktopPid)
        Assert-True $siblingParentMatched "Desktop daemon parent PID does not match desktop PID."
        $desktopStatus = Wait-ForDaemonStatus `
            -BaseUrl $desktopBaseUrl `
            -ProcessId $desktopDaemonPid `
            -ExpectedExecutablePath $daemonExe `
            -TimeoutSeconds 45
        Assert-Equal "sqlite" ([string]$desktopStatus.run_store.mode) "Desktop sibling run store mode mismatch."
        Assert-Equal $true ([bool]$desktopStatus.run_store.persistent) "Desktop sibling run store persistence mismatch."
        $desktopPersistedRun = Invoke-JsonGet -Uri "$desktopBaseUrl/v1/runs/$runId"
        Assert-Equal "succeeded" ([string]$desktopPersistedRun.status) "Desktop sibling did not reuse persisted run evidence."
        $desktopAliveDuringAssertions = Test-ExactProcessAlive -ProcessId $desktopPid -ExpectedExecutablePath $desktopExe
        Assert-True $desktopAliveDuringAssertions "loom-desktop.exe exited during assertions."
        Write-JsonEvidence -Path (Join-Path $evidenceRunDir "desktop-status.json") -Value $desktopStatus
        Write-JsonEvidence -Path (Join-Path $evidenceRunDir "desktop-persisted-run.json") -Value $desktopPersistedRun
        Write-JsonEvidence -Path (Join-Path $evidenceRunDir "processes-desktop.json") -Value @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    }
}
catch {
    $failure = $_
}
finally {
    if ($null -ne $desktopDaemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $desktopDaemonPid -ExpectedExecutablePath $daemonExe)) {
            $cleanupErrors += "desktop daemon cleanup failed"
        }
    }
    if ($null -ne $desktopPid) {
        if (-not (Stop-ExactProcessById -ProcessId $desktopPid -ExpectedExecutablePath $desktopExe)) {
            $cleanupErrors += "desktop cleanup failed"
        }
    }
    if ($null -ne $secondDaemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $secondDaemonPid -ExpectedExecutablePath $daemonExe)) {
            $cleanupErrors += "daemon B cleanup failed"
        }
    }
    if ($null -ne $firstDaemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $firstDaemonPid -ExpectedExecutablePath $daemonExe)) {
            $cleanupErrors += "daemon A cleanup failed"
        }
    }
    Start-Sleep -Milliseconds 400

    $afterCleanup = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    $candidateProcessesAfterCleanup = @($afterCleanup | Where-Object { -not $baselinePidSet.ContainsKey([string]$_.processId) })
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "processes-after-cleanup.json") -Value $afterCleanup
    Write-RedactedFile -SourcePath $firstStdout -DestinationPath (Join-Path $evidenceRunDir "daemon-a.stdout.log")
    Write-RedactedFile -SourcePath $firstStderr -DestinationPath (Join-Path $evidenceRunDir "daemon-a.stderr.log")
    Write-RedactedFile -SourcePath $secondStdout -DestinationPath (Join-Path $evidenceRunDir "daemon-b.stdout.log")
    Write-RedactedFile -SourcePath $secondStderr -DestinationPath (Join-Path $evidenceRunDir "daemon-b.stderr.log")
    Write-RedactedFile -SourcePath $desktopStdout -DestinationPath (Join-Path $evidenceRunDir "desktop.stdout.log")
    Write-RedactedFile -SourcePath $desktopStderr -DestinationPath (Join-Path $evidenceRunDir "desktop.stderr.log")

    if ($cleanupErrors.Count -gt 0 -and $null -eq $failure) {
        $failure = [System.Exception]::new(($cleanupErrors -join "; "))
    }
    if ($candidateProcessesAfterCleanup.Count -gt 0 -and $null -eq $failure) {
        $failure = [System.Exception]::new("Smoke left candidate Loom processes running after cleanup.")
    }

    $firstEventSequences = if ($null -ne $firstEvents) {
        @($firstEvents.events | ForEach-Object { [int64]$_.sequence })
    }
    else {
        @()
    }
    $persistedEventSequences = if ($null -ne $persistedEvents) {
        @($persistedEvents.events | ForEach-Object { [int64]$_.sequence })
    }
    else {
        @()
    }
    $summary = [ordered]@{
        schemaVersion = 1
        status = if ($null -eq $failure) { "passed" } else { "failed" }
        packageDir = $packageFullPath
        databasePath = $databasePath
        firstDaemonPid = $firstDaemonPid
        secondDaemonPid = $secondDaemonPid
        runId = $runId
        firstEventSequences = $firstEventSequences
        persistedEventSequences = $persistedEventSequences
        persistedStatus = if ($null -ne $persistedRun) { [string]$persistedRun.status } else { $null }
        cliExitCode = [int]$cliResult.exitCode
        desktopPid = $desktopPid
        desktopDaemonPid = $desktopDaemonPid
        desktopSkipped = [bool]$desktopSkipped
        desktopAliveDuringAssertions = [bool]$desktopAliveDuringAssertions
        siblingParentMatched = [bool]$siblingParentMatched
        candidateProcessesAfterCleanup = $candidateProcessesAfterCleanup
        evidenceDir = $evidenceRunDir
        controlPlaneRoot = $controlPlaneRoot
        firstDaemonUrl = $firstBaseUrl
        secondDaemonUrl = $secondBaseUrl
        desktopDaemonUrl = $desktopBaseUrl
        startedAt = $startedAt
        finishedAt = (Get-Date).ToString("o")
        cleanupErrors = $cleanupErrors
        error = if ($null -ne $failure) { Redact-Text (Get-FailureMessage $failure) } else { $null }
    }
    Write-JsonEvidence -Path $summaryPath -Value $summary
}

if ($null -ne $failure) {
    throw "Loom run persistence smoke failed. Evidence: $summaryPath Error: $(Redact-Text (Get-FailureMessage $failure))"
}

Write-Host "Loom run persistence smoke passed."
Write-Host "Evidence: $summaryPath"
