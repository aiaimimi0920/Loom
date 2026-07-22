[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")

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

    Write-Utf8NoBom -Path $Path -Content (ConvertTo-Json -InputObject $Value -Depth 40)
}

function Redact-Text {
    param([AllowNull()][string]$Text)

    if ($null -eq $Text) {
        return ""
    }
    $redacted = $Text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)(LOOM_(?:DAEMON|GATEWAY)_TOKEN\s*[=:]\s*)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted.Replace("loom-concurrency-smoke-token", "<redacted>")
    return $redacted
}

function Write-RedactedFile {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (Test-Path -LiteralPath $SourcePath -PathType Leaf) {
        Write-Utf8NoBom -Path $DestinationPath -Content (Redact-Text (Get-Content -Raw -LiteralPath $SourcePath))
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
    }
}

function Wait-ForPath {
    param(
        [string]$Path,
        [int]$TimeoutSeconds,
        [AllowNull()]$Job
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            return
        }
        if ($null -ne $Job -and $Job.State -eq "Failed") {
            $jobError = (Receive-Job -Job $Job -Keep -ErrorAction SilentlyContinue | Out-String).Trim()
            throw "Fixture job failed: $jobError"
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for fixture path: $Path"
}

function Test-ExactProcessAlive {
    param(
        [int]$ProcessId,
        [string]$ExpectedExecutablePath
    )

    $snapshot = Get-ProcessSnapshotById -ProcessId $ProcessId
    return ($null -ne $snapshot -and (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath))
}

function Invoke-JsonGet {
    param(
        [string]$Uri,
        [int]$TimeoutSeconds = 15
    )

    return Invoke-RestMethod -Uri $Uri -Method Get -TimeoutSec $TimeoutSeconds
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body,
        [int]$TimeoutSeconds = 30
    )

    $json = $Body | ConvertTo-Json -Depth 40 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -ContentType "application/json" -Body $json -TimeoutSec $TimeoutSeconds
}

function Receive-JsonJob {
    param([System.Management.Automation.Job]$Job)

    $lines = @(Receive-Job -Job $Job -ErrorAction Stop | ForEach-Object { $_.ToString() })
    $text = ($lines -join [Environment]::NewLine).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "Background invoke job returned no JSON."
    }
    return $text | ConvertFrom-Json
}

