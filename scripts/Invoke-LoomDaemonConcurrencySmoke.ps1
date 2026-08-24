[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")
$script:DaemonAuthHeaders = @{}
$concurrencyModuleRoot = Join-Path $PSScriptRoot "daemon-concurrency-smoke"
@(
    "Common.ps1"
    "Process.ps1"
    "Http.ps1"
    "GatewayFixture.ps1"
) | ForEach-Object {
    . (Join-Path $concurrencyModuleRoot $_)
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptRoot "LoomReleaseLayout.ps1")
$loomRoot = Split-Path -Parent $scriptRoot
$repoRoot = $loomRoot
$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
$defaultEvidenceRoot = Join-Path $loomRoot "target\runtime-smoke\daemon-concurrency"
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
New-Item -ItemType Directory -Force -Path @(
    $evidenceRunDir,
    $runtimeRoot,
    $controlPlaneRoot,
    $configurationRoot,
    $appDataRoot,
    $localAppDataRoot
) | Out-Null

$summaryPath = Join-Path $evidenceRunDir "summary.json"
$daemonStdoutPath = Join-Path $runtimeRoot "loom-daemon.stdout.log"
$daemonStderrPath = Join-Path $runtimeRoot "loom-daemon.stderr.log"
$gatewayReadyPath = Join-Path $runtimeRoot "gateway.ready"
$gatewayCapturePath = Join-Path $runtimeRoot "gateway-request.json"
$daemonExe = $null
$daemonProcess = $null
$daemonPid = $null
$daemonStartTimeUtcTicks = $null
$gatewayJob = $null
$invokeJob = $null
$enteredEvent = $null
$releaseEvent = $null
$gatewayPort = $null
$daemonPort = $null
$gatewayBaseUrl = $null
$daemonBaseUrl = $null
$status = $null
$health = $null
$gatewayInvoke = $null
$gatewayRun = $null
$gatewayEvents = $null
$secondInvoke = $null
$secondRun = $null
$secondEvents = $null
$gatewayEventKinds = @()
$secondEventKinds = @()
$gatewayRequestEntered = $false
$probeCompletedBeforeGatewayRelease = $false
$secondCapabilityCompletedBeforeGatewayRelease = $false
$candidateProcessesAfterCleanup = @()
$daemonStopped = $false
$gatewayJobStopped = $false
$invokeJobStopped = $false
$baselinePidSet = @{}
$candidatePaths = @()
$failure = $null
$cleanupErrors = @()
$startedAt = (Get-Date).ToString("o")
$enteredEventName = "LoomConcurrencyEntered$([Guid]::NewGuid().ToString('N'))"
$releaseEventName = "LoomConcurrencyRelease$([Guid]::NewGuid().ToString('N'))"
$environmentNames = @(
    "LOOM_DAEMON_HOST",
    "LOOM_DAEMON_PORT",
    "LOOM_DAEMON_TOKEN",
    "LOOM_DAEMON_WORKERS",
    "LOOM_DAEMON_QUEUE_CAPACITY",
    "LOOM_CONTROL_PLANE_ROOT",
    "LOOM_CONFIGURATION_ROOT",
    "LOOM_RUN_STORE_PATH",
    "LOOM_CAPABILITY_MANIFEST_DIR",
    "LOOM_GATEWAY_MODEL",
    "LOOM_GATEWAY_BASE_URL",
    "LOOM_GATEWAY_TOKEN",
    "LOOM_GATEWAY_TIMEOUT_SECS",
    "LOOM_DAEMON_URL",
    "APPDATA",
    "LOCALAPPDATA"
)
$oldEnvironment = @{}
foreach ($environmentName in $environmentNames) {
    $oldEnvironment[$environmentName] = [Environment]::GetEnvironmentVariable($environmentName, "Process")
}

$summary = [ordered]@{
    schemaVersion = 1
    status = "running"
    packageDir = $packageFullPath
    requestExecutorMode = $null
    workers = $null
    queueCapacity = $null
    gatewayRequestEntered = $gatewayRequestEntered
    probeCompletedBeforeGatewayRelease = $probeCompletedBeforeGatewayRelease
    secondCapabilityCompletedBeforeGatewayRelease = $secondCapabilityCompletedBeforeGatewayRelease
    gatewayRunStatus = $null
    gatewayEventKinds = @()
    secondRunStatus = $null
    secondEventKinds = @()
    candidateProcessesAfterCleanup = @()
    daemonStopped = $false
    gatewayJobStopped = $false
    invokeJobStopped = $false
    daemonPid = $null
    gatewayBaseUrl = $null
    daemonBaseUrl = $null
    evidenceDir = $evidenceRunDir
    startedAt = $startedAt
    finishedAt = $null
    cleanupErrors = @()
    error = $null
}

try {
    $packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
    $summary.packageDir = $packageFullPath
    $layout = Get-LoomReleaseLayout -PackageDir $packageFullPath
    $daemonExe = $layout.daemonExe

    $candidatePaths = @($daemonExe)
    $baselineCandidates = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    foreach ($candidate in $baselineCandidates) {
        $baselineKey = "{0}:{1}" -f $candidate.processId, $candidate.startTimeUtcTicks
        $baselinePidSet[$baselineKey] = $true
    }
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "processes-baseline.json") -Value $baselineCandidates

    $gatewayPort = Get-LoomSmokePort
    do {
        $daemonPort = Get-LoomSmokePort
    } while ($daemonPort -eq $gatewayPort)
    $gatewayBaseUrl = "http://127.0.0.1:$gatewayPort"
    $daemonBaseUrl = "http://127.0.0.1:$daemonPort"
    $summary.gatewayBaseUrl = $gatewayBaseUrl
    $summary.daemonBaseUrl = $daemonBaseUrl

    $enteredEvent = [System.Threading.EventWaitHandle]::new(
        $false,
        [System.Threading.EventResetMode]::ManualReset,
        $enteredEventName
    )
    $releaseEvent = [System.Threading.EventWaitHandle]::new(
        $false,
        [System.Threading.EventResetMode]::ManualReset,
        $releaseEventName
    )
    $gatewayJob = Start-GatewayFixtureJob `
        -Port $gatewayPort `
        -ReadyPath $gatewayReadyPath `
        -CapturePath $gatewayCapturePath `
        -EnteredEventName $enteredEventName `
        -ReleaseEventName $releaseEventName
    Wait-ForPath -Path $gatewayReadyPath -TimeoutSeconds 15 -Job $gatewayJob

    $daemonEnvironment = @{
        LOOM_DAEMON_HOST = "127.0.0.1"
        LOOM_DAEMON_PORT = [string]$daemonPort
        LOOM_DAEMON_TOKEN = $null
        LOOM_DAEMON_WORKERS = "2"
        LOOM_DAEMON_QUEUE_CAPACITY = "4"
        LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $configurationRoot
        LOOM_RUN_STORE_PATH = $null
        LOOM_CAPABILITY_MANIFEST_DIR = $null
        LOOM_GATEWAY_MODEL = "concurrency-smoke"
        LOOM_GATEWAY_BASE_URL = $gatewayBaseUrl
        LOOM_GATEWAY_TOKEN = "loom-concurrency-smoke-token"
        LOOM_GATEWAY_TIMEOUT_SECS = "30"
        LOOM_DAEMON_URL = $null
        APPDATA = $appDataRoot
        LOCALAPPDATA = $localAppDataRoot
    }
    $daemonProcess = Start-IsolatedProcess `
        -FilePath $daemonExe `
        -WorkingDirectory $packageFullPath `
        -StdoutPath $daemonStdoutPath `
        -StderrPath $daemonStderrPath `
        -EnvironmentValues $daemonEnvironment
    $daemonPid = [int]$daemonProcess.Id
    $daemonStartTimeUtcTicks = $daemonProcess.StartTime.ToUniversalTime().Ticks
    $summary.daemonPid = $daemonPid

    $daemonTokenPath = Join-Path $controlPlaneRoot "daemon-token"
    $tokenDeadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path -LiteralPath $daemonTokenPath -PathType Leaf)) {
        $daemonProcess.Refresh()
        if ($daemonProcess.HasExited) {
            throw "loom-daemon exited before writing its administrator token with code $($daemonProcess.ExitCode)."
        }
        if ((Get-Date) -ge $tokenDeadline) {
            throw "Timed out waiting for Loom daemon administrator token."
        }
        Start-Sleep -Milliseconds 100
    }
    $daemonToken = [System.IO.File]::ReadAllText($daemonTokenPath, [System.Text.Encoding]::UTF8).Trim()
    if ([string]::IsNullOrWhiteSpace($daemonToken)) {
        throw "Loom daemon administrator token file was empty."
    }
    $script:DaemonAuthHeaders = @{ Authorization = "Bearer $daemonToken" }

    $status = Wait-ForDaemonStatus `
        -BaseUrl $daemonBaseUrl `
        -Process $daemonProcess `
        -ExpectedExecutablePath $daemonExe `
        -ExpectedStartTimeUtcTicks $daemonStartTimeUtcTicks
    Assert-Equal "bounded_workers" ([string]$status.requestExecutor.mode) "Request executor mode mismatch."
    Assert-Equal 2 ([int]$status.requestExecutor.workers) "Request worker count mismatch."
    Assert-Equal 4 ([int]$status.requestExecutor.queueCapacity) "Request queue capacity mismatch."

    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "status-before-concurrency.json") -Value $status

    $firstRequest = [ordered]@{
        requestId = "loom-concurrency-gateway"
        caller = "packaged-concurrency-smoke"
        capability = "brain.plan"
        input = [ordered]@{
            goal = "Verify bounded packaged Gateway concurrency"
            constraints = @("keep the first Gateway call blocked", "preserve run evidence")
            context = [ordered]@{ source = "loom-concurrency-smoke" }
        }
    }
    $firstRequestJson = $firstRequest | ConvertTo-Json -Depth 30 -Compress
    $invokeJob = Start-Job -ArgumentList @($daemonBaseUrl, $firstRequestJson, $daemonToken) -ScriptBlock {
        param(
            [string]$BaseUrl,
            [string]$RequestJson,
            [string]$DaemonToken
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $response = Invoke-RestMethod `
            -Uri "$BaseUrl/v1/invoke" `
            -Method Post `
            -Headers @{ Authorization = "Bearer $DaemonToken" } `
            -ContentType "application/json" `
            -Body $RequestJson `
            -TimeoutSec 45
        $response | ConvertTo-Json -Depth 40 -Compress
    }

    $gatewayRequestEntered = $enteredEvent.WaitOne(15000)
    $summary.gatewayRequestEntered = $gatewayRequestEntered
    Assert-True $gatewayRequestEntered "Gateway fixture did not observe the first request."

    $health = Invoke-JsonGet -Uri "$daemonBaseUrl/health" -TimeoutSeconds 3
    $probeStatus = Invoke-JsonGet -Uri "$daemonBaseUrl/status" -TimeoutSeconds 3
    Assert-Equal "ok" ([string]$health.status) "Concurrent health probe failed."
    Assert-Equal "ready" ([string]$probeStatus.status) "Concurrent status probe failed."
    Assert-Equal "bounded_workers" ([string]$probeStatus.requestExecutor.mode) "Probe status executor mode mismatch."
    Assert-Equal 2 ([int]$probeStatus.requestExecutor.workers) "Probe status worker count mismatch."
    Assert-Equal 4 ([int]$probeStatus.requestExecutor.queueCapacity) "Probe status queue capacity mismatch."
    $probeCompletedBeforeGatewayRelease = $true
    $summary.probeCompletedBeforeGatewayRelease = $probeCompletedBeforeGatewayRelease
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "health-before-gateway-release.json") -Value $health
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "status-during-gateway-block.json") -Value $probeStatus

    $secondRequest = [ordered]@{
        requestId = "loom-concurrency-second"
        caller = "packaged-concurrency-smoke"
        capability = "tea.ticket.decompose.v1"
        input = [ordered]@{
            ticket = [ordered]@{
                id = "loom-concurrency-ticket"
                title = "Bounded daemon concurrency smoke"
                description = "Verify an approved capability completes while Gateway planning is blocked."
            }
        }
    }
    $secondInvoke = Invoke-JsonPost `
        -Uri "$daemonBaseUrl/v1/invoke" `
        -Body $secondRequest `
        -TimeoutSeconds 15
    Assert-Equal "succeeded" ([string]$secondInvoke.status) "Second capability did not complete while Gateway was blocked."
    $secondCapabilityCompletedBeforeGatewayRelease = $true
    $summary.secondCapabilityCompletedBeforeGatewayRelease = $secondCapabilityCompletedBeforeGatewayRelease
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "second-invoke.json") -Value $secondInvoke

    [void]$releaseEvent.Set()

    $invokeWait = Wait-Job -Job $invokeJob -Timeout 45
    if ($null -eq $invokeWait) {
        throw "Timed out waiting for the blocked Gateway invoke after release."
    }
    if ($invokeJob.State -eq "Failed") {
        $invokeError = (Receive-Job -Job $invokeJob -Keep -ErrorAction SilentlyContinue | Out-String).Trim()
        throw "Gateway invoke job failed: $invokeError"
    }
    $gatewayInvoke = Receive-JsonJob -Job $invokeJob
    Remove-Job -Job $invokeJob -Force -ErrorAction SilentlyContinue
    $invokeJobStopped = $true

    Assert-Equal "succeeded" ([string]$gatewayInvoke.status) "Gateway brain.plan did not succeed after release."
    Assert-Equal "gateway" ([string]$gatewayInvoke.output.planner.source) "Gateway planner source mismatch."
    Assert-Equal "concurrency-smoke-resolved" ([string]$gatewayInvoke.output.planner.model) "Gateway planner model mismatch."
    Assert-Equal 2 @($gatewayInvoke.output.steps).Count "Gateway planner step count mismatch."
    $gatewayRunId = [string]$gatewayInvoke.output.runId
    Assert-True (-not [string]::IsNullOrWhiteSpace($gatewayRunId)) "Gateway invoke did not return a run id."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "gateway-invoke.json") -Value $gatewayInvoke

    $gatewayRun = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$gatewayRunId"
    $gatewayEvents = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$gatewayRunId/events"
    $gatewayEventKinds = @($gatewayEvents.events | ForEach-Object { [string]$_.kind })
    Assert-Equal "succeeded" ([string]$gatewayRun.status) "Gateway run status mismatch."
    Assert-Equal "run_started,capability_completed" ($gatewayEventKinds -join ",") "Gateway event order mismatch."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "gateway-run.json") -Value $gatewayRun
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "gateway-events.json") -Value $gatewayEvents

    $secondRunId = [string]$secondInvoke.output.runId
    Assert-True (-not [string]::IsNullOrWhiteSpace($secondRunId)) "Second capability did not return a run id."
    $secondRun = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$secondRunId"
    $secondEvents = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$secondRunId/events"
    $secondEventKinds = @($secondEvents.events | ForEach-Object { [string]$_.kind })
    Assert-Equal "succeeded" ([string]$secondRun.status) "Second capability run status mismatch."
    Assert-Equal "run_started,capability_completed" ($secondEventKinds -join ",") "Second capability event order mismatch."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "second-run.json") -Value $secondRun
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "second-events.json") -Value $secondEvents

    Wait-ForPath -Path $gatewayCapturePath -TimeoutSeconds 15 -Job $gatewayJob
    if ($gatewayJob.State -eq "Failed") {
        $gatewayError = (Receive-Job -Job $gatewayJob -Keep -ErrorAction SilentlyContinue | Out-String).Trim()
        throw "Gateway fixture failed: $gatewayError"
    }
    $gatewayCapture = Get-Content -Raw -LiteralPath $gatewayCapturePath | ConvertFrom-Json
    Assert-True ([bool]$gatewayCapture.valid) "Gateway fixture rejected the packaged request."
    Assert-True ([bool]$gatewayCapture.authReceived) "Gateway fixture did not receive the expected bearer token."
    Assert-Equal "POST" ([string]$gatewayCapture.method) "Gateway fixture method mismatch."
    Assert-Equal "/v1/chat/completions" ([string]$gatewayCapture.path) "Gateway fixture path mismatch."
    Assert-Equal "concurrency-smoke" ([string]$gatewayCapture.model) "Gateway fixture model mismatch."
    Assert-True (-not ([string]$gatewayCapture.userContent).Contains("loom-concurrency-smoke-token")) "Gateway capture leaked the token."
    Write-JsonEvidence -Path (Join-Path $evidenceRunDir "gateway-request.json") -Value $gatewayCapture

    $summary.requestExecutorMode = [string]$status.requestExecutor.mode
    $summary.workers = [int]$status.requestExecutor.workers
    $summary.queueCapacity = [int]$status.requestExecutor.queueCapacity
    $summary.gatewayRunStatus = [string]$gatewayRun.status
    $summary.gatewayEventKinds = $gatewayEventKinds
    $summary.secondRunStatus = [string]$secondRun.status
    $summary.secondEventKinds = $secondEventKinds
    $summary.status = "passed"
}
catch {
    $failure = $_
    $summary.status = "failed"
    $summary.error = Redact-Text $_.Exception.Message
}
finally {
    if ($null -ne $releaseEvent) {
        [void]$releaseEvent.Set()
    }

    foreach ($environmentName in $environmentNames) {
        Restore-EnvironmentValue -Name $environmentName -Value $oldEnvironment[$environmentName]
    }

    try {
        if ($null -ne $invokeJob) {
            if ($invokeJob.State -eq "Running" -or $invokeJob.State -eq "NotStarted") {
                Stop-Job -Job $invokeJob -ErrorAction SilentlyContinue
            }
            Remove-Job -Job $invokeJob -Force -ErrorAction SilentlyContinue
            $invokeJobStopped = $true
        }
    }
    catch {
        $cleanupErrors += "invoke job cleanup failed: $($_.Exception.Message)"
    }

    try {
        $daemonStopped = Stop-ExactProcessById `
            -ProcessId $daemonPid `
            -ExpectedExecutablePath $daemonExe `
            -ExpectedStartTimeUtcTicks $daemonStartTimeUtcTicks
    }
    catch {
        $cleanupErrors += "daemon cleanup failed: $($_.Exception.Message)"
        $daemonStopped = $false
    }

    try {
        if ($null -ne $gatewayJob) {
            if ($gatewayJob.State -eq "Running" -or $gatewayJob.State -eq "NotStarted") {
                $wakeClient = $null
                try {
                    $wakeClient = [System.Net.Sockets.TcpClient]::new()
                    $wakeClient.Connect("127.0.0.1", [int]$gatewayPort)
                }
                catch {
                }
                finally {
                    if ($null -ne $wakeClient) { $wakeClient.Dispose() }
                }
                [void](Wait-Job -Job $gatewayJob -Timeout 3 -ErrorAction SilentlyContinue)
                if ($gatewayJob.State -eq "Running" -or $gatewayJob.State -eq "NotStarted") {
                    Stop-Job -Job $gatewayJob -ErrorAction SilentlyContinue
                }
            }
            Remove-Job -Job $gatewayJob -Force -ErrorAction SilentlyContinue
        }
        $gatewayJobStopped = $true
    }
    catch {
        $cleanupErrors += "gateway job cleanup failed: $($_.Exception.Message)"
        $gatewayJobStopped = $false
    }

    if ($null -ne $enteredEvent) { $enteredEvent.Dispose() }
    if ($null -ne $releaseEvent) { $releaseEvent.Dispose() }
    Start-Sleep -Milliseconds 300

    if ($candidatePaths.Count -gt 0) {
        $afterCleanup = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
        $candidateProcessesAfterCleanup = @($afterCleanup | Where-Object {
            $candidateKey = "{0}:{1}" -f $_.processId, $_.startTimeUtcTicks
            -not $baselinePidSet.ContainsKey($candidateKey)
        })
        Write-JsonEvidence -Path (Join-Path $evidenceRunDir "processes-after-cleanup.json") -Value $afterCleanup
    }
    Write-RedactedFile -SourcePath $daemonStdoutPath -DestinationPath (Join-Path $evidenceRunDir "loom-daemon.stdout.log")
    Write-RedactedFile -SourcePath $daemonStderrPath -DestinationPath (Join-Path $evidenceRunDir "loom-daemon.stderr.log")
    if (Test-Path -LiteralPath $gatewayCapturePath -PathType Leaf) {
        Write-RedactedFile -SourcePath $gatewayCapturePath -DestinationPath (Join-Path $evidenceRunDir "gateway-request.redacted.json")
    }

    if ($candidateProcessesAfterCleanup.Count -gt 0) {
        $cleanupErrors += "Smoke left candidate Loom processes running after cleanup."
    }
    $cleanupErrors = @($cleanupErrors | ForEach-Object { Redact-Text ([string]$_) })
    $summary.candidateProcessesAfterCleanup = $candidateProcessesAfterCleanup
    $summary.daemonStopped = $daemonStopped
    $summary.gatewayJobStopped = $gatewayJobStopped
    $summary.invokeJobStopped = $invokeJobStopped
    $summary.cleanupErrors = $cleanupErrors
    if ($cleanupErrors.Count -gt 0) {
        $summary.status = "failed"
        $summary.error = ($cleanupErrors -join "; ")
    }
    $summary.finishedAt = (Get-Date).ToString("o")
    Write-JsonEvidence -Path $summaryPath -Value $summary
}

if ($null -ne $failure) {
    throw "Loom daemon concurrency smoke failed. Evidence: $summaryPath Error: $(Redact-Text $failure.Exception.Message)"
}
if ($summary.status -ne "passed") {
    throw "Loom daemon concurrency smoke failed. Evidence: $summaryPath Error: $summary.error"
}

$summary | ConvertTo-Json -Depth 40
