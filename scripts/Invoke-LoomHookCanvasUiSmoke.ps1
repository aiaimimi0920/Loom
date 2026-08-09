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

function Limit-SmokeText {
    param(
        [AllowNull()][object]$Text,
        [int]$MaxLength = 4000
    )

    $value = [string]$Text
    if ($value.Length -le $MaxLength) { return $value }
    return $value.Substring(0, $MaxLength) + "..."
}

function Read-BoundedTaskText {
    param(
        [System.Threading.Tasks.Task[string]]$Task,
        [string]$StreamName,
        [int]$TimeoutMilliseconds = 2000
    )

    if ($null -eq $Task) { return "" }
    try {
        if (-not $Task.Wait($TimeoutMilliseconds)) {
            return "[$StreamName drain timed out after $TimeoutMilliseconds milliseconds]"
        }
        return $Task.GetAwaiter().GetResult()
    }
    catch {
        return "[$StreamName drain failed: $($_.Exception.Message)]"
    }
}

function Get-HookCanvasFailureDiagnostic {
    param(
        [AllowNull()][object]$DesktopPid,
        [int]$CdpPort,
        [string]$DesktopStderrPath
    )

    $sections = [System.Collections.Generic.List[string]]::new()
    try {
        $processes = @(Get-CimInstance Win32_Process -OperationTimeoutSec 2 -ErrorAction Stop)
        $relatedIds = @()
        if ($null -ne $DesktopPid) { $relatedIds += [int]$DesktopPid }
        $added = $true
        while ($added) {
            $added = $false
            foreach ($candidate in $processes) {
                if ($relatedIds -contains [int]$candidate.ProcessId) { continue }
                if ($relatedIds -contains [int]$candidate.ParentProcessId) {
                    $relatedIds += [int]$candidate.ProcessId
                    $added = $true
                }
            }
        }
        $processTree = @($processes | Where-Object { $relatedIds -contains [int]$_.ProcessId } | ForEach-Object {
            [ordered]@{
                processId = [int]$_.ProcessId
                parentProcessId = [int]$_.ParentProcessId
                name = [string]$_.Name
                commandLine = [string]$_.CommandLine
            }
        })
        $processTreeText = Limit-SmokeText -Text ($processTree | ConvertTo-Json -Depth 5 -Compress) -MaxLength 1800
        $sections.Add("Desktop process tree: " + $processTreeText)
    }
    catch {
        $sections.Add("Desktop process tree unavailable: $($_.Exception.Message)")
    }

    try {
        $listeners = @([System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() | Where-Object { [int]$_.Port -eq $CdpPort } | ForEach-Object {
            [ordered]@{
                localAddress = [string]$_.Address
                localPort = [int]$_.Port
            }
        })
        $listenerText = Limit-SmokeText -Text ($listeners | ConvertTo-Json -Depth 4 -Compress) -MaxLength 500
        $sections.Add("CDP listeners: " + $listenerText)
    }
    catch {
        $sections.Add("CDP listeners unavailable: $($_.Exception.Message)")
    }

    $desktopStderr = ""
    if (Test-Path -LiteralPath $DesktopStderrPath -PathType Leaf) {
        try { $desktopStderr = (Get-Content -Raw -Encoding UTF8 -LiteralPath $DesktopStderrPath).Trim() }
        catch { $desktopStderr = "could not read desktop stderr: $($_.Exception.Message)" }
    }
    $desktopStderrText = if ([string]::IsNullOrWhiteSpace($desktopStderr)) { "<empty>" } else { Limit-SmokeText -Text $desktopStderr -MaxLength 900 }
    $sections.Add("Desktop stderr: " + $desktopStderrText)
    return Limit-SmokeText -Text ($sections -join [Environment]::NewLine) -MaxLength 3500
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
        [ordered]@{ id = "failed-art"; type = "art"; artId = "fixture-art"; status = "error"; errorMessage = $quotaExceededErrorMessage; src = "images/failed-art.png"; x = 600; y = 190; w = 190; h = 150 },
        [ordered]@{ id = "missing"; type = "sticker"; src = "images/missing.png"; x = 860; y = 360; w = 150; h = 110 }
    )
    if ($NodeCount -ge 4) {
        $nodes += [ordered]@{ id = "extra"; type = "art"; artId = "fixture-extra"; src = "images/art.png"; x = 1040; y = 120; w = 160; h = 120 }
    }
    $links = @(
        [ordered]@{ id = "capture-to-failed-art"; fromUnitId = "capture"; fromPortId = "output_image"; toUnitId = "failed-art"; toPortId = "input_image" }
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

function Invoke-Inspector {
    param(
        [string]$InspectorPath,
        [int]$DebugPort,
        [string]$OutputPath,
        [string]$ScreenshotPath,
        [int]$MinimumNodes = 1,
        [int]$TimeoutSeconds = 20
    )

    $stdoutPath = "$OutputPath.inspector.stdout.log"
    $stderrPath = "$OutputPath.inspector.stderr.log"
    Remove-Item -LiteralPath $OutputPath, $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    $quotedInspectorPath = '"' + $InspectorPath.Replace('"', '\"') + '"'
    $quotedOutputPath = '"' + $OutputPath.Replace('"', '\"') + '"'
    $quotedScreenshotPath = '"' + $ScreenshotPath.Replace('"', '\"') + '"'
    $process = $null
    $stdoutTask = $null
    $stderrTask = $null
    $timedOut = $false
    $terminationError = $null
    $terminated = $true
    try {
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = (Get-Command node).Source
        $startInfo.Arguments = @(
            $quotedInspectorPath,
            "--debug-port", [string]$DebugPort,
            "--output", $quotedOutputPath,
            "--screenshot", $quotedScreenshotPath,
            "--min-nodes", [string]$MinimumNodes
        ) -join " "
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $startInfo
        if (-not $process.Start()) { throw "Could not start the WebView inspector process." }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            $timedOut = $true
            try { $process.Kill() }
            catch { $terminationError = "Initial inspector termination failed: $($_.Exception.Message)" }
            try { $terminated = $process.WaitForExit(2000) }
            catch { $terminated = $false }
            if (-not $terminated) {
                try { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
                catch { $terminationError = (($terminationError, "Forced inspector termination failed: $($_.Exception.Message)") -join " ").Trim() }
                try { $terminated = $process.WaitForExit(2000) }
                catch { $terminated = $false }
            }
            if (-not $terminated -and [string]::IsNullOrWhiteSpace($terminationError)) {
                $terminationError = "Inspector process did not exit after bounded termination attempts."
            }
        }
        $stdout = Read-BoundedTaskText -Task $stdoutTask -StreamName "stdout"
        $stderr = Read-BoundedTaskText -Task $stderrTask -StreamName "stderr"
        $exitCode = $null
        if ($process.HasExited) { $exitCode = $process.ExitCode }
        Write-Utf8NoBom -Path $stdoutPath -Content $stdout
        Write-Utf8NoBom -Path $stderrPath -Content $stderr
        if ($timedOut) {
            $terminationSuffix = if ([string]::IsNullOrWhiteSpace($terminationError)) { "" } else { " $terminationError" }
            throw "WebView inspector timed out after $TimeoutSeconds seconds.$terminationSuffix $(Limit-SmokeText -Text $stderr)"
        }
        if ($null -eq $exitCode) {
            throw "WebView inspector exited without a concrete exit code."
        }
        if ($exitCode -ne 0) {
            $output = Limit-SmokeText -Text (($stdout, $stderr | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine)
            throw "WebView inspector failed with exit code $exitCode`: $output"
        }
    }
    finally {
        if ($null -ne $process) { $process.Dispose() }
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
        [int]$TimeoutSeconds = 90
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastError = $null
    while ((Get-Date) -lt $deadline) {
        try {
            $remainingSeconds = [Math]::Max(1, [Math]::Ceiling(($deadline - (Get-Date)).TotalSeconds))
            $inspectorTimeoutSeconds = [int][Math]::Min(20, $remainingSeconds)
            $snapshot = Invoke-Inspector `
                -InspectorPath $InspectorPath `
                -DebugPort $DebugPort `
                -OutputPath $OutputPath `
                -ScreenshotPath $ScreenshotPath `
                -MinimumNodes $MinimumNodes `
                -TimeoutSeconds $inspectorTimeoutSeconds
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
            $lastError = Limit-SmokeText -Text $_.Exception.Message
        }
        Start-Sleep -Milliseconds 250
    }
    $diagnostic = "unavailable"
    if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
        try {
            $diagnostic = Limit-SmokeText -Text ((Get-Content -Raw -Encoding UTF8 -LiteralPath $OutputPath).Trim())
        }
        catch {
            $diagnostic = "could not read ${OutputPath}: $($_.Exception.Message)"
        }
    }
    throw "Timed out waiting for Hook canvas UI nodes=$MinimumNodes with a new revision. $lastError Inspector diagnostic: $diagnostic"
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
    Wait-ForDaemon -BaseUrl $daemonUrl -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe | Out-Null

    $bridgeStatus = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/status"
    if ([bool]$bridgeStatus.running -and [int]$bridgeStatus.port -ne $bridgePort) {
        $stoppedBridge = Invoke-JsonPost -Uri "$daemonUrl/v1/hook-bridge/stop" -Body @{}
        Assert-Equal $false ([bool]$stoppedBridge.running) "Isolated Hook Bridge did not stop before changing ports."
        $bridgeStatus = $stoppedBridge
    }
    if (-not [bool]$bridgeStatus.running) {
        $bridgeStatus = Invoke-JsonPost -Uri "$daemonUrl/v1/hook-bridge/start" -Body @{ port = $bridgePort }
    }
    Assert-True ([bool]$bridgeStatus.running) "Isolated Hook Bridge did not start."
    Assert-Equal $bridgePort ([int]$bridgeStatus.port) "Isolated Hook Bridge port mismatch."
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
    Assert-Equal $true ([bool]$initialUi.failedArtThumbnailFailureVisible) "Hook thumbnail must show execution failure for failed-art."
    Assert-Equal $false ([bool]$initialUi.yamlVisible) "YAML editor is visible before opening advanced information."
    Assert-Equal $false ([bool]$initialUi.advancedOpen) "Advanced technical information must be collapsed by default."
    $initialFailedArtNode = @($initialUi.thumbnailNodes | Where-Object { [string]$_.nodeId -eq "failed-art" })[0]
    Assert-True ($null -ne $initialFailedArtNode) "Missing failed-art thumbnail node presentation."
    Assert-Equal $false ([bool]$initialFailedArtNode.hasImage) "failed-art thumbnail must not render an image preview."
    Assert-Equal $quotaExceededErrorMessage ([string]$initialFailedArtNode.placeholderDetailText) "failed-art thumbnail must show the Hook failure reason."

    Write-HookFixture -AppDataRoot $appDataRoot -NodeCount 4
    $refreshBroadcast = Invoke-JsonPost -Uri "$daemonUrl/v1/artloom-compat/arts/broadcast-updated" -Body @{}
    Assert-Equal $true ([bool]$refreshBroadcast.broadcasted) "Hook canvas refresh broadcast failed."
    Write-JsonFile -Path (Join-Path $runDir "refresh-broadcast.json") -Value $refreshBroadcast

    $updatedUi = Wait-ForHookCanvasUi `
        -InspectorPath $inspectorPath `
        -DebugPort $cdpPort `
        -OutputPath (Join-Path $runDir "ui-updated.json") `
        -ScreenshotPath (Join-Path $runDir "ui-updated.png") `
        -MinimumNodes 4 `
        -PreviousRevision ([string]$initialUi.revision)
    Assert-Equal 4 ([int]$updatedUi.thumbnailNodeCount) "Hook canvas did not refresh to the updated node count."
    Assert-True ([string]$updatedUi.revision -ne [string]$initialUi.revision) "Hook canvas revision did not change after the bridge update."
    Assert-Equal $true ([bool]$updatedUi.failedArtThumbnailFailureVisible) "Updated Hook thumbnail must keep the failed-art execution failure presentation."
    $updatedFailedArtNode = @($updatedUi.thumbnailNodes | Where-Object { [string]$_.nodeId -eq "failed-art" })[0]
    Assert-True ($null -ne $updatedFailedArtNode) "Missing updated failed-art thumbnail node presentation."
    Assert-Equal $false ([bool]$updatedFailedArtNode.hasImage) "Updated failed-art thumbnail must not render an image preview."
    Assert-Equal $quotaExceededErrorMessage ([string]$updatedFailedArtNode.placeholderDetailText) "Updated failed-art thumbnail must keep the Hook failure reason."
    Write-JsonFile -Path (Join-Path $runDir "processes-during.json") -Value @(Get-CandidateProcessSnapshot -ExecutablePaths $candidatePaths)
}
catch {
    $runtimeDiagnostic = Get-HookCanvasFailureDiagnostic -DesktopPid $desktopPid -CdpPort $cdpPort -DesktopStderrPath $stderrPath
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
