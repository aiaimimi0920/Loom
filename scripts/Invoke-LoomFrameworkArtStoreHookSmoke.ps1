[CmdletBinding()]
param(
    [string]$PackageDir = "",
    [string]$EvidenceRoot = ".\target\framework-art-store-hook-smoke",
    [ValidateSet("Debug", "Release")][string]$Configuration = "Debug",
    [switch]$SkipBuild,
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:DaemonRequestHeaders = @{}

$moduleRoot = Join-Path $PSScriptRoot "framework-art-store-hook-smoke"
$libraryModules = @(
    "Assertions.ps1",
    "Paths.ps1",
    "Process.ps1",
    "Http.ps1",
    "HookBridge.ps1",
    "FixtureArchive.ps1"
)
foreach ($moduleName in $libraryModules) {
    $modulePath = Join-Path $moduleRoot $moduleName
    if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
        throw "Missing framework/store Hook smoke module: $modulePath"
    }
    $moduleItem = Get-Item -LiteralPath $modulePath -Force
    if (($moduleItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Framework/store Hook smoke modules must not be reparse points: $modulePath"
    }
    . $moduleItem.FullName
}
$imageSearchLabel = ConvertFrom-UnicodeCodePoints @(0x56FE, 0x7247, 0x641C, 0x7D22)

$repoRoot = Resolve-SmokeRealDirectory -Path (Join-Path $PSScriptRoot "..") -Label "Loom repository"
$frameworkIds = @(
    "process",
    "cloud_api",
    "mcp",
    "workflow"
)
$smokePortHelperPath = Resolve-SmokeRealFile -Path (Join-Path $repoRoot "scripts\LoomSmokePorts.ps1") -Label "smoke port helper"
. $smokePortHelperPath
$packageFullPath = $null
if (-not [string]::IsNullOrWhiteSpace($PackageDir)) {
    $packageCandidate = if ([System.IO.Path]::IsPathRooted($PackageDir)) {
        [System.IO.Path]::GetFullPath($PackageDir)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $repoRoot $PackageDir))
    }
    $packageFullPath = Resolve-SmokeRealDirectory -Path $packageCandidate -Label "Loom package"
}
$frameworkArtifactFullPath = if ($null -ne $packageFullPath -and -not $PSBoundParameters.ContainsKey("FrameworkArtifactRoot")) {
    [System.IO.Path]::GetFullPath((Join-Path $packageFullPath "packages\frameworks"))
} elseif ([System.IO.Path]::IsPathRooted($FrameworkArtifactRoot)) {
    [System.IO.Path]::GetFullPath($FrameworkArtifactRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $FrameworkArtifactRoot))
}
$evidenceRootCandidate = if ([System.IO.Path]::IsPathRooted($EvidenceRoot)) {
    [System.IO.Path]::GetFullPath($EvidenceRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $EvidenceRoot))
}
$EvidenceRoot = Initialize-SmokeRealDirectory -Path $evidenceRootCandidate -Label "smoke evidence root"
$frameworkArtifactFullPath = Resolve-SmokeRealDirectory -Path $frameworkArtifactFullPath -Label "framework artifact root"

$buildArgs = @("build", "-p", "loom-daemon", "-p", "loom-art-store")
$targetSubdir = "debug"
if ($Configuration -eq "Release") {
    $buildArgs = @("build", "--release", "-p", "loom-daemon", "-p", "loom-art-store")
    $targetSubdir = "release"
}

$daemonExe = Join-Path $repoRoot "target\$targetSubdir\loom-daemon.exe"
$daemonWorkingDirectory = $repoRoot
$artStoreExe = Join-Path $repoRoot "target\$targetSubdir\loom-art-store.exe"
$embeddedPython = Join-Path $repoRoot "resources\python-embed\python.exe"
$pythonResourcesRoot = Join-Path $repoRoot "resources\python"
$isWindows = ($env:OS -eq "Windows_NT")
$fixturePythonCommand = $null
$fixturePythonArgsPrefix = @()

$pythonCommand = Get-Command python -ErrorAction SilentlyContinue
if ($null -ne $pythonCommand) {
    $fixturePythonCommand = $pythonCommand.Source
} else {
    $pyLauncher = Get-Command py -ErrorAction SilentlyContinue
    if ($null -ne $pyLauncher) {
        $fixturePythonCommand = $pyLauncher.Source
        $fixturePythonArgsPrefix = @("-3")
    }
}

