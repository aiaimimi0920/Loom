[CmdletBinding()]
param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$EvidenceRoot = ".\target\plugin-boundary-smoke",
    [string]$HookRepository = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$script:LoomAuthorizationHeader = ""

. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")
. (Join-Path $PSScriptRoot "plugin-boundary-smoke\Packages.ps1")
. (Join-Path $PSScriptRoot "plugin-boundary-smoke\SourceIntegrity.ps1")
. (Join-Path $PSScriptRoot "plugin-boundary-smoke\Diagnostics.ps1")
. (Join-Path $PSScriptRoot "plugin-boundary-smoke\WebSocket.ps1")
. (Join-Path $PSScriptRoot "plugin-boundary-smoke\Cleanup.ps1")

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

function Resolve-HookRepository {
    param(
        [string]$LoomRepository,
        [string]$ExplicitPath
    )

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        $candidates += Resolve-RepoPath -Path $ExplicitPath
    } else {
        $candidates += [System.IO.Path]::GetFullPath((Join-Path $LoomRepository "..\Hook"))

        $superproject = (& git -C $LoomRepository rev-parse --show-superproject-working-tree 2>$null | Out-String).Trim()
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($superproject)) {
            $candidates += [System.IO.Path]::GetFullPath((Join-Path $superproject "Hook"))
        }

        $commonGitDir = (& git -C $LoomRepository rev-parse --path-format=absolute --git-common-dir 2>$null | Out-String).Trim()
        if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($commonGitDir)) {
            $submoduleMatch = [regex]::Match($commonGitDir, '^(?<root>.+)[\\/]\.git[\\/]modules[\\/].+$')
            if ($submoduleMatch.Success) {
                $candidates += [System.IO.Path]::GetFullPath((Join-Path $submoduleMatch.Groups['root'].Value "Hook"))
            }
        }
    }

    foreach ($candidate in @($candidates | Select-Object -Unique)) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Container)) {
            continue
        }
        $insideWorkTree = (& git -C $candidate rev-parse --is-inside-work-tree 2>$null | Out-String).Trim()
        if ($LASTEXITCODE -eq 0 -and [string]::Equals($insideWorkTree, "true", [System.StringComparison]::OrdinalIgnoreCase)) {
            return $candidate
        }
    }

    if (-not [string]::IsNullOrWhiteSpace($ExplicitPath)) {
        throw "Explicit Hook repository could not be resolved. Checked: $($candidates -join ', ')"
    }
    # Loom is also released as a standalone repository. The boundary smoke can validate Loom's
    # third-party lifecycle without a sibling checkout; when Hook is present, retain the stronger
    # source-integrity assertion below.
    return $null
}

