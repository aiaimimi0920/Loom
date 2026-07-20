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

    Write-Utf8NoBom -Path $Path -Content ($Value | ConvertTo-Json -Depth 30)
}

function Redact-Text {
    param([AllowNull()][string]$Text)

    if ($null -eq $Text) {
        return ""
    }
    $redacted = $Text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted.Replace("smoke-token", "<redacted>")
    $redacted = $redacted.Replace("failure-secret", "<redacted>")
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

function Restore-EnvironmentValue {
    param(
        [string]$Name,
        [AllowNull()][string]$Value
    )

    if ($null -eq $Value) {
        Remove-Item -LiteralPath "Env:$Name" -ErrorAction SilentlyContinue
    }
    else {
        Set-Item -LiteralPath "Env:$Name" -Value $Value
    }
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string]$WorkingDirectory,
        [string]$StdoutPath,
        [string]$StderrPath
    )

    return Start-Process `
        -FilePath $FilePath `
        -WorkingDirectory $WorkingDirectory `
        -RedirectStandardOutput $StdoutPath `
        -RedirectStandardError $StderrPath `
        -WindowStyle Hidden `
        -PassThru
}

function Stop-SmokeProcess {
    param([AllowNull()][System.Diagnostics.Process]$Process)

    if ($null -eq $Process) {
        return $true
    }
    try {
        $Process.Refresh()
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            [void]$Process.WaitForExit(5000)
        }
        $Process.Refresh()
        return [bool]$Process.HasExited
    }
    catch {
        return $false
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
            throw "Gateway fixture job failed: $jobError"
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for fixture file: $Path"
}

function Wait-ForHealth {
    param(
        [string]$BaseUrl,
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutSeconds = 30
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            $exitCode = $Process.ExitCode
            throw "loom-daemon exited before health check with code $exitCode"
        }
        try {
            $health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -TimeoutSec 2
            if ([string]$health.status -eq "ok") {
                return $health
            }
        }
        catch {
            Start-Sleep -Milliseconds 200
        }
    }
    throw "Timed out waiting for Loom health at $BaseUrl"
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

    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = @(& $FilePath @Arguments 2>&1 | ForEach-Object { $_.ToString() })
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    return [ordered]@{
        exitCode = $exitCode
        output = ($output -join [Environment]::NewLine)
    }
}

function Get-JsonFile {
    param([string]$Path)

    return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptRoot "LoomReleaseLayout.ps1")
$repoRoot = Split-Path -Parent $scriptRoot
$packageFullPath = (Resolve-Path -LiteralPath $PackageDir).Path
$defaultEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\gateway"
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    $evidenceRootFullPath = $defaultEvidenceRoot
}
elseif ([System.IO.Path]::IsPathRooted($EvidenceRoot)) {
    $evidenceRootFullPath = [System.IO.Path]::GetFullPath($EvidenceRoot)
}
else {
    $evidenceRootFullPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $EvidenceRoot))
}

$runId = "{0}-{1}" -f (Get-Date -Format "yyyyMMdd-HHmmss"), ([Guid]::NewGuid().ToString("N").Substring(0, 8))
$evidenceRunDir = Join-Path $evidenceRootFullPath $runId
$tempRoot = Join-Path $env:TEMP "loom-gateway-brain-plan-smoke-$PID-$runId"
New-Item -ItemType Directory -Force -Path $evidenceRunDir, $tempRoot | Out-Null

$layout = Get-LoomReleaseLayout -PackageDir $packageFullPath -CliExtractRoot (Join-Path $evidenceRunDir "cli")
$loomExe = $layout.cliExe
$daemonExe = $layout.daemonExe

$gatewayPort = Get-FreeTcpPort
do {
    $daemonPort = Get-FreeTcpPort
} while ($daemonPort -eq $gatewayPort)
$gatewayBaseUrl = "http://127.0.0.1:$gatewayPort"
$daemonBaseUrl = "http://127.0.0.1:$daemonPort"

$gatewayReadyPath = Join-Path $tempRoot "gateway.ready"
$gatewayCapturePath = Join-Path $tempRoot "gateway.capture.json"
$daemonStdoutPath = Join-Path $tempRoot "loom-daemon.stdout.log"
$daemonStderrPath = Join-Path $tempRoot "loom-daemon.stderr.log"
$controlPlaneRoot = Join-Path $tempRoot "control-plane"
$configurationRoot = Join-Path $tempRoot "configuration"
$logRoot = Join-Path $tempRoot "logs"

$summaryPath = Join-Path $evidenceRunDir "summary.json"
$gatewayEvidencePath = Join-Path $evidenceRunDir "gateway-request.json"
$daemonStdoutEvidencePath = Join-Path $evidenceRunDir "loom-daemon.stdout.log"
$daemonStderrEvidencePath = Join-Path $evidenceRunDir "loom-daemon.stderr.log"

