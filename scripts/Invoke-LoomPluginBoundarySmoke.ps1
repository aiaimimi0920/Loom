[CmdletBinding()]
param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks",
    [string]$EvidenceRoot = ".\target\plugin-boundary-smoke"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Resolve-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Write-Utf8NoBomFile {
    param([string]$Path, [string]$Content)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function Get-GitStateFingerprint {
    param([string]$Repository)
    $head = (& git -C $Repository rev-parse HEAD 2>$null | Out-String).Trim()
    $status = (& git -C $Repository status --porcelain=v1 --untracked-files=all 2>$null | Out-String).Trim()
    return "$head|$status"
}

function Invoke-LoomRaw {
    param(
        [string]$Method,
        [string]$Url,
        [AllowNull()][object]$Body
    )
    $request = [System.Net.HttpWebRequest]::Create($Url)
    $request.Method = $Method.ToUpperInvariant()
    $request.Timeout = 120000
    $request.ReadWriteTimeout = 120000
    $request.ContentType = "application/json"
    if ($null -ne $Body) {
        $json = $Body | ConvertTo-Json -Depth 50 -Compress
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
        $request.ContentLength = $bytes.Length
        $stream = $request.GetRequestStream()
        try {
            $stream.Write($bytes, 0, $bytes.Length)
        }
        finally {
            $stream.Dispose()
        }
    }
    $response = $null
    $statusCode = 0
    try {
        $response = $request.GetResponse()
        $statusCode = [int]$response.StatusCode
    }
    catch [System.Net.WebException] {
        if ($null -eq $_.Exception.Response) {
            throw
        }
        $response = $_.Exception.Response
        $statusCode = [int]$response.StatusCode
    }
    $reader = [System.IO.StreamReader]::new($response.GetResponseStream())
    try {
        $text = $reader.ReadToEnd()
    }
    finally {
        $reader.Dispose()
        $response.Dispose()
    }
    $bodyValue = if ([string]::IsNullOrWhiteSpace($text)) {
        $null
    }
    else {
        $text | ConvertFrom-Json
    }
    return [pscustomobject]@{ statusCode = $statusCode; body = $bodyValue }
}

function Invoke-LoomJson {
    param(
        [string]$Method,
        [string]$Url,
        [AllowNull()][object]$Body
    )
    $result = Invoke-LoomRaw -Method $Method -Url $Url -Body $Body
    if ($result.statusCode -lt 200 -or $result.statusCode -ge 300) {
        throw "Loom request failed ($($result.statusCode)) $Method $($Url): $($result.body | ConvertTo-Json -Depth 20 -Compress)"
    }
    return $result.body
}

function Encode-ZipDataUrl {
    param([string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
}

function Install-Zip {
    param([string]$Url, [string]$Path)
    return Invoke-LoomJson -Method Post -Url $Url -Body @{
        zipBase64 = Encode-ZipDataUrl -Path $Path
    }
}

function Copy-ZipEntry {
    param([string]$ZipPath, [string]$EntryName, [string]$Destination)
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        $normalizedEntryName = $EntryName.Replace('\', '/')
        $entry = @($archive.Entries | Where-Object {
            $_.FullName.Replace('\', '/') -eq $normalizedEntryName
        }) | Select-Object -First 1
        Assert-True ($null -ne $entry) "Framework artifact is missing $EntryName."
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
        $source = $entry.Open()
        $target = [System.IO.File]::Create($Destination)
        try {
            $source.CopyTo($target)
        }
        finally {
            $target.Dispose()
            $source.Dispose()
        }
    }
    finally {
        $archive.Dispose()
    }
}

function Start-TestDaemon {
    param(
        [string]$Executable,
        [string]$WorkingDirectory,
        [string]$BaseUrl,
        [string]$StdoutPath,
        [string]$StderrPath
    )
    $process = Start-Process -FilePath $Executable -WorkingDirectory $WorkingDirectory -WindowStyle Hidden -PassThru -RedirectStandardOutput $StdoutPath -RedirectStandardError $StderrPath
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        Start-Sleep -Milliseconds 250
        try {
            $health = Invoke-LoomRaw -Method Get -Url "$BaseUrl/health" -Body $null
            if ($health.statusCode -eq 200) {
                return $process
            }
        }
        catch {
            if ($process.HasExited) {
                throw "Loom daemon exited before readiness. See $StdoutPath and $StderrPath."
            }
        }
    }
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    throw "Loom daemon did not become ready at $BaseUrl."
}

function Stop-TestDaemon {
    param([AllowNull()][System.Diagnostics.Process]$Process)
    if ($null -ne $Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        $null = $Process.WaitForExit(5000)
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$hookRoot = Join-Path $repoRoot "..\Hook"
$daemonPath = Resolve-RepoPath -Path $DaemonExecutable
$frameworkRootPath = Resolve-RepoPath -Path $FrameworkArtifactRoot
$evidencePath = Resolve-RepoPath -Path $EvidenceRoot
$scriptFrameworkZip = Join-Path $frameworkRootPath "script.zip"
$controlPlane = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-plugin-boundary-" + [guid]::NewGuid().ToString("N"))
$configuration = Join-Path $controlPlane "configuration"
$runStore = Join-Path $controlPlane "runs.sqlite3"
$packageRoot = Join-Path $controlPlane "third-party-packages"
$frameworkStage = Join-Path $packageRoot "framework"
$artStage = Join-Path $packageRoot "art"
$thirdPartyFrameworkZip = Join-Path $packageRoot "third-party-echo-framework.zip"
$thirdPartyArtZip = Join-Path $packageRoot "third-party-image-echo-art.zip"
$stdoutPath = Join-Path $controlPlane "daemon.stdout.log"
$stderrPath = Join-Path $controlPlane "daemon.stderr.log"
$port = Get-FreePort
$baseUrl = "http://127.0.0.1:$port"
$frameworkId = "third-party-echo"
$artId = "third-party-image-echo"
$loomBefore = Get-GitStateFingerprint -Repository $repoRoot
$hookBefore = Get-GitStateFingerprint -Repository $hookRoot
$daemon = $null
$succeeded = $false
$oldEnvironment = @{}

foreach ($name in @("LOOM_DAEMON_HOST", "LOOM_DAEMON_PORT", "LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_RUN_STORE_PATH")) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

New-Item -ItemType Directory -Force -Path $evidencePath, $configuration, $frameworkStage, $artStage | Out-Null
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"
Assert-True (Test-Path -LiteralPath $scriptFrameworkZip -PathType Leaf) "script framework artifact not found: $scriptFrameworkZip"

try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    $frameworkRuntime = Join-Path $frameworkStage "runtime\loom-framework-third-party.exe"
    Copy-ZipEntry -ZipPath $scriptFrameworkZip -EntryName "runtime/loom-framework-script.exe" -Destination $frameworkRuntime
    $frameworkManifest = [ordered]@{
        id = $frameworkId
        name = "Third-party Echo Framework"
        description = "Framework package created outside the Loom source tree."
        version = "1.0.0"
        protocolVersion = "loom.framework.v1"
        platforms = @("windows-x64")
        entry = [ordered]@{
            kind = "process"
            command = "runtime/loom-framework-third-party.exe"
            args = @("--framework-id", $frameworkId)
        }
        permissions = @("process.spawn", "file.read", "file.write")
        artExecution = [ordered]@{
            requestSchema = "loom.art.execute.v1"
            responseSchema = "loom.art.result.v1"
        }
    }
    Write-Utf8NoBomFile -Path (Join-Path $frameworkStage "framework.manifest.json") -Content (($frameworkManifest | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    Compress-Archive -Path (Join-Path $frameworkStage "*") -DestinationPath $thirdPartyFrameworkZip -CompressionLevel Optimal -Force

    $runtimeScript = @'
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$prefix = [string]$request.params.prefix
$inputValue = [string]$request.inputs.input
$response = [ordered]@{
    status = "success"
    output = [ordered]@{
        thirdParty = $true
        frameworkId = [string]$request.frameworkId
        artId = [string]$request.artId
        value = ($prefix + ":" + $inputValue)
    }
}
[Console]::Out.Write(($response | ConvertTo-Json -Depth 30 -Compress))
'@
    Write-Utf8NoBomFile -Path (Join-Path $artStage "runtime\main.ps1") -Content $runtimeScript
    $runtimeManifest = [ordered]@{
        protocolVersion = "loom.art.runtime.v1"
        entry = [ordered]@{
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "runtime/main.ps1")
        }
    }
    Write-Utf8NoBomFile -Path (Join-Path $artStage "art.runtime.json") -Content (($runtimeManifest | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    $artManifest = [ordered]@{
        id = $artId
        name = "Third-party Image Echo"
        description = "An Art package generated outside Loom and Hook source."
        enabled = $true
        execution = [ordered]@{
            type = "framework_art"
            framework = $frameworkId
        }
        inputs = @(
            [ordered]@{ name = "input"; label = "Input"; type = "text" }
        )
        outputs = @(
            [ordered]@{ name = "output"; label = "Output"; type = "value" }
        )
        params = @(
            [ordered]@{ id = "prefix"; label = "Prefix"; widget = "text"; default = "third-party" }
        )
        metadata = [ordered]@{
            dependencies = [ordered]@{ framework = $frameworkId }
            capabilities = [ordered]@{
                preview = "image"
                parameterEditor = "generic"
            }
        }
    }
    Write-Utf8NoBomFile -Path (Join-Path $artStage "manifest.json") -Content (($artManifest | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    Compress-Archive -Path (Join-Path $artStage "*") -DestinationPath $thirdPartyArtZip -CompressionLevel Optimal -Force

    $env:LOOM_DAEMON_HOST = "127.0.0.1"
    $env:LOOM_DAEMON_PORT = [string]$port
    $env:LOOM_CONTROL_PLANE_ROOT = $controlPlane
    $env:LOOM_CONFIGURATION_ROOT = $configuration
    $env:LOOM_RUN_STORE_PATH = $runStore
    $daemon = Start-TestDaemon -Executable $daemonPath -WorkingDirectory $repoRoot -BaseUrl $baseUrl -StdoutPath $stdoutPath -StderrPath $stderrPath

    $initial = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/frameworks" -Body $null
    $initialInstalled = @($initial.frameworks | Where-Object { [bool]$_.installed })
    Assert-True ($initialInstalled.Count -eq 0) "Fresh control plane has installed frameworks."

    $notReady = Invoke-LoomRaw -Method Post -Url "$baseUrl/v1/arts/install" -Body @{
        zipBase64 = Encode-ZipDataUrl -Path $thirdPartyArtZip
    }
    Assert-True ($notReady.statusCode -eq 409) "Art install without framework did not return 409."
    Assert-True ([string]$notReady.body.error.code -eq "framework_not_ready") "Missing framework error was not named framework_not_ready."

    $installedFramework = Install-Zip -Url "$baseUrl/v1/frameworks/install" -Path $thirdPartyFrameworkZip
    Assert-True ([string]$installedFramework.framework.id -eq $frameworkId) "Third-party framework install returned the wrong id."
    $frameworkStatus = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/frameworks" -Body $null
    $dynamicStatus = @($frameworkStatus.frameworks | Where-Object { [string]$_.id -eq $frameworkId }) | Select-Object -First 1
    Assert-True ($null -ne $dynamicStatus -and [bool]$dynamicStatus.ready) "Third-party framework is not ready."

    $installedArt = Install-Zip -Url "$baseUrl/v1/arts/install" -Path $thirdPartyArtZip
    Assert-True ([string]$installedArt.report.toolId -eq $artId) "Third-party Art install returned the wrong id."
    $tools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    Assert-True (@($tools.tools | Where-Object { [string]$_.id -eq $artId }).Count -eq 1) "Third-party Art was not listed."
    $installedTool = @($tools.tools | Where-Object { [string]$_.id -eq $artId }) | Select-Object -First 1

    $arguments = @{ inputs = @{ input = "hello" }; params = @{ prefix = "plugin" } }
    $executed = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ([string]$executed.status -eq "succeeded") "Third-party Art execution failed: $($executed | ConvertTo-Json -Depth 30 -Compress)"
    Assert-True ([bool]$executed.result.thirdParty) "Third-party runtime marker is missing."
    Assert-True ([string]$executed.result.value -eq "plugin:hello") "Third-party runtime did not receive inputs and params."

    $disabledFramework = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/frameworks/$frameworkId/disable" -Body $null
    Assert-True (-not [bool]$disabledFramework.framework.ready) "Third-party framework disable did not make it unready."
    $readiness = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools/$artId/readiness" -Body $null
    Assert-True (-not [bool]$readiness.ready) "Dependent Art remained ready after framework disable."
    $disabledExecution = Invoke-LoomRaw -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ($disabledExecution.statusCode -ge 400) "Execution unexpectedly succeeded while framework was disabled."

    $enabledFramework = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/frameworks/$frameworkId/enable" -Body $null
    Assert-True ([bool]$enabledFramework.framework.ready) "Third-party framework re-enable failed."
    $installedTool.enabled = $false
    $disabledArt = Invoke-LoomJson -Method Put -Url "$baseUrl/v1/tools/$artId" -Body $installedTool
    Assert-True (-not [bool]$disabledArt.tool.enabled) "Third-party Art disable failed."
    $disabledArtExecution = Invoke-LoomRaw -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ($disabledArtExecution.statusCode -ge 400) "Disabled Art unexpectedly executed."
    $installedTool.enabled = $true
    $enabledArt = Invoke-LoomJson -Method Put -Url "$baseUrl/v1/tools/$artId" -Body $installedTool
    Assert-True ([bool]$enabledArt.tool.enabled) "Third-party Art re-enable failed."

    Stop-TestDaemon -Process $daemon
    $daemon = Start-TestDaemon -Executable $daemonPath -WorkingDirectory $repoRoot -BaseUrl $baseUrl -StdoutPath $stdoutPath -StderrPath $stderrPath
    $afterRestartFrameworks = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/frameworks" -Body $null
    $afterRestartFramework = @($afterRestartFrameworks.frameworks | Where-Object { [string]$_.id -eq $frameworkId }) | Select-Object -First 1
    Assert-True ($null -ne $afterRestartFramework -and [bool]$afterRestartFramework.ready) "Third-party framework was not restored after restart."
    $afterRestartTools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    Assert-True (@($afterRestartTools.tools | Where-Object { [string]$_.id -eq $artId }).Count -eq 1) "Third-party Art was not restored after restart."

    $deleted = Invoke-LoomRaw -Method Delete -Url "$baseUrl/v1/tools/$artId" -Body $null
    Assert-True ($deleted.statusCode -ge 200 -and $deleted.statusCode -lt 300) "Third-party Art uninstall failed: status=$($deleted.statusCode) body=$($deleted.body | ConvertTo-Json -Depth 20 -Compress)"
    $missingTools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    Assert-True (@($missingTools.tools | Where-Object { [string]$_.id -eq $artId }).Count -eq 0) "Third-party Art remained after uninstall."

    Install-Zip -Url "$baseUrl/v1/arts/install" -Path $thirdPartyArtZip | Out-Null
    $reinstalled = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ([string]$reinstalled.result.value -eq "plugin:hello") "Third-party Art did not execute after reinstall."

    Invoke-LoomJson -Method Post -Url "$baseUrl/v1/frameworks/$frameworkId/uninstall" -Body $null | Out-Null
    $uninstalledReadiness = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools/$artId/readiness" -Body $null
    Assert-True (-not [bool]$uninstalledReadiness.ready) "Art remained ready after framework uninstall."
    Install-Zip -Url "$baseUrl/v1/frameworks/install" -Path $thirdPartyFrameworkZip | Out-Null
    $reinstalledFrameworkRun = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ([string]$reinstalledFrameworkRun.result.value -eq "plugin:hello") "Art did not execute after framework reinstall."

    $loomAfter = Get-GitStateFingerprint -Repository $repoRoot
    $hookAfter = Get-GitStateFingerprint -Repository $hookRoot
    Assert-True ($loomBefore -eq $loomAfter) "Third-party smoke changed Loom source state."
    Assert-True ($hookBefore -eq $hookAfter) "Third-party smoke changed Hook source state."

    $succeeded = $true
    $evidence = [ordered]@{
        thirdPartyFrameworkInstalled = $true
        thirdPartyArtInstalled = $true
        thirdPartyArtExecuted = $true
        restarted = $true
        frameworkLifecycle = "install-disable-enable-uninstall-reinstall"
        artLifecycle = "install-disable-enable-uninstall-reinstall"
        loomSourceChanged = $false
        hookSourceChanged = $false
        frameworkId = $frameworkId
        artId = $artId
    }
    Write-Utf8NoBomFile -Path (Join-Path $evidencePath "plugin-boundary-evidence.json") -Content (($evidence | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
    Write-Host "Plugin Art boundary smoke passed."
}
finally {
    Stop-TestDaemon -Process $daemon
    if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) {
        Copy-Item -LiteralPath $stdoutPath -Destination (Join-Path $evidencePath "daemon.stdout.log") -Force
    }
    if (Test-Path -LiteralPath $stderrPath -PathType Leaf) {
        Copy-Item -LiteralPath $stderrPath -Destination (Join-Path $evidencePath "daemon.stderr.log") -Force
    }
    foreach ($name in $oldEnvironment.Keys) {
        [System.Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name])
    }
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    $controlFull = [System.IO.Path]::GetFullPath($controlPlane)
    if ($controlFull.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $controlPlane)) {
        Remove-Item -LiteralPath $controlPlane -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if (-not $succeeded) {
    throw "Plugin Art boundary smoke did not complete successfully. Evidence: $evidencePath"
}