function Write-Utf8NoBomFile {
    param([string]$Path, [string]$Content)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
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
    if (-not [string]::IsNullOrWhiteSpace($script:LoomAuthorizationHeader)) {
        $request.Headers["Authorization"] = $script:LoomAuthorizationHeader
    }
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
$hookRoot = Resolve-HookRepository -LoomRepository $repoRoot -ExplicitPath $HookRepository
$daemonPath = Resolve-RepoPath -Path $DaemonExecutable
$evidencePath = Resolve-RepoPath -Path $EvidenceRoot
$controlPlane = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-plugin-boundary-" + [guid]::NewGuid().ToString("N"))
$configuration = Join-Path $controlPlane "configuration"
$runStore = Join-Path $controlPlane "runs.sqlite3"
$packageRoot = Join-Path $controlPlane "third-party-packages"
$frameworkSourceRoot = Join-Path $packageRoot "source"
$frameworkStageV1 = Join-Path $packageRoot "framework-v1"
$frameworkStageV2 = Join-Path $packageRoot "framework-v2"
$artStage = Join-Path $packageRoot "art"
$thirdPartyFrameworkZipV1 = Join-Path $packageRoot "third-party-echo-framework-v1.zip"
$thirdPartyFrameworkZipV2 = Join-Path $packageRoot "third-party-echo-framework-v2.zip"
$thirdPartyArtZip = Join-Path $packageRoot "third-party-image-echo-art.zip"
$stdoutPath = Join-Path $controlPlane "daemon.stdout.log"
$stderrPath = Join-Path $controlPlane "daemon.stderr.log"
$port = Get-LoomSmokePort
$baseUrl = "http://127.0.0.1:$port"
$publisherId = "third.party"
$frameworkId = "third-party-echo"
$qualifiedFrameworkId = "$publisherId/$frameworkId"
$artId = "third-party-image-echo"
$qualifiedArtId = "$publisherId/$artId"
$loomBefore = Get-GitStateFingerprint -Repository $repoRoot
$hookBefore = if ($null -ne $hookRoot) {
    Get-GitStateFingerprint -Repository $hookRoot
} else {
    $null
}
$daemon = $null
$hookBridgeClient = $null
$hookBridgeRunning = $false
$succeeded = $false
$primaryFailure = $null
Reset-LoomPluginBoundaryDiagnostic
$oldEnvironment = @{}

foreach ($name in @("LOOM_DAEMON_HOST", "LOOM_DAEMON_PORT", "LOOM_DAEMON_TOKEN", "LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_RUN_STORE_PATH")) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

New-Item -ItemType Directory -Force -Path $evidencePath, $configuration, $frameworkSourceRoot, $frameworkStageV1, $frameworkStageV2, $artStage | Out-Null
if (Test-Path -LiteralPath (Join-Path $evidencePath "plugin-boundary-diagnostic.json")) {
    Remove-Item -LiteralPath (Join-Path $evidencePath "plugin-boundary-diagnostic.json") -Force
}
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"

try {
    New-ThirdPartyFrameworkPackage -Version "1.0.0" -StageRoot $frameworkStageV1 -SourcePath (Join-Path $frameworkSourceRoot "framework-v1.rs") -ZipPath $thirdPartyFrameworkZipV1 -FrameworkId $frameworkId -PublisherId $publisherId
    New-ThirdPartyFrameworkPackage -Version "2.0.0" -StageRoot $frameworkStageV2 -SourcePath (Join-Path $frameworkSourceRoot "framework-v2.rs") -ZipPath $thirdPartyFrameworkZipV2 -FrameworkId $frameworkId -PublisherId $publisherId

    $runtimeScript = @'
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$prefix = if ($null -ne $request.params.prefix) { [string]$request.params.prefix } else { [string]$request.inputs.prefix }
$inputValue = [string]$request.inputs.input
$frameworkVersion = [string]$env:THIRD_PARTY_FRAMEWORK_VERSION
$value = ($prefix + ":" + $inputValue + ":" + $frameworkVersion)
$response = [ordered]@{
    status = "success"
    output = [ordered]@{
        thirdParty = $true
        frameworkId = [string]$request.frameworkId
        artId = [string]$request.artId
        frameworkVersion = $frameworkVersion
        value = $value
        content = @(
            [ordered]@{ type = "text"; text = $value }
        )
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
            framework = $qualifiedFrameworkId
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
            dependencies = [ordered]@{ framework = $qualifiedFrameworkId }
            packageSecurity = [ordered]@{
                version = "1.0.0"
                publisher = [ordered]@{ id = $publisherId; name = "Third Party" }
            }
            capabilities = [ordered]@{
                preview = "image"
                parameterEditor = "generic"
            }
            art = [ordered]@{ qualifiedId = $qualifiedArtId }
        }
    }
    Write-Utf8NoBomFile -Path (Join-Path $artStage "manifest.json") -Content (($artManifest | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    Compress-Archive -Path (Join-Path $artStage "*") -DestinationPath $thirdPartyArtZip -CompressionLevel Optimal -Force

    $env:LOOM_DAEMON_HOST = "127.0.0.1"
    $env:LOOM_DAEMON_PORT = [string]$port
    $env:LOOM_DAEMON_TOKEN = [Guid]::NewGuid().ToString("N") + [Guid]::NewGuid().ToString("N")
    $script:LoomAuthorizationHeader = "Bearer $env:LOOM_DAEMON_TOKEN"
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

    $installedFramework = Install-Zip -Url "$baseUrl/v1/frameworks/install" -Path $thirdPartyFrameworkZipV1
    Assert-True ([string]$installedFramework.framework.id -eq $frameworkId) "Third-party framework install returned the wrong id."
    Assert-True ([string]$installedFramework.framework.version -eq "1.0.0") "Third-party framework install returned the wrong version."
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
    Assert-True ([string]$executed.result.value -eq "plugin:hello:1.0.0") "Third-party runtime did not receive inputs, params, and framework version."

    $upgradedFramework = Install-Zip -Url "$baseUrl/v1/frameworks/$frameworkId/upgrade" -Path $thirdPartyFrameworkZipV2
    Assert-True ([string]$upgradedFramework.framework.version -eq "2.0.0") "Third-party framework upgrade did not persist version 2.0.0."
    $staleFrameworkLockExecution = Invoke-LoomRaw -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ($staleFrameworkLockExecution.statusCode -eq 409) "Art with a stale framework lock did not return 409 after framework upgrade."
    Assert-True ([string]$staleFrameworkLockExecution.body.error.code -eq "art_package_integrity_failed") "Stale framework lock error was not named art_package_integrity_failed."
    Assert-True ([string]$staleFrameworkLockExecution.body.error.message -like "*locked framework*$frameworkId*version is no longer active*") "Stale framework lock error did not explain the inactive locked version."
    Install-Zip -Url "$baseUrl/v1/arts/install" -Path $thirdPartyArtZip | Out-Null
    $upgradedExecution = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ([string]$upgradedExecution.result.value -eq "plugin:hello:2.0.0") "Third-party framework upgrade did not replace runtime behavior."

    $capabilities = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    $hookCapability = @($capabilities.tools | Where-Object { [string]$_.id -eq $artId -and [bool]$_.enabled }) | Select-Object -First 1
    Assert-True ($null -ne $hookCapability) "Hook-facing enabled Art discovery did not include the third-party Art."
    Assert-True ([string]$hookCapability.execution.type -eq "framework_art") "Hook-facing capability did not preserve generic framework_art execution metadata."

    $hookBridgeStarted = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/hook-bridge/start" -Body @{ port = 0 }
    Assert-True ([bool]$hookBridgeStarted.running) "Hook Bridge did not start for the third-party Art."
    $hookBridgeRunning = $true
    Set-LoomPluginBoundaryPhase -Phase "connect"
    $hookBridgeClient = New-LoomHookBridgeWebSocket -Port ([int]$hookBridgeStarted.port)
    Set-LoomPluginBoundaryPhase -Phase "subscribe-send"
    Send-LoomHookBridgeWebSocketJson -Client $hookBridgeClient -Json '{"method":"loom.hook.subscribe","params":{"requestId":"subscribe:third-party-plugin","events":["loom.hook.workflow.instantiated","loom.hook.art.ack","loom.hook.art.progress","loom.hook.art.result","loom.hook.art.failure"]}}'
    Set-LoomPluginBoundaryPhase -Phase "subscribe-receive"
    $subscribed = Receive-LoomHookBridgeWebSocketJson -Client $hookBridgeClient
    Assert-True ([string]$subscribed.protocolVersion -eq "loom.hook.v1" -and [string]$subscribed.status -eq "succeeded") "Hook Bridge did not subscribe to Art node events."
    Reset-LoomPluginBoundaryDiagnostic
    $instantiated = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/hook-bridge/workflows/instantiate" -Body @{
        workflowId = "third-party-plugin-smoke"
        mode = "reference"
        nodes = @(
            @{ id = "third-party-node"; type = "artNode"; data = @{ artId = $qualifiedArtId; label = "Third-party Image Echo" } }
        )
        edges = @()
    }
    Assert-True ([string]$instantiated.status -eq "succeeded") "Third-party Hook node instantiation failed."
    Set-LoomPluginBoundaryPhase -Phase "instantiation-receive"
    $instantiatedBroadcast = Receive-LoomHookBridgeWebSocketJson -Client $hookBridgeClient
    Assert-True ([string]$instantiatedBroadcast.method -eq "loom.hook.workflow.instantiated") "Third-party Hook node broadcast was not emitted."
    Assert-True ([string]$instantiatedBroadcast.params.nodes[0].data.artId -eq $qualifiedArtId) "Hook node broadcast lost the dynamic Art id."
    $hookExecutionRequest = [ordered]@{
        method = "loom.hook.art.execute"
        params = [ordered]@{
            protocolVersion = "loom.hook.v1"
            requestId = "execute:third-party-plugin"
            nodeId = "third-party-node"
            artId = $qualifiedArtId
            generation = 1
            deviceId = "device:plugin-boundary"
            outputTransports = @("websocket")
            inputs = [ordered]@{ input = [ordered]@{ kind = "value"; value = "hello" } }
            parameters = [ordered]@{ prefix = "hook" }
            disabledParameters = @()
        }
    }
    Set-LoomPluginBoundaryPhase -Phase "execute-send"
    Send-LoomHookBridgeWebSocketJson -Client $hookBridgeClient -Json ($hookExecutionRequest | ConvertTo-Json -Depth 20 -Compress)
    Set-LoomPluginBoundaryPhase -Phase "execute-receive"
    $hookExecution = Receive-LoomHookBridgeExecutionResult -Client $hookBridgeClient
    Assert-True ([string]$hookExecution.status -eq "succeeded") "Third-party Art failed through the Hook Bridge."
    $hookOutput = @($hookExecution.data.outputs.PSObject.Properties | ForEach-Object { $_.Value })[0]
    Assert-True ([string]$hookOutput.kind -eq "value") "Hook Bridge did not return the formal value output kind."
    Assert-True (($hookOutput.value | ConvertTo-Json -Depth 20 -Compress) -like '*hook:hello:2.0.0*') "Hook Bridge did not execute the upgraded third-party framework and Art content."
    Reset-LoomPluginBoundaryDiagnostic
    Close-LoomHookBridgeWebSocket -Client $hookBridgeClient
    $hookBridgeClient = $null
    Invoke-LoomJson -Method Post -Url "$baseUrl/v1/hook-bridge/stop" -Body @{} | Out-Null
    $hookBridgeRunning = $false

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
    Assert-True ([string]$reinstalled.result.value -eq "plugin:hello:2.0.0") "Third-party Art did not execute after reinstall."

    Invoke-LoomJson -Method Post -Url "$baseUrl/v1/frameworks/$frameworkId/uninstall" -Body $null | Out-Null
    $uninstalledReadiness = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools/$artId/readiness" -Body $null
    Assert-True (-not [bool]$uninstalledReadiness.ready) "Art remained ready after framework uninstall."
    Install-Zip -Url "$baseUrl/v1/frameworks/install" -Path $thirdPartyFrameworkZipV2 | Out-Null
    $reinstalledFrameworkRun = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/tools/$artId/execute" -Body @{ arguments = $arguments }
    Assert-True ([string]$reinstalledFrameworkRun.result.value -eq "plugin:hello:2.0.0") "Art did not execute after framework reinstall."

    $loomAfter = Get-GitStateFingerprint -Repository $repoRoot
    Assert-True ($loomBefore -eq $loomAfter) "Third-party smoke changed Loom source state."
    if ($null -ne $hookRoot) {
        $hookAfter = Get-GitStateFingerprint -Repository $hookRoot
        Assert-True ($hookBefore -eq $hookAfter) "Third-party smoke changed Hook source state."
    }

    $succeeded = $true
    $evidence = [ordered]@{
        thirdPartyFrameworkInstalled = $true
        thirdPartyArtInstalled = $true
        thirdPartyArtExecuted = $true
        thirdPartyFrameworkCompiledOutsideRepository = $true
        frameworkUpgraded = $true
        frameworkLockRefreshRequired = $true
        hookCapabilityDiscovered = $true
        hookNodeInstantiated = $true
        hookBridgeExecuted = $true
        restarted = $true
        frameworkLifecycle = "install-upgrade-disable-enable-uninstall-reinstall"
        artLifecycle = "install-disable-enable-uninstall-reinstall"
        loomSourceChanged = $false
        hookSourceChanged = $false
        hookRepositoryChecked = ($null -ne $hookRoot)
        frameworkId = $qualifiedFrameworkId
        artId = $qualifiedArtId
    }
    Write-Utf8NoBomFile -Path (Join-Path $evidencePath "plugin-boundary-evidence.json") -Content (($evidence | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
    Write-Host "Plugin Art boundary smoke passed."
}
catch {
    $primaryFailure = $_
    Save-LoomPluginBoundaryDiagnostic -EvidencePath $evidencePath -Failure $_
    throw
}
finally {
    Complete-LoomPluginBoundaryCleanup -PrimaryFailure $primaryFailure -EvidencePath $evidencePath -Actions @({
        Close-LoomHookBridgeWebSocket -Client $hookBridgeClient
    }, {
        if ($hookBridgeRunning -and $null -ne $daemon -and -not $daemon.HasExited) {
            Invoke-LoomJson -Method Post -Url "$baseUrl/v1/hook-bridge/stop" -Body @{} | Out-Null
        }
    }, {
        Stop-TestDaemon -Process $daemon
    }, {
        if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) {
            Copy-Item -LiteralPath $stdoutPath -Destination (Join-Path $evidencePath "daemon.stdout.log") -Force
        }
    }, {
        if (Test-Path -LiteralPath $stderrPath -PathType Leaf) {
            Copy-Item -LiteralPath $stderrPath -Destination (Join-Path $evidencePath "daemon.stderr.log") -Force
        }
    }, {
        foreach ($name in $oldEnvironment.Keys) {
            [System.Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name])
        }
    }, {
        $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
        $controlFull = [System.IO.Path]::GetFullPath($controlPlane)
        if ($controlFull.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $controlPlane)) {
            Remove-Item -LiteralPath $controlPlane -Recurse -Force
        }
    })
}

if (-not $succeeded) {
    throw "Plugin Art boundary smoke did not complete successfully. Evidence: $evidencePath"
}