$summary = [ordered]@{
    schemaVersion = 1
    status = "running"
    packageDir = $packageFullPath
    gatewayBaseUrl = $gatewayBaseUrl
    daemonBaseUrl = $daemonBaseUrl
    gatewayPort = $gatewayPort
    daemonPort = $daemonPort
    gatewayModel = "smoke-planner"
    gatewayResolvedModel = $null
    plannerSource = $null
    planSummary = $null
    planSteps = @()
    runId = $null
    runStatus = $null
    eventKinds = @()
    cliStatus = $null
    daemonPid = $null
    daemonStopped = $false
    gatewayJobStopped = $false
    evidenceRunId = $runId
    evidenceRunDir = $evidenceRunDir
    summaryPath = $summaryPath
    gatewayEvidencePath = $gatewayEvidencePath
    daemonStdoutEvidencePath = $daemonStdoutEvidencePath
    daemonStderrEvidencePath = $daemonStderrEvidencePath
    startedAt = (Get-Date).ToString("o")
    finishedAt = $null
    error = $null
}

$environmentNames = @(
    "LOOM_DAEMON_HOST",
    "LOOM_DAEMON_PORT",
    "LOOM_DAEMON_TOKEN",
    "LOOM_CONTROL_PLANE_ROOT",
    "LOOM_CONFIGURATION_ROOT",
    "LOOM_LOG_DIR",
    "LOOM_SETTINGS_BASE_URL",
    "LOOM_GATEWAY_MODEL",
    "LOOM_GATEWAY_BASE_URL",
    "LOOM_GATEWAY_TOKEN",
    "LOOM_GATEWAY_TIMEOUT_SECS"
)
$oldEnvironment = @{}
foreach ($name in $environmentNames) {
    $oldEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

$daemonProcess = $null
$gatewayJob = $null
$failure = $null
$cleanupError = $null

try {
    $loomVersion = Invoke-Executable -FilePath $loomExe -Arguments @("--version")
    Assert-Equal 0 $loomVersion.exitCode "packaged loom.exe --version failed"
    Assert-True ([string]$loomVersion.output -match "^loom ") "packaged loom.exe version output mismatch"

    $daemonVersion = Invoke-Executable -FilePath $daemonExe -Arguments @("--version")
    Assert-Equal 0 $daemonVersion.exitCode "packaged loom-daemon.exe --version failed"
    Assert-True ([string]$daemonVersion.output -match "^loom-daemon ") "packaged loom-daemon.exe version output mismatch"

    $gatewayJob = Start-Job -ArgumentList @($gatewayPort, $gatewayReadyPath, $gatewayCapturePath) -ScriptBlock {
        param(
            [int]$Port,
            [string]$ReadyPath,
            [string]$CapturePath
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $listener = $null
        $client = $null
        $stream = $null
        $encoding = [System.Text.UTF8Encoding]::new($false)
        try {
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
                throw "Gateway fixture received an incomplete HTTP request"
            }
            $headerText = $requestText.Substring(0, $headerEnd)
            $bodyStart = $headerEnd + 4
            $bodyText = if ($requestText.Length -ge ($bodyStart + $contentLength)) {
                $requestText.Substring($bodyStart, $contentLength)
            } else {
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
            $payload = $null
            try {
                if (-not [string]::IsNullOrWhiteSpace($bodyText)) {
                    $payload = $bodyText | ConvertFrom-Json
                }
            }
            catch {
                $payload = $null
            }
            $model = if ($null -ne $payload) { [string]$payload.model } else { "" }
            $messages = @()
            if ($null -ne $payload -and $null -ne $payload.messages) {
                $messages = @($payload.messages | ForEach-Object {
                    [ordered]@{
                        role = [string]$_.role
                        content = [string]$_.content
                    }
                })
            }
            $valid = (
                $method -eq "POST" -and
                $path -eq "/v1/chat/completions" -and
                $authorization -eq "Bearer smoke-token" -and
                $model -eq "smoke-planner" -and
                $messages.Count -ge 2
            )
            $userContent = if ($messages.Count -ge 2) { [string]$messages[1].content } else { "" }
            $capture = [ordered]@{
                valid = [bool]$valid
                method = $method
                path = $path
                authReceived = ($authorization -eq "Bearer smoke-token")
                model = $model
                messageRoles = @($messages | ForEach-Object { [string]$_.role })
                userContent = $userContent
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($capture | ConvertTo-Json -Depth 20),
                $encoding
            )

            if ($valid) {
                $assistantContent = '{"summary":"Packaged Gateway plan","steps":["inspect packaged request","execute packaged plan"]}'
                $responseObject = [ordered]@{
                    model = "smoke-resolved-model"
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
                        code = "invalid_smoke_request"
                        message = "Gateway smoke request did not match the expected contract"
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
            $errorCapture = [ordered]@{
                valid = $false
                error = $_.Exception.Message
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($errorCapture | ConvertTo-Json -Depth 10),
                $encoding
            )
            throw
        }
        finally {
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
            if ($null -ne $listener) { $listener.Stop() }
        }
    }
    Wait-ForPath -Path $gatewayReadyPath -TimeoutSeconds 15 -Job $gatewayJob

    $environmentValues = @{
        LOOM_DAEMON_HOST = "127.0.0.1"
        LOOM_DAEMON_PORT = [string]$daemonPort
        LOOM_DAEMON_TOKEN = ""
        LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $configurationRoot
        LOOM_LOG_DIR = $logRoot
        LOOM_SETTINGS_BASE_URL = "$daemonBaseUrl/settings"
        LOOM_GATEWAY_MODEL = "smoke-planner"
        LOOM_GATEWAY_BASE_URL = $gatewayBaseUrl
        LOOM_GATEWAY_TOKEN = "smoke-token"
        LOOM_GATEWAY_TIMEOUT_SECS = "10"
    }
    foreach ($entry in $environmentValues.GetEnumerator()) {
        Set-Item -LiteralPath "Env:$($entry.Key)" -Value $entry.Value
    }
    try {
        $daemonProcess = Start-SmokeProcess `
            -FilePath $daemonExe `
            -WorkingDirectory $packageFullPath `
            -StdoutPath $daemonStdoutPath `
            -StderrPath $daemonStderrPath
    }
    finally {
        foreach ($name in $environmentNames) {
            Restore-EnvironmentValue -Name $name -Value $oldEnvironment[$name]
        }
    }
    $summary.daemonPid = $daemonProcess.Id

    $health = Wait-ForHealth -BaseUrl $daemonBaseUrl -Process $daemonProcess
    Assert-Equal "ok" ([string]$health.status) "Loom health status mismatch"
    $status = Invoke-JsonGet -Uri "$daemonBaseUrl/status"
    Assert-Equal "ready" ([string]$status.status) "Loom status mismatch"
    Assert-Equal "gateway" ([string]$status.brain_planner.mode) "Gateway planner mode mismatch"
    Assert-Equal $true ([bool]$status.brain_planner.configured) "Gateway planner configured flag mismatch"
    Assert-Equal "smoke-planner" ([string]$status.brain_planner.model) "Gateway planner model mismatch"
    Assert-Equal 10 ([int]$status.brain_planner.timeout_seconds) "Gateway planner timeout mismatch"
    Assert-True ($null -eq $status.brain_planner.PSObject.Properties["auth_token"]) "Gateway planner status must not expose auth_token"
    $summary.cliStatus = [string]$status.status

    $oldCliToken = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_TOKEN", "Process")
    Set-Item -LiteralPath "Env:LOOM_DAEMON_TOKEN" -Value ""
    try {
        $cliStatus = Invoke-Executable -FilePath $loomExe -Arguments @(
            "status",
            "--daemon-url",
            $daemonBaseUrl
        )
    }
    finally {
        Restore-EnvironmentValue -Name "LOOM_DAEMON_TOKEN" -Value $oldCliToken
    }
    if ($cliStatus.exitCode -ne 0) {
        throw "packaged loom.exe status failed with exit code $($cliStatus.exitCode): $(Redact-Text $cliStatus.output)"
    }
    Assert-True ([string]$cliStatus.output -match '"status"\s*:\s*"ready"') "packaged loom.exe status output mismatch"

    $invoke = Invoke-JsonPost -Uri "$daemonBaseUrl/v1/invoke" -Body ([ordered]@{
        requestId = "loom-gateway-package-smoke"
        caller = "packaged-smoke"
        capability = "brain.plan"
        input = [ordered]@{
            goal = "Verify packaged Gateway planning"
            constraints = @("preserve packaged run contract", "record Gateway evidence")
            context = [ordered]@{ source = "packaged-gateway-smoke" }
        }
    })
    Assert-Equal "succeeded" ([string]$invoke.status) "Gateway brain.plan smoke status mismatch"
    Assert-Equal "Packaged Gateway plan" ([string]$invoke.output.summary) "Gateway plan summary mismatch"
    Assert-Equal "gateway" ([string]$invoke.output.planner.source) "Gateway planner source mismatch"
    Assert-Equal "smoke-resolved-model" ([string]$invoke.output.planner.model) "Gateway resolved model mismatch"
    Assert-Equal 2 (@($invoke.output.steps).Count) "Gateway plan step count mismatch"
    Assert-Equal "inspect packaged request" ([string]$invoke.output.steps[0]) "Gateway plan first step mismatch"
    Assert-Equal "execute packaged plan" ([string]$invoke.output.steps[1]) "Gateway plan second step mismatch"

    $runId = [string]$invoke.output.runId
    Assert-True (-not [string]::IsNullOrWhiteSpace($runId)) "Gateway smoke did not return a run id"
    $run = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$runId"
    Assert-Equal "succeeded" ([string]$run.status) "Stored Gateway run status mismatch"
    Assert-Equal "gateway" ([string]$run.output.planner.source) "Stored planner source mismatch"
    $events = Invoke-JsonGet -Uri "$daemonBaseUrl/v1/runs/$runId/events"
    $eventKinds = @($events.events | ForEach-Object { [string]$_.kind })
    Assert-Equal "run_started,capability_completed" ($eventKinds -join ",") "Gateway event sequence mismatch"

    Wait-ForPath -Path $gatewayCapturePath -TimeoutSeconds 15 -Job $gatewayJob
    $capture = Get-JsonFile -Path $gatewayCapturePath
    Assert-Equal $true ([bool]$capture.valid) "Gateway fixture did not validate the request"
    Assert-Equal $true ([bool]$capture.authReceived) "Gateway fixture did not receive the expected bearer token"
    Assert-Equal "POST" ([string]$capture.method) "Gateway fixture method mismatch"
    Assert-Equal "/v1/chat/completions" ([string]$capture.path) "Gateway fixture path mismatch"
    Assert-Equal "smoke-planner" ([string]$capture.model) "Gateway fixture request model mismatch"
    $userPayload = ([string]$capture.userContent) | ConvertFrom-Json
    Assert-Equal "Verify packaged Gateway planning" ([string]$userPayload.goal) "Gateway user goal mismatch"
    Assert-Equal "preserve packaged run contract" ([string]$userPayload.constraints[0]) "Gateway user constraint mismatch"
    Assert-Equal "packaged-gateway-smoke" ([string]$userPayload.context.source) "Gateway user context mismatch"
    Assert-True (-not ([string]$capture.userContent).Contains("smoke-token")) "Gateway user prompt leaked token"

    $summary.gatewayResolvedModel = [string]$invoke.output.planner.model
    $summary.plannerSource = [string]$invoke.output.planner.source
    $summary.planSummary = [string]$invoke.output.summary
    $summary.planSteps = @($invoke.output.steps | ForEach-Object { [string]$_ })
    $summary.runId = $runId
    $summary.runStatus = [string]$run.status
    $summary.eventKinds = $eventKinds
    $summary.status = "passed"
}
catch {
    $failure = $_
    $summary.status = "failed"
    $summary.error = Redact-Text $_.Exception.Message
}
finally {
    foreach ($name in $environmentNames) {
        Restore-EnvironmentValue -Name $name -Value $oldEnvironment[$name]
    }

    try {
        $summary.daemonStopped = Stop-SmokeProcess -Process $daemonProcess
        if (-not $summary.daemonStopped) {
            throw "loom-daemon process cleanup failed"
        }
    }
    catch {
        $cleanupError = $_.Exception.Message
        $summary.daemonStopped = $false
    }

    try {
        if ($null -ne $gatewayJob) {
            if ($gatewayJob.State -eq "Running" -or $gatewayJob.State -eq "NotStarted") {
                $wakeClient = $null
                try {
                    $wakeClient = [System.Net.Sockets.TcpClient]::new()
                    $wakeClient.Connect("127.0.0.1", $gatewayPort)
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
        $summary.gatewayJobStopped = $true
    }
    catch {
        $cleanupError = if ($null -eq $cleanupError) { $_.Exception.Message } else { "$cleanupError; $($_.Exception.Message)" }
        $summary.gatewayJobStopped = $false
    }

    Write-RedactedFile -SourcePath $daemonStdoutPath -DestinationPath $daemonStdoutEvidencePath
    Write-RedactedFile -SourcePath $daemonStderrPath -DestinationPath $daemonStderrEvidencePath
    if (Test-Path -LiteralPath $gatewayCapturePath -PathType Leaf) {
        Write-RedactedFile -SourcePath $gatewayCapturePath -DestinationPath $gatewayEvidencePath
    }
    if (Test-Path -LiteralPath $tempRoot -PathType Container) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }

    if ($null -ne $cleanupError) {
        $summary.status = "failed"
        $summary.error = Redact-Text $cleanupError
    }
    $summary.finishedAt = (Get-Date).ToString("o")
    Write-JsonEvidence -Path $summaryPath -Value $summary
}

if ($null -ne $failure) {
    throw $failure.Exception
}
if ($summary.status -ne "passed") {
    throw [System.InvalidOperationException]::new([string]$summary.error)
}

$summary | ConvertTo-Json -Depth 30
