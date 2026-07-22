[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ".\target\runtime-smoke"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Parse("127.0.0.1"),
        0
    )
    $listener.Start()
    try { return [int]$listener.LocalEndpoint.Port }
    finally { $listener.Stop() }
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
    return Invoke-RestMethod -Uri $Uri -Method Get -TimeoutSec 10
}

function Invoke-JsonPost {
    param([string]$Uri, [object]$Body)
    $json = $Body | ConvertTo-Json -Depth 40 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Wait-ForDaemon {
    param([string]$BaseUrl, [int]$ProcessId, [string]$ExpectedExecutablePath)
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        if (-not (Test-ExactProcessAlive -ProcessId $ProcessId -ExpectedExecutablePath $ExpectedExecutablePath)) {
            throw "Loom daemon exited before becoming ready: $ProcessId"
        }
        try {
            $health = Invoke-JsonGet -Uri "$BaseUrl/health"
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

function Write-HookFixture {
    param([string]$AppDataRoot, [int]$NodeCount)
    $sessionDir = Join-Path $AppDataRoot "com.vmjcv.arthook-next"
    $imageDir = Join-Path $sessionDir "images"
    New-Item -ItemType Directory -Force -Path $imageDir | Out-Null
    $nodes = @(
        [ordered]@{ id = "capture"; type = "sticker"; src = "images/capture.png"; x = 120; y = 80; w = 360; h = 210 },
        [ordered]@{ id = "art"; type = "art"; artId = "fixture-art"; src = "images/art.png"; x = 600; y = 190; w = 190; h = 150 },
        [ordered]@{ id = "missing"; type = "sticker"; src = "images/missing.png"; x = 860; y = 360; w = 150; h = 110 }
    )
    if ($NodeCount -ge 4) {
        $nodes += [ordered]@{ id = "extra"; type = "art"; artId = "fixture-extra"; src = "images/art.png"; x = 1040; y = 120; w = 160; h = 120 }
    }
    $links = @(
        [ordered]@{ id = "capture-to-art"; fromUnitId = "capture"; fromPortId = "output_image"; toUnitId = "art"; toPortId = "input_image" }
    )
    $payload = [ordered]@{
        workflowId = "hook-live"
        stickers = $nodes
        links = $links
    }
    Write-JsonFile -Path (Join-Path $sessionDir "session.json") -Value $payload
    $pngBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
    [System.IO.File]::WriteAllBytes((Join-Path $imageDir "capture.png"), $pngBytes)
    [System.IO.File]::WriteAllBytes((Join-Path $imageDir "art.png"), $pngBytes)
}

function Invoke-Inspector {
    param(
        [string]$InspectorPath,
        [int]$DebugPort,
        [string]$OutputPath,
        [string]$ScreenshotPath,
        [int]$MinimumNodes = 1
    )
    $output = @(& node $InspectorPath --debug-port $DebugPort --output $OutputPath --screenshot $ScreenshotPath --min-nodes $MinimumNodes 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "WebView inspector failed: $($output -join [Environment]::NewLine)"
    }
    return Get-Content -Raw -Encoding UTF8 -LiteralPath $OutputPath | ConvertFrom-Json
}

function Wait-ForHookCanvasUi {
    param(
        [string]$InspectorPath,
        [int]$DebugPort,
        [string]$OutputPath,
        [string]$ScreenshotPath,
        [int]$MinimumNodes,
        [AllowNull()][string]$PreviousRevision,
        [int]$TimeoutSeconds = 45
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastError = $null
    while ((Get-Date) -lt $deadline) {
        try {
            $snapshot = Invoke-Inspector `
                -InspectorPath $InspectorPath `
                -DebugPort $DebugPort `
                -OutputPath $OutputPath `
                -ScreenshotPath $ScreenshotPath `
                -MinimumNodes $MinimumNodes
            $revisionChanged = [string]::IsNullOrWhiteSpace($PreviousRevision) -or `
                ([string]$snapshot.revision -ne $PreviousRevision)
            if (@($snapshot.thumbnailNodeCount).Count -gt 0 -and `
                [int]$snapshot.thumbnailNodeCount -ge $MinimumNodes -and `
                $revisionChanged) {
                return $snapshot
            }
            $lastError = "Observed nodes=$($snapshot.thumbnailNodeCount), revision=$($snapshot.revision)."
        }
        catch {
            $lastError = $_.Exception.Message
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for Hook canvas UI nodes=$MinimumNodes with a new revision. $lastError"
}

$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
$layout = Get-LoomReleaseLayout -PackageDir $packageFullPath
$evidenceRootFullPath = [System.IO.Path]::GetFullPath($EvidenceRoot)
$runDir = Join-Path $evidenceRootFullPath ("hook-canvas-" + [Guid]::NewGuid().ToString("N"))
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
    Write-HookFixture -AppDataRoot $appDataRoot -NodeCount 3
    $desktopEnvironment = @{
        LOOM_DAEMON_URL = $daemonUrl
        LOOM_DAEMON_EXECUTABLE = $daemonExe
        LOOM_HOOK_BRIDGE_PORT = [string]$bridgePort
        LOOM_HOOK_BRIDGE_URL = $bridgeUrl
        LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
        LOOM_CONFIGURATION_ROOT = $configurationRoot
        APPDATA = $appDataRoot
        LOCALAPPDATA = $localAppDataRoot
        WEBVIEW2_USER_DATA_FOLDER = $webViewRoot
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$cdpPort"
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
    Wait-ForDaemon -BaseUrl $daemonUrl -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe | Out-Null

    $bridgeStart = Invoke-JsonPost -Uri "$daemonUrl/v1/hook-bridge/start" -Body @{ port = $bridgePort }
    Assert-True ([bool]$bridgeStart.running) "Isolated Hook Bridge did not start."
    $bridgeStatus = $null
    $bridgeDeadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $bridgeDeadline) {
        $bridgeStatus = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/status"
        if ([bool]$bridgeStatus.running -and [int]$bridgeStatus.subscribedClients -gt 0) { break }
        Start-Sleep -Milliseconds 250
    }
    Assert-True ($null -ne $bridgeStatus -and [int]$bridgeStatus.subscribedClients -gt 0) "Loom desktop did not subscribe to isolated Hook Bridge."

    $canvas = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/canvas"
    Assert-Equal $true ([bool]$canvas.available) "Initial Hook canvas was not available."
    Assert-Equal 3 @($canvas.nodes).Count "Initial Hook canvas node count mismatch."
    Assert-Equal 1 @($canvas.edges).Count "Initial Hook canvas edge count mismatch."
    Write-JsonFile -Path (Join-Path $runDir "canvas-initial.json") -Value $canvas

    $inspectorPath = Join-Path $repoRoot "scripts\Inspect-LoomWebView.mjs"
    $initialUi = Wait-ForHookCanvasUi `
        -InspectorPath $inspectorPath `
        -DebugPort $cdpPort `
        -OutputPath (Join-Path $runDir "ui-initial.json") `
        -ScreenshotPath (Join-Path $runDir "ui-initial.png") `
        -MinimumNodes 3 `
        -PreviousRevision $null
    Assert-Equal $true ([bool]$initialUi.thumbnailVisible) "Hook canvas thumbnail is not visible."
    Assert-Equal 3 ([int]$initialUi.thumbnailNodeCount) "Hook thumbnail node count mismatch."
    Assert-Equal 1 ([int]$initialUi.thumbnailEdgeCount) "Hook thumbnail edge count mismatch."
    Assert-Equal $false ([bool]$initialUi.yamlVisible) "YAML editor is visible before opening advanced information."
    Assert-Equal $false ([bool]$initialUi.advancedOpen) "Advanced technical information must be collapsed by default."
    Assert-Equal $true ([bool]$initialUi.fullCanvasVisible) "Clicking the thumbnail did not open the full visual canvas."

    Write-HookFixture -AppDataRoot $appDataRoot -NodeCount 4
    $instantiate = Invoke-JsonPost -Uri "$daemonUrl/v1/artloom-compat/ipc/instantiate-workflow" -Body @{
        nodes = @(@{ id = "capture" }, @{ id = "art" }, @{ id = "missing" }, @{ id = "extra" })
        edges = @(@{ source = "capture"; target = "art" })
        mode = "reference"
        workflowId = "hook-live"
    }
    Assert-Equal "success" ([string]$instantiate.type) "Hook instantiate broadcast failed."
    Write-JsonFile -Path (Join-Path $runDir "instantiate.json") -Value $instantiate

    $updatedUi = Wait-ForHookCanvasUi `
        -InspectorPath $inspectorPath `
        -DebugPort $cdpPort `
        -OutputPath (Join-Path $runDir "ui-updated.json") `
        -ScreenshotPath (Join-Path $runDir "ui-updated.png") `
        -MinimumNodes 4 `
        -PreviousRevision ([string]$initialUi.revision)
    Assert-Equal 4 ([int]$updatedUi.thumbnailNodeCount) "Hook canvas did not refresh to the updated node count."
    Assert-True ([string]$updatedUi.revision -ne [string]$initialUi.revision) "Hook canvas revision did not change after the bridge update."
    Write-JsonFile -Path (Join-Path $runDir "processes-during.json") -Value @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
}
catch {
    $failure = $_.Exception.ToString()
}
finally {
    if ($null -ne $daemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe)) {
            $failure = ($failure + "`nIsolated daemon cleanup failed.").Trim()
        }
    }
    if ($null -ne $desktopPid) {
        if (-not (Stop-ExactProcessById -ProcessId $desktopPid -ExpectedExecutablePath $desktopExe)) {
            $failure = ($failure + "`nIsolated desktop cleanup failed.").Trim()
        }
    }
    Start-Sleep -Milliseconds 500
    $afterCleanup = @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
    $unexpected = @($afterCleanup | Where-Object { $baselinePids -notcontains [int]$_.processId })
    Write-JsonFile -Path (Join-Path $runDir "processes-after-cleanup.json") -Value $afterCleanup
    if ($unexpected.Count -gt 0) {
        $failure = ($failure + "`nUnexpected packaged Loom processes remained: " + (($unexpected | ForEach-Object { $_.processId }) -join ",")).Trim()
    }
    $summary = [ordered]@{
        status = if ([string]::IsNullOrWhiteSpace($failure)) { "passed" } else { "failed" }
        packageDir = $packageFullPath
        evidenceRoot = $runDir
        daemonPort = $daemonPort
        hookBridgePort = $bridgePort
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
