[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ".\target\runtime-smoke\hook-error-preview"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
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
        executablePath = [string]$process.ExecutablePath
        commandLine = [string]$process.CommandLine
    }
}

function Test-ExactProcessAlive {
    param([int]$ProcessId, [string]$ExpectedExecutablePath)
    $snapshot = Get-ProcessSnapshotById -ProcessId $ProcessId
    return ($null -ne $snapshot -and (Test-SamePath -Left $snapshot.executablePath -Right $ExpectedExecutablePath))
}

function Stop-ExactProcessById {
    param([AllowNull()][object]$ProcessId, [string]$ExpectedExecutablePath)
    if ($null -eq $ProcessId) { return $true }
    $id = [int]$ProcessId
    $snapshot = Get-ProcessSnapshotById -ProcessId $id
    if ($null -eq $snapshot) { return $true }
    if (-not (Test-SamePath -Left $snapshot.executablePath -Right $ExpectedExecutablePath)) { return $false }
    Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline) {
        if ($null -eq (Get-ProcessSnapshotById -ProcessId $id)) { return $true }
        Start-Sleep -Milliseconds 100
    }
    return $false
}

function Start-InheritedEnvProcess {
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

function Write-HookFixture {
    param([string]$AppDataRoot)
    $sessionDir = Join-Path $AppDataRoot "com.vmjcv.arthook-next"
    $imageDir = Join-Path $sessionDir "images"
    New-Item -ItemType Directory -Force -Path $imageDir | Out-Null
    $upstreamPath = [System.IO.Path]::GetFullPath((Join-Path $imageDir "upstream.png"))
    $failedArtPath = [System.IO.Path]::GetFullPath((Join-Path $imageDir "failed-art.png"))
    $payload = [ordered]@{
        workflowId = "hook-error-preview"
        stickers = @(
            [ordered]@{
                id = "upstream"
                type = "sticker"
                src = $upstreamPath
                x = 120
                y = 80
                w = 360
                h = 210
            },
            [ordered]@{
                id = "failed-art"
                type = "art"
                artId = "custom-1770131241684"
                status = "error"
                src = $failedArtPath
                x = 600
                y = 190
                w = 190
                h = 150
                minified = $true
                opacityMini = 0.9
                opacityNormal = 1.0
                savedRect = [ordered]@{
                    x = 1508.0
                    y = 7.0
                    w = 500.0
                    h = 750.0
                }
                cropOffset = [ordered]@{
                    x = 269.33333333333326
                    y = 384.33333333333326
                }
                params = [ordered]@{
                    reference = "upstream"
                    strength = 61
                }
            }
        )
        links = @(
            [ordered]@{
                id = "upstream-to-failed-art"
                fromUnitId = "upstream"
                fromPortId = "output"
                toUnitId = "failed-art"
                toPortId = "input"
            }
        )
    }
    Write-JsonFile -Path (Join-Path $sessionDir "session.json") -Value $payload

    $upstreamBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
    $failedArtBytes = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=")
    [System.IO.File]::WriteAllBytes($upstreamPath, $upstreamBytes)
    [System.IO.File]::WriteAllBytes($failedArtPath, $failedArtBytes)
}

$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
$layout = Get-LoomReleaseLayout -PackageDir $packageFullPath
$evidenceRootFullPath = [System.IO.Path]::GetFullPath($EvidenceRoot)
$runDir = Join-Path $evidenceRootFullPath ("hook-error-preview-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$controlPlaneRoot = Join-Path $runDir "control-plane"
$appDataRoot = Join-Path $runDir "appdata"
$localAppDataRoot = Join-Path $runDir "localappdata"
foreach ($root in @($controlPlaneRoot, $appDataRoot, $localAppDataRoot)) {
    New-Item -ItemType Directory -Force -Path $root | Out-Null
}

$daemonExe = [System.IO.Path]::GetFullPath($layout.daemonExe)
$daemonPort = Get-LoomSmokePort
$daemonUrl = "http://127.0.0.1:$daemonPort"
$stdoutPath = Join-Path $runDir "loom-daemon.stdout.log"
$stderrPath = Join-Path $runDir "loom-daemon.stderr.log"
$downloadPath = Join-Path $runDir "failed-art.preview.png"
$daemonProcess = $null
$daemonPid = $null
$failure = $null

try {
    Write-HookFixture -AppDataRoot $appDataRoot
    $daemonEnvironment = @{
        LOOM_DAEMON_HOST = "127.0.0.1"
        LOOM_DAEMON_PORT = [string]$daemonPort
        LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
        APPDATA = $appDataRoot
        LOCALAPPDATA = $localAppDataRoot
    }
    $daemonProcess = Start-InheritedEnvProcess `
        -FilePath $daemonExe `
        -WorkingDirectory $packageFullPath `
        -Arguments @() `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath `
        -EnvironmentValues $daemonEnvironment
    $daemonPid = [int]$daemonProcess.Id
    Assert-True (Test-ExactProcessAlive -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe) "Packaged loom-daemon.exe did not remain alive."
    Wait-ForDaemon -BaseUrl $daemonUrl -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe | Out-Null

    $canvas = Invoke-JsonGet -Uri "$daemonUrl/v1/hook-bridge/canvas"
    Assert-Equal $true ([bool]$canvas.available) "Hook canvas must be available."
    Assert-Equal 2 @($canvas.nodes).Count "Hook canvas node count mismatch."
    Assert-Equal 1 @($canvas.edges).Count "Hook canvas edge count mismatch."
    $failedArtNode = @($canvas.nodes | Where-Object { [string]$_.id -eq "failed-art" })[0]
    Assert-True ($null -ne $failedArtNode) "Missing failed-art node in Hook canvas."
    Assert-Equal "error" ([string]$failedArtNode.status) "failed-art node status mismatch."
    Assert-Equal $true ([bool]$failedArtNode.previewAvailable) "failed-art preview must be available."

    $previewUrl = [string]$failedArtNode.previewUrl
    Assert-True (-not [string]::IsNullOrWhiteSpace($previewUrl)) "failed-art preview URL is missing."
    Assert-True $previewUrl.Contains("/failed-art/preview") "failed-art preview URL should target the failed-art node."

    Invoke-WebRequest -Uri ($daemonUrl + $previewUrl) -OutFile $downloadPath -TimeoutSec 15 | Out-Null

    $failedArtSourcePath = Join-Path $appDataRoot "com.vmjcv.arthook-next\images\failed-art.png"
    $upstreamSourcePath = Join-Path $appDataRoot "com.vmjcv.arthook-next\images\upstream.png"
    $downloadHash = (Get-FileHash -LiteralPath $downloadPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $failedArtSourceHash = (Get-FileHash -LiteralPath $failedArtSourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $upstreamSourceHash = (Get-FileHash -LiteralPath $upstreamSourcePath -Algorithm SHA256).Hash.ToLowerInvariant()

    Assert-Equal $failedArtSourceHash $downloadHash "failed-art preview hash mismatch."
    Assert-True ($downloadHash -ne $upstreamSourceHash) "failed-art preview must not fall back to the upstream image hash."

    $summary = [ordered]@{
        status = "passed"
        packageDir = $packageFullPath
        evidenceRoot = $runDir
        daemonPort = $daemonPort
        daemonPid = $daemonPid
        failedArtNodeStatus = [string]$failedArtNode.status
        failedArtPreviewUrl = $previewUrl
        downloadedPreviewSha256 = $downloadHash
        failedArtSourceSha256 = $failedArtSourceHash
        upstreamSourceSha256 = $upstreamSourceHash
        previewMatchesFailedArtSource = ($downloadHash -eq $failedArtSourceHash)
        previewDiffersFromUpstream = ($downloadHash -ne $upstreamSourceHash)
        error = $null
    }
    Write-JsonFile -Path (Join-Path $runDir "summary.json") -Value $summary
    Write-Output ($summary | ConvertTo-Json -Depth 20)
}
catch {
    $failure = $_.Exception.ToString()
}
finally {
    if ($null -ne $daemonPid) {
        if (-not (Stop-ExactProcessById -ProcessId $daemonPid -ExpectedExecutablePath $daemonExe)) {
            $failure = (($failure, "Isolated daemon cleanup failed.") -join [Environment]::NewLine).Trim()
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($failure)) {
        $summary = [ordered]@{
            status = "failed"
            packageDir = $packageFullPath
            evidenceRoot = $runDir
            daemonPort = $daemonPort
            daemonPid = $daemonPid
            error = $failure
        }
        Write-JsonFile -Path (Join-Path $runDir "summary.json") -Value $summary
    }
}

if (-not [string]::IsNullOrWhiteSpace($failure)) {
    throw $failure
}
