[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ".\target\runtime-smoke"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$quotaExceededErrorMessage = ConvertFrom-UnicodeCodePoints @(
    0x989D, 0x5EA6, 0x4E0D, 0x8DB3, 0xFF08, 0x0048, 0x0054, 0x0054, 0x0050,
    0x0020, 0x0034, 0x0030, 0x0032, 0xFF09
)
$SmokePortMinimum = 30000
$SmokePortMaximum = 45000
$script:DaemonRequestHeaders = @{}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
}

. (Join-Path $PSScriptRoot "otter-canvas-ui-smoke\Inspector.ps1")

function Write-Utf8NoBom {
    param([string]$Path, [string]$Content)
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Write-JsonFile {
    param([string]$Path, [object]$Value)
    Write-Utf8NoBom -Path $Path -Content (($Value | ConvertTo-Json -Depth 40) + [Environment]::NewLine)
}

function Get-FreeTcpPort {
    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $port = Get-Random -Minimum $SmokePortMinimum -Maximum ($SmokePortMaximum + 1)
        $listener = [System.Net.Sockets.TcpListener]::new(
            [System.Net.IPAddress]::Parse("127.0.0.1"),
            $port
        )
        $listener.ExclusiveAddressUse = $true
        try {
            $listener.Start()
            return [int]$port
        }
        catch { }
        finally {
            $listener.Stop()
        }
    }
    throw "Unable to allocate an isolated Loom smoke port between $SmokePortMinimum and $SmokePortMaximum."
}

function Test-SamePath {
    param([AllowNull()][string]$Left, [AllowNull()][string]$Right)
    if ([string]::IsNullOrWhiteSpace($Left) -or [string]::IsNullOrWhiteSpace($Right)) { return $false }
    return [System.IO.Path]::GetFullPath($Left).Equals(
        [System.IO.Path]::GetFullPath($Right),
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function Get-ProcessSnapshotById {
    param([int]$ProcessId)
    $process = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId=$ProcessId" -ErrorAction SilentlyContinue
    if ($null -eq $process) { return $null }
    return [pscustomobject][ordered]@{
        processId = [int]$process.ProcessId
        parentProcessId = [int]$process.ParentProcessId
        name = [string]$process.Name
        ExecutablePath = [string]$process.ExecutablePath
        commandLine = [string]$process.CommandLine
    }
}

function Get-CandidateProcessSnapshot {
    param([string[]]$ExecutablePaths)
    $paths = @($ExecutablePaths | ForEach-Object { [System.IO.Path]::GetFullPath($_) })
    $result = @()
    foreach ($process in @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)) {
        $executable = [string]$process.ExecutablePath
        if ([string]::IsNullOrWhiteSpace($executable)) { continue }
        $matched = $false
        foreach ($path in $paths) {
            if (Test-SamePath -Left $executable -Right $path) { $matched = $true; break }
        }
        if ($matched) {
            $result += [pscustomobject][ordered]@{
                processId = [int]$process.ProcessId
                parentProcessId = [int]$process.ParentProcessId
                name = [string]$process.Name
                ExecutablePath = $executable
                commandLine = [string]$process.CommandLine
            }
        }
    }
    return @($result | Sort-Object processId)
}

function Test-ExactProcessAlive {
    param([int]$ProcessId, [string]$ExpectedExecutablePath)
    $snapshot = Get-ProcessSnapshotById -ProcessId $ProcessId
    return ($null -ne $snapshot -and (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath))
}

function Stop-ExactProcessById {
    param([AllowNull()][object]$ProcessId, [string]$ExpectedExecutablePath)
    if ($null -eq $ProcessId) { return $true }
    $id = [int]$ProcessId
    $snapshot = Get-ProcessSnapshotById -ProcessId $id
    if ($null -eq $snapshot) { return $true }
    if (-not (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath)) { return $false }
    Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline) {
        if ($null -eq (Get-ProcessSnapshotById -ProcessId $id)) { return $true }
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
        if ($Arguments.Count -gt 0) { $start.ArgumentList = $Arguments }
        return Start-Process @start
    }
    finally {
        foreach ($name in $oldEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name], "Process")
        }
    }
}