function Wait-ForDaemonStatus {
    param(
        [string]$BaseUrl,
        [System.Diagnostics.Process]$Process,
        [string]$ExpectedExecutablePath,
        [int]$TimeoutSeconds = 45
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "loom-daemon exited before status became ready with code $($Process.ExitCode)."
        }
        if (-not (Test-ExactProcessAlive -ProcessId $Process.Id -ExpectedExecutablePath $ExpectedExecutablePath)) {
            throw "loom-daemon process path changed or process exited."
        }
        try {
            $health = Invoke-JsonGet -Uri "$BaseUrl/health" -TimeoutSeconds 2
            $status = Invoke-JsonGet -Uri "$BaseUrl/status" -TimeoutSeconds 2
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

function Restore-EnvironmentValue {
    param(
        [string]$Name,
        [AllowNull()][string]$Value
    )

    [Environment]::SetEnvironmentVariable($Name, $Value, "Process")
}

function Start-GatewayFixtureJob {
    param(
        [int]$Port,
        [string]$ReadyPath,
        [string]$CapturePath,
        [string]$EnteredEventName,
        [string]$ReleaseEventName
    )

    return Start-Job -ArgumentList @(
        $Port,
        $ReadyPath,
        $CapturePath,
        $EnteredEventName,
        $ReleaseEventName
    ) -ScriptBlock {
        param(
            [int]$Port,
            [string]$ReadyPath,
            [string]$CapturePath,
            [string]$EnteredEventName,
            [string]$ReleaseEventName
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $listener = $null
        $client = $null
        $stream = $null
        $enteredEvent = $null
        $releaseEvent = $null
        $encoding = [System.Text.UTF8Encoding]::new($false)
        try {
            $enteredEvent = [System.Threading.EventWaitHandle]::OpenExisting($EnteredEventName)
            $releaseEvent = [System.Threading.EventWaitHandle]::OpenExisting($ReleaseEventName)
            $listener = [System.Net.Sockets.TcpListener]::new(
                [System.Net.IPAddress]::Parse("127.0.0.1"),
                $Port
            )
            $listener.Start()
            [System.IO.File]::WriteAllText($ReadyPath, "ready", $encoding)

            $client = $listener.AcceptTcpClient()
            $stream = $client.GetStream()
            $memory = [System.IO.MemoryStream]::new()
            $buffer = New-Object byte[] 8192
            $headerEnd = -1
            $contentLength = 0
            $requestText = ""
            while ($true) {
                $count = $stream.Read($buffer, 0, $buffer.Length)
                if ($count -eq 0) {
                    break
                }
                $memory.Write($buffer, 0, $count)
                $requestText = [System.Text.Encoding]::UTF8.GetString($memory.ToArray())
                $headerEnd = $requestText.IndexOf("`r`n`r`n", [System.StringComparison]::Ordinal)
                if ($headerEnd -lt 0) {
                    continue
                }
                $headerText = $requestText.Substring(0, $headerEnd)
                foreach ($line in ($headerText -split "`r`n")) {
                    $separator = $line.IndexOf(":", [System.StringComparison]::Ordinal)
                    if ($separator -gt 0 -and $line.Substring(0, $separator).Trim().ToLowerInvariant() -eq "content-length") {
                        $contentLength = [int]$line.Substring($separator + 1).Trim()
                    }
                }
                if ($memory.Length -ge ($headerEnd + 4 + $contentLength)) {
                    break
                }
            }

            if ($headerEnd -lt 0) {
                throw "Gateway fixture received an incomplete HTTP request."
            }
            $headerText = $requestText.Substring(0, $headerEnd)
            $bodyStart = $headerEnd + 4
            $bodyText = if ($requestText.Length -ge ($bodyStart + $contentLength)) {
                $requestText.Substring($bodyStart, $contentLength)
            }
            else {
                ""
            }
            $lines = $headerText -split "`r`n"
            $requestParts = $lines[0] -split " "
            $method = if ($requestParts.Count -gt 0) { $requestParts[0] } else { "" }
            $path = if ($requestParts.Count -gt 1) { $requestParts[1] } else { "" }
            $authorization = ""
            for ($index = 1; $index -lt $lines.Count; $index++) {
                $separator = $lines[$index].IndexOf(":", [System.StringComparison]::Ordinal)
                if ($separator -gt 0 -and $lines[$index].Substring(0, $separator).Trim().ToLowerInvariant() -eq "authorization") {
                    $authorization = $lines[$index].Substring($separator + 1).Trim()
                }
            }

            $payload = if ([string]::IsNullOrWhiteSpace($bodyText)) {
                $null
            }
            else {
                $bodyText | ConvertFrom-Json
            }
            $model = if ($null -ne $payload) { [string]$payload.model } else { "" }
            $messages = @()
            if ($null -ne $payload -and $null -ne $payload.messages) {
                $messages = @($payload.messages | ForEach-Object {
                    [ordered]@{
                        role = [string]$_.role
                        content = ([string]$_.content).Replace("loom-concurrency-smoke-token", "<redacted>")
                    }
                })
            }
            $valid = (
                $method -eq "POST" -and
                $path -eq "/v1/chat/completions" -and
                $authorization -eq "Bearer loom-concurrency-smoke-token" -and
                $model -eq "concurrency-smoke" -and
                $messages.Count -ge 2
            )
            $capture = [ordered]@{
                valid = [bool]$valid
                method = $method
                path = $path
                authReceived = ($authorization -eq "Bearer loom-concurrency-smoke-token")
                model = $model
                messageRoles = @($messages | ForEach-Object { [string]$_.role })
                userContent = if ($messages.Count -ge 2) { [string]$messages[1].content } else { "" }
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($capture | ConvertTo-Json -Depth 20),
                $encoding
            )
            [void]$enteredEvent.Set()

            if (-not $releaseEvent.WaitOne(30000)) {
                throw "Gateway fixture timed out waiting for release."
            }

            if ($valid) {
                $assistantContent = '{"summary":"Concurrent packaged Gateway plan","steps":["inspect concurrent request","complete concurrent plan"]}'
                $responseObject = [ordered]@{
                    model = "concurrency-smoke-resolved"
                    choices = @(
                        [ordered]@{
                            message = [ordered]@{
                                role = "assistant"
                                content = $assistantContent
                            }
                        }
                    )
                }
                $statusLine = "200 OK"
            }
            else {
                $responseObject = [ordered]@{
                    error = [ordered]@{
                        code = "invalid_concurrency_smoke_request"
                        message = "Gateway concurrency request did not match the expected contract."
                    }
                }
                $statusLine = "400 Bad Request"
            }
            $responseJson = $responseObject | ConvertTo-Json -Depth 20 -Compress
            $responseBytes = [System.Text.Encoding]::UTF8.GetBytes($responseJson)
            $responseHeader = "HTTP/1.1 $statusLine`r`nContent-Type: application/json`r`nContent-Length: $($responseBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($responseHeader)
            $stream.Write($headerBytes, 0, $headerBytes.Length)
            $stream.Write($responseBytes, 0, $responseBytes.Length)
            $stream.Flush()
        }
        catch {
            if (-not (Test-Path -LiteralPath $CapturePath -PathType Leaf)) {
                $errorCapture = [ordered]@{
                    valid = $false
                    error = $_.Exception.Message
                }
                [System.IO.File]::WriteAllText(
                    $CapturePath,
                    ($errorCapture | ConvertTo-Json -Depth 10),
                    $encoding
                )
            }
            throw
        }
        finally {
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
            if ($null -ne $listener) { $listener.Stop() }
            if ($null -ne $enteredEvent) { $enteredEvent.Dispose() }
            if ($null -ne $releaseEvent) { $releaseEvent.Dispose() }
        }
    }
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
        $baselinePidSet[[string]$candidate.processId] = $true
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
    $summary.daemonPid = $daemonPid

    $status = Wait-ForDaemonStatus `
        -BaseUrl $daemonBaseUrl `
        -Process $daemonProcess `
        -ExpectedExecutablePath $daemonExe
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
    $invokeJob = Start-Job -ArgumentList @($daemonBaseUrl, $firstRequestJson) -ScriptBlock {
        param(
            [string]$BaseUrl,
            [string]$RequestJson
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $response = Invoke-RestMethod `
            -Uri "$BaseUrl/v1/invoke" `
            -Method Post `
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
        -TimeoutSeconds 5
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
        $daemonStopped = Stop-ExactProcessById -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe
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
            -not $baselinePidSet.ContainsKey([string]$_.processId)
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