Assert-True (-not [string]::IsNullOrWhiteSpace($fixturePythonCommand)) "No host Python interpreter was found for the temporary cloud/MCP fixture servers."
$embeddedPython = Resolve-SmokeRealFile -Path $embeddedPython -Label "packaged Python runtime"
[void](Resolve-SmokeRealFile -Path (Join-Path $pythonResourcesRoot "Launcher.py") -Label "packaged Python launcher")
$fixturePythonCommand = Resolve-SmokeRealFile -Path $fixturePythonCommand -Label "host Python interpreter"

if ($null -ne $packageFullPath) {
    $daemonExe = Join-Path $packageFullPath "runtime\loom-daemon.exe"
    $daemonWorkingDirectory = $packageFullPath
    $buildArgs = @("build", "-p", "loom-art-store")
    if ($Configuration -eq "Release") {
        $buildArgs = @("build", "--release", "-p", "loom-art-store")
    }
}

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo $($buildArgs -join ' ') failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

$daemonExe = Resolve-SmokeRealFile -Path $daemonExe -Label "daemon binary"
$daemonWorkingDirectory = Resolve-SmokeRealDirectory -Path $daemonWorkingDirectory -Label "daemon working directory"
$artStoreExe = Resolve-SmokeRealFile -Path $artStoreExe -Label "art store binary"

$runId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-framework-store-$PID-$([System.Guid]::NewGuid().ToString('N'))"
$runRoot = Initialize-SmokeRealDirectory -Path (Join-Path $EvidenceRoot $runId) -Label "smoke run root"
$storeRoot = Join-Path $runRoot "store"
$controlPlaneRoot = Join-Path $runRoot "control-plane"
$appDataRoot = Join-Path $runRoot "appdata"
$localAppDataRoot = Join-Path $runRoot "localappdata"
$logsRoot = Join-Path $runRoot "logs"
$fixturesRoot = Join-Path $runRoot "fixtures"
$summaryPath = Join-Path $runRoot "summary.json"

New-Item -ItemType Directory -Force -Path $storeRoot, $controlPlaneRoot, $appDataRoot, $localAppDataRoot, $logsRoot, $fixturesRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $storeRoot "arts"), (Join-Path $storeRoot "frameworks"), (Join-Path $storeRoot "binaries") | Out-Null

foreach ($frameworkId in $frameworkIds) {
    $sourceZip = Join-Path $frameworkArtifactFullPath "$frameworkId.zip"
    $sourceZip = Resolve-SmokeRealFile -Path $sourceZip -Label "framework package artifact $frameworkId"
    Copy-Item -LiteralPath $sourceZip -Destination (Join-Path $storeRoot "frameworks\$frameworkId.zip") -Force
}

$cloudPort = Get-LoomSmokePort
$storePort = Get-LoomSmokePort
$daemonPort = Get-LoomSmokePort

$imageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
$secondImageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAEElEQVR4AQEFAPr/AAoUHv8BpAE8tOS4KAAAAABJRU5ErkJggg=="
$cloudEvidencePath = Join-Path $fixturesRoot "cloud-request.json"
$mcpEvidencePath = Join-Path $fixturesRoot "mcp-request.json"
$cloudScriptPath = Join-Path $fixturesRoot "fake-cloud-server.py"
$mcpScriptPath = Join-Path $fixturesRoot "fake-mcp-server.py"
$mcpPackagePath = Join-Path $fixturesRoot "store-fixture-mcp.zip"

$serviceFixtureModule = Resolve-SmokeRealFile -Path (Join-Path $moduleRoot "ServiceFixtures.ps1") -Label "service fixture module"
$artFixtureModule = Resolve-SmokeRealFile -Path (Join-Path $moduleRoot "ArtFixtures.ps1") -Label "Art fixture module"
. $serviceFixtureModule
. $artFixtureModule
$catalogExpectedIds = @(
    "store-cli-art",
    "store-cloud-art",
    "store-mcp-art",
    "store-python-art",
    "store-script-art",
    "store-workflow-art"
)