function Invoke-JsonGet {
    param([string]$Uri)
    return Invoke-RestMethod -Uri $Uri -Method Get -Headers $script:DaemonRequestHeaders -TimeoutSec 10
}

function Invoke-JsonPost {
    param([string]$Uri, [object]$Body)
    $json = $Body | ConvertTo-Json -Depth 40 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -Headers $script:DaemonRequestHeaders -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Wait-ForDaemon {
    param([string]$BaseUrl, [int]$ProcessId, [string]$ExpectedExecutablePath, [string]$TokenPath)
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        if (-not (Test-ExactProcessAlive -ProcessId $ProcessId -ExpectedExecutablePath $ExpectedExecutablePath)) {
            throw "Loom daemon exited before becoming ready: $ProcessId"
        }
        try {
            $health = Invoke-JsonGet -Uri "$BaseUrl/health"
            if (-not (Test-Path -LiteralPath $TokenPath -PathType Leaf)) {
                Start-Sleep -Milliseconds 200
                continue
            }
            $token = [System.IO.File]::ReadAllText($TokenPath).Trim()
            if ([string]::IsNullOrWhiteSpace($token)) {
                Start-Sleep -Milliseconds 200
                continue
            }
            $script:DaemonRequestHeaders = @{ Authorization = "Bearer $token" }
            $status = Invoke-JsonGet -Uri "$BaseUrl/status"
            if ([string]$health.status -eq "ok" -and [string]$status.status -eq "ready") {
                return $status
            }
        } catch { }
        Start-Sleep -Milliseconds 200
    }
    throw "Timed out waiting for Loom daemon at $BaseUrl"
}

function Wait-ForDesktopDaemon {
    param([int]$DesktopPid, [string]$DaemonExecutablePath, [int[]]$BaselinePids)
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        foreach ($candidate in @(Get-CandidateProcessSnapshot -ExecutablePaths @($DaemonExecutablePath))) {
            if ($BaselinePids -contains [int]$candidate.processId) { continue }
            if ([int]$candidate.parentProcessId -eq $DesktopPid) { return $candidate }
        }
        Start-Sleep -Milliseconds 200
    }
    throw "Timed out waiting for Loom desktop daemon sidecar."
}

function Write-OtterFixture {
    param([string]$AppDataRoot, [int]$NodeCount)
    $sessionDir = Join-Path $AppDataRoot "com.yamiyu.hook"
    $imageDir = Join-Path $sessionDir "images"
    New-Item -ItemType Directory -Force -Path $imageDir | Out-Null
    $nodes = @(
        [ordered]@{ id = "capture"; type = "sticker"; src = "images/capture.png"; x = 120; y = 80; w = 360; h = 210 },
        [ordered]@{ id = "failed-art"; type = "art"; artId = "test.publisher/fixture-art"; status = "error"; errorMessage = $quotaExceededErrorMessage; src = "images/failed-art.png"; x = 600; y = 190; w = 190; h = 150 },
        [ordered]@{ id = "missing"; type = "sticker"; src = "images/missing.png"; x = 860; y = 360; w = 150; h = 110 }
    )
    if ($NodeCount -ge 4) {
        $nodes += [ordered]@{ id = "extra"; type = "art"; artId = "test.publisher/fixture-extra"; src = "images/art.png"; x = 1040; y = 120; w = 160; h = 120 }
    }
    $links = @(
        [ordered]@{ id = "capture-to-failed-art"; fromUnitId = "capture"; fromPortId = "output"; toUnitId = "failed-art"; toPortId = "input" }
    )
    $payload = [ordered]@{
        workflowId = "hook-live"
        stickers = $nodes
        links = $links
    }
    Write-JsonFile -Path (Join-Path $sessionDir "session.json") -Value $payload
    $captureBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
    $failedArtBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
    [System.IO.File]::WriteAllBytes((Join-Path $imageDir "capture.png"), $captureBytes)
    [System.IO.File]::WriteAllBytes((Join-Path $imageDir "failed-art.png"), $failedArtBytes)
    [System.IO.File]::WriteAllBytes((Join-Path $imageDir "art.png"), $captureBytes)
}

$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
$layout = Get-LoomReleaseLayout -PackageDir $packageFullPath
$evidenceRootFullPath = [System.IO.Path]::GetFullPath($EvidenceRoot)
$runDir = Join-Path $evidenceRootFullPath ("otter-canvas-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$controlPlaneRoot = Join-Path $runDir "control-plane"
$configurationRoot = Join-Path $runDir "configuration"
$appDataRoot = Join-Path $runDir "appdata"
$localAppDataRoot = Join-Path $runDir "localappdata"
$webViewRoot = Join-Path $runDir "webview2"
foreach ($root in @($controlPlaneRoot, $configurationRoot, $appDataRoot, $localAppDataRoot, $webViewRoot)) {
    New-Item -ItemType Directory -Force -Path $root | Out-Null
}

$desktopExe = [System.IO.Path]::GetFullPath($layout.desktopExe)
$daemonExe = [System.IO.Path]::GetFullPath($layout.daemonExe)
$candidatePaths = @($desktopExe, $daemonExe)
$baselineCandidates = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
$baselinePids = @($baselineCandidates | ForEach-Object { [int]$_.processId })
Write-JsonFile -Path (Join-Path $runDir "processes-baseline.json") -Value $baselineCandidates

$daemonPort = Get-FreeTcpPort
$bridgePort = Get-FreeTcpPort
$cdpPort = Get-FreeTcpPort
while ($bridgePort -eq $daemonPort -or $cdpPort -eq $daemonPort -or $cdpPort -eq $bridgePort) {
    $bridgePort = Get-FreeTcpPort
    $cdpPort = Get-FreeTcpPort
}
$daemonUrl = "http://127.0.0.1:$daemonPort"
$bridgeUrl = "ws://127.0.0.1:$bridgePort"
$stdoutPath = Join-Path $runDir "Loom.stdout.log"
$stderrPath = Join-Path $runDir "Loom.stderr.log"
$desktopProcess = $null
$desktopPid = $null
$daemonPid = $null
$failure = $null

try {
    Write-OtterFixture -AppDataRoot $appDataRoot -NodeCount 3
    $desktopEnvironment = @{
        LOOM_DAEMON_URL = $daemonUrl
        LOOM_DAEMON_EXECUTABLE = $daemonExe
        LOOM_HOOK_BRIDGE_PORT = [string]$bridgePort
        LOOM_HOOK_BRIDGE_URL = $bridgeUrl
        LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $configurationRoot
        LOOM_SMOKE_ALLOW_MULTIPLE_INSTANCES = "1"
        APPDATA = $appDataRoot
        LOCALAPPDATA = $localAppDataRoot
        WEBVIEW2_USER_DATA_FOLDER = $webViewRoot
        LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT = [string]$cdpPort
    }
    $desktopProcess = Start-IsolatedProcess `
        -FilePath $desktopExe `
        -WorkingDirectory $packageFullPath `
        -Arguments @() `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath `
        -EnvironmentValues $desktopEnvironment
    $desktopPid = [int]$desktopProcess.Id
    Assert-True (Test-ExactProcessAlive -ProcessId $desktopPid -ExpectedExecutablePath $desktopExe) "Packaged Loom.exe did not remain alive."

    $sibling = Wait-ForDesktopDaemon -DesktopPid $desktopPid -DaemonExecutablePath $daemonExe -BaselinePids $baselinePids
    $daemonPid = [int]$sibling.processId
    Wait-ForDaemon `
        -BaseUrl $daemonUrl `
        -ProcessId $daemonPid `
        -ExpectedExecutablePath $daemonExe `
        -TokenPath (Join-Path $controlPlaneRoot "daemon-token") | Out-Null

    $bridgeStatus = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/status"
    if ([bool]$bridgeStatus.running -and [int]$bridgeStatus.port -ne $bridgePort) {
        $stoppedBridge = Invoke-JsonPost -Uri "$daemonUrl/v1/hook-bridge/stop" -Body @{}
        Assert-Equal $false ([bool]$stoppedBridge.running) "Isolated Otter Bridge did not stop before changing ports."
        $bridgeStatus = $stoppedBridge
    }
    if (-not [bool]$bridgeStatus.running) {
        $bridgeStatus = Invoke-JsonPost -Uri "$daemonUrl/v1/hook-bridge/start" -Body @{ port = $bridgePort }
    }
    Assert-True ([bool]$bridgeStatus.running) "Isolated Otter Bridge did not start."
    Assert-Equal $bridgePort ([int]$bridgeStatus.port) "Isolated Otter Bridge port mismatch."
    $bridgeDeadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $bridgeDeadline) {
        $bridgeStatus = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/status"
        if ([bool]$bridgeStatus.running -and [int]$bridgeStatus.subscribedClients -gt 0) { break }
        Start-Sleep -Milliseconds 250
    }
    Assert-True ($null -ne $bridgeStatus -and [int]$bridgeStatus.subscribedClients -gt 0) "Loom desktop did not subscribe to isolated Otter Bridge."

    $canvas = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/canvas"
    Assert-Equal $true ([bool]$canvas.available) "Initial Otter canvas was not available."
    Assert-Equal 3 @($canvas.nodes).Count "Initial Otter canvas node count mismatch."
    Assert-Equal 1 @($canvas.edges).Count "Initial Otter canvas edge count mismatch."
    Write-JsonFile -Path (Join-Path $runDir "canvas-initial.json") -Value $canvas

    $inspectorPath = Join-Path $repoRoot "scripts\Inspect-LoomWebView.mjs"
    $initialUi = Wait-ForOtterCanvasUi `
        -InspectorPath $inspectorPath `
        -DebugPort $cdpPort `
        -OutputPath (Join-Path $runDir "ui-initial.json") `
        -ScreenshotPath (Join-Path $runDir "ui-initial.png") `
        -MinimumNodes 3 `
        -PreviousRevision $null
    Assert-Equal $true ([bool]$initialUi.thumbnailVisible) "Otter canvas thumbnail is not visible."
    Assert-Equal 3 ([int]$initialUi.thumbnailNodeCount) "Otter thumbnail node count mismatch."
    Assert-Equal 1 ([int]$initialUi.thumbnailEdgeCount) "Otter thumbnail edge count mismatch."
    Assert-Equal $true ([bool]$initialUi.failedArtThumbnailFailureVisible) "Otter thumbnail must show execution failure for failed-art."
    Assert-Equal $false ([bool]$initialUi.yamlVisible) "YAML editor is visible before opening advanced information."
    Assert-Equal $false ([bool]$initialUi.advancedOpen) "Advanced technical information must be collapsed by default."
    $initialFailedArtNode = @($initialUi.thumbnailNodes | Where-Object { [string]$_.nodeId -eq "failed-art" })[0]
    Assert-True ($null -ne $initialFailedArtNode) "Missing failed-art thumbnail node presentation."
    Assert-Equal $false ([bool]$initialFailedArtNode.hasImage) "failed-art thumbnail must not render an image preview."
    Assert-Equal $quotaExceededErrorMessage ([string]$initialFailedArtNode.placeholderDetailText) "failed-art thumbnail must show the Otter failure reason."

    Write-OtterFixture -AppDataRoot $appDataRoot -NodeCount 4
    $refreshStatus = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/status"
    Assert-Equal "loom.hook.v1" ([string]$refreshStatus.protocol) "Otter canvas refresh protocol mismatch."
    Write-JsonFile -Path (Join-Path $runDir "refresh-status.json") -Value $refreshStatus

    $updatedUi = Wait-ForOtterCanvasUi `
        -InspectorPath $inspectorPath `
        -DebugPort $cdpPort `
        -OutputPath (Join-Path $runDir "ui-updated.json") `
        -ScreenshotPath (Join-Path $runDir "ui-updated.png") `
        -MinimumNodes 4 `
        -PreviousRevision ([string]$initialUi.revision)
    Assert-Equal 4 ([int]$updatedUi.thumbnailNodeCount) "Otter canvas did not refresh to the updated node count."
    Assert-True ([string]$updatedUi.revision -ne [string]$initialUi.revision) "Otter canvas revision did not change after the bridge update."
    Assert-Equal $true ([bool]$updatedUi.failedArtThumbnailFailureVisible) "Updated Otter thumbnail must keep the failed-art execution failure presentation."
    $updatedFailedArtNode = @($updatedUi.thumbnailNodes | Where-Object { [string]$_.nodeId -eq "failed-art" })[0]
    Assert-True ($null -ne $updatedFailedArtNode) "Missing updated failed-art thumbnail node presentation."
    Assert-Equal $false ([bool]$updatedFailedArtNode.hasImage) "Updated failed-art thumbnail must not render an image preview."
    Assert-Equal $quotaExceededErrorMessage ([string]$updatedFailedArtNode.placeholderDetailText) "Updated failed-art thumbnail must keep the Otter failure reason."
    Write-JsonFile -Path (Join-Path $runDir "processes-during.json") -Value @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
}
catch {
    $runtimeDiagnostic = Get-OtterCanvasFailureDiagnostic -DesktopPid $desktopPid -CdpPort $cdpPort -DesktopStderrPath $stderrPath
    $primaryFailure = Limit-SmokeText -Text $_.Exception.ToString() -MaxLength 4000
    $failure = Limit-SmokeText -Text (("Runtime diagnostic: " + $runtimeDiagnostic), ("Primary failure: " + $primaryFailure) -join [Environment]::NewLine) -MaxLength 8000
}
finally {
    if ($null -ne $daemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe)) {
            $failure = Limit-SmokeText -Text (($failure + "`nIsolated daemon cleanup failed.").Trim()) -MaxLength 8000
        }
    }
    if ($null -ne $desktopPid) {
        if (-not (Stop-ExactProcessById -ProcessId $desktopPid -ExpectedExecutablePath $desktopExe)) {
            $failure = Limit-SmokeText -Text (($failure + "`nIsolated desktop cleanup failed.").Trim()) -MaxLength 8000
        }
    }
    Start-Sleep -Milliseconds 500
    $afterCleanup = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    $unexpected = @($afterCleanup | Where-Object { $baselinePids -notcontains [int]$_.processId })
    Write-JsonFile -Path (Join-Path $runDir "processes-after-cleanup.json") -Value $afterCleanup
    if ($unexpected.Count -gt 0) {
        $failure = Limit-SmokeText -Text (($failure + "`nUnexpected packaged Loom processes remained: " + (($unexpected | ForEach-Object { $_.processId }) -join ",")).Trim()) -MaxLength 8000
    }
    $summary = [ordered]@{
        status = if ([string]::IsNullOrWhiteSpace($failure)) { "passed" } else { "failed" }
        packageDir = $packageFullPath
        evidenceRoot = $runDir
        daemonPort = $daemonPort
        otterBridgePort = $bridgePort
        cdpPort = $cdpPort
        desktopPid = $desktopPid
        daemonPid = $daemonPid
        baselineProcessCount = $baselineCandidates.Count
        unexpectedProcessesAfterCleanup = $unexpected.Count
        error = $failure
    }
    Write-JsonFile -Path (Join-Path $runDir "summary.json") -Value $summary
}

if (-not [string]::IsNullOrWhiteSpace($failure)) {
    throw $failure
}

Write-Output (($summary | ConvertTo-Json -Depth 20))