$cloudProcess = $null
$artStoreProcess = $null
$daemonProcess = $null
$baseUrl = $null
$daemonToken = [Guid]::NewGuid().ToString("N") + [Guid]::NewGuid().ToString("N")
$script:DaemonRequestHeaders = @{ Authorization = "Bearer $daemonToken" }
$summary = [ordered]@{
    runId = $runId
    configuration = $Configuration
    packageDir = $packageFullPath
    runRoot = $runRoot
    storePort = $storePort
    daemonPort = $daemonPort
    cloudPort = $cloudPort
    catalogExpectedIds = $catalogExpectedIds
}
$cleanupFailures = New-Object System.Collections.ArrayList

try {
    $cloudProcess = Start-SmokeProcess `
        -FilePath $fixturePythonCommand `
        -ArgumentList @($fixturePythonArgsPrefix + @($cloudScriptPath, "$cloudPort", $cloudEvidencePath, $imageData, $secondImageData)) `
        -WorkingDirectory $repoRoot `
        -StdoutPath (Join-Path $logsRoot "cloud.stdout.log") `
        -StderrPath (Join-Path $logsRoot "cloud.stderr.log")
    Wait-TcpPort -HostName "127.0.0.1" -Port $cloudPort -Message "Cloud fixture did not open its TCP port"

    $artStoreProcess = Start-InheritedEnvProcess `
        -FilePath $artStoreExe `
        -WorkingDirectory $repoRoot `
        -Environment @{
            LOOM_ART_STORE_HOST = "127.0.0.1"
            LOOM_ART_STORE_PORT = "$storePort"
            LOOM_ART_STORE_ROOT = $storeRoot
        } `
        -StdoutPath (Join-Path $logsRoot "art-store.stdout.log") `
        -StderrPath (Join-Path $logsRoot "art-store.stderr.log")
    [void](Wait-HttpJson -Uri "http://127.0.0.1:$storePort/health" -Message "Art store did not become healthy")

    $daemonProcess = Start-InheritedEnvProcess `
        -FilePath $daemonExe `
        -WorkingDirectory $daemonWorkingDirectory `
        -Environment @{
            LOOM_DAEMON_HOST = "127.0.0.1"
            LOOM_DAEMON_PORT = "$daemonPort"
            LOOM_DAEMON_TOKEN = $daemonToken
            LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
            LOOM_ART_STORE_URL = "http://127.0.0.1:$storePort"
            LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES = "1"
            APPDATA = $appDataRoot
            LOCALAPPDATA = $localAppDataRoot
        } `
        -StdoutPath (Join-Path $logsRoot "daemon.stdout.log") `
        -StderrPath (Join-Path $logsRoot "daemon.stderr.log")
    [void](Wait-HttpJson -Uri "http://127.0.0.1:$daemonPort/health" -Message "Loom daemon did not become healthy")

    $baseUrl = "http://127.0.0.1:$daemonPort"
    $summary.baseUrl = $baseUrl

    $frameworksBefore = Invoke-JsonGet -Uri "$baseUrl/v1/frameworks"
    Assert-Equal 4 (@($frameworksBefore.frameworks).Count) "Framework list must expose exactly four framework ids."
    $summary.frameworksBefore = $frameworksBefore.frameworks

    $catalog = Invoke-JsonGet -Uri "$baseUrl/v1/arts/store/catalog"
    $catalogIds = @($catalog.arts | ForEach-Object { [string]$_.id })
    Assert-Equal ($catalogExpectedIds -join ",") (($catalogIds | Sort-Object) -join ",") "Store catalog ids mismatch."
    $summary.catalog = $catalog.arts

    $savedServer = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/servers/install" -Body @{
        zipBase64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($mcpPackagePath))
    }
    Assert-Equal "store-fixture" ([string]$savedServer.server.id) "MCP package install id mismatch."
    Assert-Equal "package" ([string]$savedServer.server.source) "MCP smoke fixture must use the independent package lifecycle."
    Assert-Equal "neuro.official/store-fixture" ([string]$savedServer.server.package.qualifiedId) "MCP package identity mismatch."
    $summary.mcpServer = $savedServer.server

    $frameworkInstallReports = @{}
    foreach ($frameworkId in $frameworkIds) {
        $frameworkReport = Invoke-JsonPost -Uri "$baseUrl/v1/frameworks/$frameworkId/install" -Body @{}
        Assert-Equal $frameworkId ([string]$frameworkReport.framework.id) "Framework install id mismatch for $frameworkId."
        Assert-Equal $true ([bool]$frameworkReport.framework.ready) "Framework should be ready after install: $frameworkId."
        $frameworkInstallReports[$frameworkId] = $frameworkReport.framework
    }
    $installedFrameworkPackageText = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5bey5a6J6KOF5qGG5p625YyF"))
    Assert-Contains $installedFrameworkPackageText ([string]$frameworkInstallReports['process'].readyDetail) "process ready detail should describe the installed framework package."
    $summary.frameworkInstallReports = $frameworkInstallReports

    $frameworksAfter = Invoke-JsonGet -Uri "$baseUrl/v1/frameworks"
    $summary.frameworksAfter = $frameworksAfter.frameworks

    $installOrder = @(
        "store-cli-art",
        "store-script-art",
        "store-cloud-art",
        "store-python-art",
        "store-mcp-art",
        "store-workflow-art"
    )
    $installReports = @{}
    foreach ($artId in $installOrder) {
        $installed = Invoke-JsonPost -Uri "$baseUrl/v1/arts/store/install" -Body @{
            artId = $artId
        }
        Assert-Equal $artId ([string]$installed.reports[0].toolId) "Installed art id mismatch for $artId."
        $installReports[$artId] = $installed.reports
    }
    $summary.installReports = $installReports

    $installedWorkflow = Invoke-JsonGet -Uri "$baseUrl/v1/workflows/store-script-workflow"
    Assert-Equal "store-script-workflow" ([string]$installedWorkflow.workflow.id) "Packaged workflow registration id mismatch."
    $summary.workflow = $installedWorkflow.workflow

    $readinessChecks = @{}
    foreach ($artId in $installOrder) {
        $readiness = Invoke-JsonGet -Uri "$baseUrl/v1/tools/$artId/readiness"
        Assert-Equal $true ([bool]$readiness.frameworkInstalled) "Tool readiness must report installed framework for $artId."
        Assert-Equal $true ([bool]$readiness.ready) "Tool readiness must report ready framework for $artId."
        $readinessChecks[$artId] = $readiness
    }
    $summary.toolReadiness = $readinessChecks

    $hookBridgeStarted = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/start" -Body @{ port = 0 }
    Assert-Equal $true ([bool]$hookBridgeStarted.running) "Hook Bridge should start."
    $summary.hookBridge = $hookBridgeStarted
    $hookBridgeRunning = $true

    $subscriber = $null
    try {
        $subscriber = New-LoomHookBridgeWebSocket -Port ([int]$hookBridgeStarted.port)
        Send-LoomHookBridgeWebSocketJson `
            -Client $subscriber `
            -Json '{"method":"loom.hook.subscribe","params":{"requestId":"subscribe:release-framework-smoke","events":["loom.hook.workflow.instantiated","loom.hook.workflow.updated","loom.hook.art.ack","loom.hook.art.progress","loom.hook.art.preview","loom.hook.art.result","loom.hook.art.failure"]}}'
        $subscribed = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "loom.hook.v1" ([string]$subscribed.protocolVersion) "Hook Bridge subscribe protocol mismatch."
        Assert-Equal "succeeded" ([string]$subscribed.status) "Hook Bridge subscribe status mismatch."

        $nodes = @(
            @{ id = "node-cli"; type = "artNode"; data = @{ artId = "neuro.official/store-cli-art"; label = "Store CLI Art" } },
            @{ id = "node-script"; type = "artNode"; data = @{ artId = "neuro.official/store-script-art"; label = "Store Script Art" } },
            @{ id = "node-cloud"; type = "artNode"; data = @{ artId = "neuro.official/store-cloud-art"; label = "Store Cloud Art" } },
            @{ id = "node-python"; type = "artNode"; data = @{ artId = "neuro.official/store-python-art"; label = "Store Python Art" } },
            @{ id = "node-mcp"; type = "artNode"; data = @{ artId = "neuro.official/store-mcp-art"; label = "Store MCP Art" } },
            @{ id = "node-workflow"; type = "artNode"; data = @{ artId = "neuro.official/store-workflow-art"; label = "Store Workflow Art" } }
        )
        $instantiated = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/workflows/instantiate" -Body @{
            nodes = $nodes
            edges = @()
            mode = "reference"
            workflowId = "store-framework-smoke"
        }
        Assert-Equal "loom.hook.v1" ([string]$instantiated.protocolVersion) "Instantiate workflow protocol mismatch."
        Assert-Equal "succeeded" ([string]$instantiated.status) "Instantiate workflow response status mismatch."

        $broadcast = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "loom.hook.workflow.instantiated" ([string]$broadcast.method) "Instantiate workflow broadcast method mismatch."
        Assert-Equal 6 (@($broadcast.params.nodes).Count) "Hook broadcast node count mismatch."
        $summary.instantiateWorkflow = @{
            protocolVersion = [string]$instantiated.protocolVersion
            workflowId = [string]$broadcast.params.workflowId
            nodeCount = @($broadcast.params.nodes).Count
        }

        Send-LoomHookBridgeWebSocketJson `
            -Client $subscriber `
            -Json (@{
                method = "loom.hook.workflow.sync"
                params = @{
                    requestId = "workflow-sync:release-framework-smoke"
                    workflowId = "hook-live"
                    snapshot = @{
                        name = "Hook Live"
                        nodes = @(
                            @{
                                id = "node-mcp"
                                type = "artNode"
                                position = @{ x = 160; y = 40 }
                                measured = @{ width = 96; height = 96 }
                                data = @{
                                    artId = "neuro.official/store-mcp-art"
                                    label = "Store MCP Art"
                                    params = @{
                                        query = "smoke mcp image search"
                                        count = 2
                                    }
                                    w = 96
                                    h = 96
                                }
                            }
                        )
                        edges = @()
                    }
                }
            } | ConvertTo-Json -Depth 20 -Compress)
        $overwritten = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "succeeded" ([string]$overwritten.status) "Hook live workflow sync response status mismatch."
        $summary.liveWorkflowOverwrite = @{
            workflowId = "hook-live"
            nodeCount = 1
        }

        $imageInput = @{
            input = @{
                kind = "inline_resource"
                mime = "image/png"
                dataBase64 = $imageData.Split(",", 2)[1]
            }
        }
        $executeResults = [ordered]@{}
        $executeResults["store-cli-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-cli-art" -NodeId "node-cli" -ArtId "store-cli-art" -Inputs $imageInput -Parameters @{}
        $executeResults["store-script-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-script-art" -NodeId "node-script" -ArtId "store-script-art" -Inputs $imageInput -Parameters @{}
        $executeResults["store-cloud-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-cloud-art" -NodeId "node-cloud" -ArtId "store-cloud-art" -Inputs $imageInput -Parameters @{}
        $executeResults["store-python-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-python-art" -NodeId "node-python" -ArtId "store-python-art" -Inputs @{} -Parameters @{ text = "smoke python art" }
        $executeResults["store-mcp-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-mcp-art" -NodeId "node-mcp" -ArtId "store-mcp-art" -Inputs @{} -Parameters @{
            query = "smoke mcp image search"
            count = 2
            result_index = 1
            safesearch = "off"
            spellcheck = $true
        }
        $executeResults["store-workflow-art"] = Invoke-LoomHookArtExecution -Client $subscriber -RequestId "execute:store-workflow-art" -NodeId "node-workflow" -ArtId "store-workflow-art" -Inputs $imageInput -Parameters @{}
        $summary.executeResults = $executeResults

        foreach ($imageArtId in @("store-cli-art", "store-script-art", "store-cloud-art", "store-workflow-art")) {
            $result = $executeResults[$imageArtId]
            $output = @($result.data.outputs.PSObject.Properties | ForEach-Object { $_.Value })[0]
            Assert-True (@("shared_memory", "inline_resource") -contains [string]$output.kind) "$imageArtId formal output kind mismatch: $([string]$output.kind)"
        }
        $mcpOutput = @($executeResults["store-mcp-art"].data.outputs.PSObject.Properties | ForEach-Object { $_.Value })[0]
        Assert-True (@("shared_memory", "inline_resource") -contains [string]$mcpOutput.kind) "store-mcp-art formal output kind mismatch: $([string]$mcpOutput.kind)"
        $mcpCandidates = $executeResults["store-mcp-art"].data.candidates
        Assert-Equal "image.candidates" ([string]$mcpCandidates.kind) "store-mcp-art candidate kind mismatch."
        Assert-Equal 2 (@($mcpCandidates.items).Count) "store-mcp-art candidate count mismatch."
        Assert-Equal 1 ([int]$mcpCandidates.selectedIndex) "store-mcp-art selected candidate mismatch."
        $pythonOutput = @($executeResults["store-python-art"].data.outputs.PSObject.Properties | ForEach-Object { $_.Value })[0]
        Assert-Equal "value" ([string]$pythonOutput.kind) "store-python-art formal output kind mismatch."
        Assert-Contains "python art saw smoke python art" ($pythonOutput.value | ConvertTo-Json -Depth 20 -Compress) "store-python-art output text mismatch."

        $stopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Body @{}
        $summary.hookBridgeStopped = $stopped
        $hookBridgeRunning = $false

        $summary.formalHookExecutions = @{
            count = $executeResults.Count
            protocolVersion = "loom.hook.v1"
            mcpOutputKind = [string]$mcpOutput.kind
            mcpCandidateKind = [string]$mcpCandidates.kind
            mcpCandidateCount = @($mcpCandidates.items).Count
            mcpSelectedIndex = [int]$mcpCandidates.selectedIndex
            pythonOutputKind = [string]$pythonOutput.kind
        }
    } finally {
        if ($null -ne $subscriber) {
            Close-LoomHookBridgeWebSocket -Client $subscriber
        }
        try {
            if ($hookBridgeRunning) {
                $stopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Body @{}
                $summary.hookBridgeStopped = $stopped
                $hookBridgeRunning = $false
            }
        } catch {
            $stopError = ConvertTo-SafeSmokeErrorText -Text $_.Exception.Message -Secrets @($daemonToken)
            Write-Warning "Hook Bridge cleanup failed: $stopError"
            [void]$cleanupFailures.Add("Hook Bridge cleanup failed: $stopError")
        }
    }

    if (Test-Path -LiteralPath $cloudEvidencePath) {
        $summary.cloudEvidence = Get-Content -Raw -LiteralPath $cloudEvidencePath | ConvertFrom-Json
    }
    if (Test-Path -LiteralPath $mcpEvidencePath) {
        $summary.mcpEvidence = Get-Content -Raw -LiteralPath $mcpEvidencePath | ConvertFrom-Json
    }

    $summary.result = "passed"
} catch {
    $summary.result = "failed"
    $safeError = ConvertTo-SafeSmokeErrorText -Text $_.Exception.ToString() -Secrets @($daemonToken)
    $summary.error = $safeError
} finally {
    if (
        $null -ne $daemonProcess -and
        -not $daemonProcess.HasExited -and
        -not [string]::IsNullOrWhiteSpace($baseUrl)
    ) {
        try {
            Invoke-JsonDelete -Uri "$baseUrl/v1/mcp/servers/store-fixture" | Out-Null
        } catch {
            $cleanupError = ConvertTo-SafeSmokeErrorText -Text $_.Exception.Message -Secrets @($daemonToken)
            Write-Warning "MCP fixture cleanup failed: $cleanupError"
            [void]$cleanupFailures.Add("MCP fixture cleanup failed: $cleanupError")
        }
    }
    foreach ($process in @($daemonProcess, $artStoreProcess, $cloudProcess)) {
        foreach ($cleanupFailure in @(Stop-SpawnedProcess $process)) {
            [void]$cleanupFailures.Add([string]$cleanupFailure)
        }
    }
    $uniqueCleanupFailures = @(
        $cleanupFailures |
            ForEach-Object {
                ConvertTo-SafeSmokeErrorText -Text ([string]$_) -Secrets @($daemonToken)
            } |
            Select-Object -Unique
    )
    if ($uniqueCleanupFailures.Count -gt 0) {
        $summary.cleanupErrors = $uniqueCleanupFailures
        if ([string]$summary.result -ne "failed") {
            $summary.result = "failed"
            $summary.error = "Smoke cleanup failed: " + ($uniqueCleanupFailures -join "; ")
        }
    }
    Write-Utf8NoBomFile -Path $summaryPath -Content (ConvertTo-NormalizedJson $summary)
}

if ([string]$summary.result -eq "failed") {
    throw ([string]$summary.error)
}
Write-Output ($summary | ConvertTo-Json -Depth 20)
