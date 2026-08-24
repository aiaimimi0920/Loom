<# Owns the end-to-end local Loom release candidate smoke and all daemon/control-plane assertions. #>

function Test-LoomRelease {
    $loomExe = $null
    $loomDaemonExe = $null
    $loomDesktopExe = $null
    $process = $null
    $cloudJob = $null
    $mcpRegistryJob = $null
    $tempRoot = New-SmokeTempRoot -Prefix "loom-release-smoke"
    try {
        $cliExtractRoot = Join-Path $tempRoot "cli"
        $layout = Get-LoomReleaseLayout -PackageDir $PackageDir -CliExtractRoot $cliExtractRoot
        $loomExe = $layout.cliExe
        $loomDaemonExe = $layout.daemonExe
        $loomDesktopExe = $layout.desktopExe
        Assert-PathExists $loomExe
        Assert-PathExists $loomDaemonExe
        Assert-PathExists $loomDesktopExe

        $helpRun = Invoke-ProcessCapture -FilePath $loomExe -ArgumentList @("--help") -TimeoutSeconds 30
        $helpText = [string]$helpRun.output
        if ($helpRun.exitCode -ne 0) {
            throw "loom CLI --help failed with exit code $($helpRun.exitCode) output=$(Redact-SmokeFailureText -Text $helpText)"
        }
        Assert-Contains "Usage: loom" $helpText "Loom CLI help output mismatch."

        $versionRun = Invoke-ProcessCapture -FilePath $loomExe -ArgumentList @("--version") -TimeoutSeconds 30
        $versionText = [string]$versionRun.output
        if ($versionRun.exitCode -ne 0) {
            throw "loom CLI --version failed with exit code $($versionRun.exitCode) output=$(Redact-SmokeFailureText -Text $versionText)"
        }
        Assert-Contains "loom " $versionText "Loom CLI version output mismatch."

        $daemonVersionRun = Invoke-ProcessCapture -FilePath $loomDaemonExe -ArgumentList @("--version") -TimeoutSeconds 30
        $daemonVersionText = [string]$daemonVersionRun.output
        if ($daemonVersionRun.exitCode -ne 0) {
            throw "loom-daemon.exe --version failed with exit code $($daemonVersionRun.exitCode) output=$(Redact-SmokeFailureText -Text $daemonVersionText)"
        }
        Assert-Contains "loom-daemon " $daemonVersionText "Loom daemon version output mismatch."

        $port = Get-LoomSmokePort
        $manifestDir = Join-Path $tempRoot "capabilities"
        New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
        $stdout = Join-Path $tempRoot "loom-daemon.stdout.log"
        $stderr = Join-Path $tempRoot "loom-daemon.stderr.log"
        $controlPlaneRoot = Join-Path $tempRoot "loom-control-plane"
        $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
        $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
        $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
        $oldMcpRegistryEndpoint = [Environment]::GetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "Process")
        $mcpRegistryFixtureDir = Join-Path $tempRoot "mcp-registry-fixture"
        $mcpRegistryPort = Get-LoomSmokePort
        $mcpRegistryJob = Start-LoomMcpRegistryFixtureJob -Port $mcpRegistryPort -OutputDir $mcpRegistryFixtureDir
        Wait-ForPath -Path (Join-Path $mcpRegistryFixtureDir "ready.txt") -TimeoutSeconds 20
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$port, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $controlPlaneRoot, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "http://127.0.0.1:$mcpRegistryPort/v0/servers", "Process")
        try {
            $process = Start-SmokeProcess `
                -FilePath $loomDaemonExe `
                -ArgumentList @("--manifest-dir", $manifestDir) `
                -StdoutPath $stdout `
                -StderrPath $stderr
        } finally {
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", $oldHost, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", $oldPort, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $oldControlPlaneRoot, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", $oldMcpRegistryEndpoint, "Process")
        }

        $manifest = Wait-ForFileJson -Path (Join-Path $manifestDir "loom.json")
        Assert-Equal "loom" $manifest.appId "Loom manifest appId mismatch."
        Assert-Equal "Loom" $manifest.displayName "Loom manifest displayName mismatch."
        Assert-Equal "http" $manifest.transport.type "Loom manifest transport type mismatch."
        Assert-Equal "bearer" $manifest.transport.auth "Loom manifest auth mismatch."
        $manifestToken = [string]$manifest.transport.authToken
        if ([string]::IsNullOrWhiteSpace($manifestToken)) {
            throw "Loom manifest did not contain the generated administrator token."
        }
        $persistedManifestToken = [System.IO.File]::ReadAllText((Join-Path $controlPlaneRoot "daemon-token"), [System.Text.Encoding]::UTF8).Trim()
        if ($manifestToken -ne $persistedManifestToken) {
            throw "Loom persisted daemon token mismatch."
        }
        $script:DaemonAuthHeaders = @{ Authorization = "Bearer $manifestToken" }
        $manifestCapabilityIds = @($manifest.capabilities) -join ","
        Assert-Equal $ExpectedLoomCapabilityIds $manifestCapabilityIds "Loom manifest capability list mismatch."

        $baseUrl = [string]$manifest.transport.baseUrl
        Assert-Equal "http://127.0.0.1:$port" $baseUrl "Loom manifest baseUrl mismatch."
        $health = Wait-LoomDaemonHealth -BaseUrl $baseUrl -Message "Timed out waiting for Loom daemon"

        $status = Invoke-JsonGet -Uri "$baseUrl/status"
        Assert-Equal "ready" $status.status "Loom daemon status mismatch."

        $capabilities = Invoke-JsonGet -Uri "$baseUrl/v1/capabilities"
        $capabilityIds = @($capabilities.capabilities | ForEach-Object { $_.id }) -join ","
        Assert-Equal $ExpectedLoomCapabilityIds $capabilityIds "Loom capability list mismatch."

        $mcpServers = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/servers"
        Assert-Equal 0 (@($mcpServers.servers).Count) "Loom MCP server list should start empty in isolated smoke root."
        $savedMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/release-mcp" -Body @{
            id = "release-mcp"
            name = "Release MCP"
            transport = "stdio"
            command = "npx"
            args = @("-y", "@example/release-mcp")
            env = @{}
            enabled = $true
        }
        Assert-Equal "release-mcp" $savedMcpServer.server.id "Loom MCP server save id mismatch."
        $mcpServersAfterSave = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/servers"
        Assert-Equal "release-mcp" ([string]$mcpServersAfterSave.servers[0].id) "Loom MCP server list id mismatch."

        $fixtureMcpScript = New-LoomFixtureMcpServerScript -TempRoot $tempRoot

        # GET /v1/mcp/registry?search=fixture
        $mcpRegistry = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/registry?search=fixture&limit=250&cursor=cursor-1"
        Assert-Equal "io.modelcontextprotocol/fixture" ([string]$mcpRegistry.servers[0].server.name) "Loom MCP Registry server name mismatch."
        $mcpRegistryRequestPath = Join-Path $mcpRegistryFixtureDir "request.txt"
        Assert-PathExists $mcpRegistryRequestPath
        $mcpRegistryRequest = Get-Content -Raw -LiteralPath $mcpRegistryRequestPath
        Assert-Contains "GET /v0/servers?limit=100&search=fixture&cursor=cursor-1" $mcpRegistryRequest "Loom MCP Registry request URL mismatch."

        # POST /v1/mcp/test
        $mcpConnectionTest = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/test" -Body @{
            id = "fixture-test"
            name = "Fixture Test MCP"
            transport = "stdio"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal $true ([bool]$mcpConnectionTest.success) "Loom MCP connection test success mismatch."
        Assert-Equal "echo" ([string]$mcpConnectionTest.tools[0].name) "Loom MCP connection test tool name mismatch."
        Assert-Equal "release-fixture" ([string]$mcpConnectionTest.server_info.serverInfo.name) "Loom MCP connection test server info mismatch."

        # POST /v1/mcp/package/check and POST /v1/mcp/package/install-plan
        $mcpPackageCheck = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/check" -Body @{
            moduleName = "json"
        }
        Assert-Equal "json" ([string]$mcpPackageCheck.module) "Loom MCP package check module mismatch."
        $mcpPackageInstallPlan = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/install-plan" -Body @{
            packageName = "mcp-server-demo"
        }
        Assert-Equal $false ([bool]$mcpPackageInstallPlan.sideEffect) "Loom MCP package install plan must be side-effect free."
        Assert-Contains "pip" (($mcpPackageInstallPlan.command | ForEach-Object { [string]$_ }) -join " ") "Loom MCP package install plan should include pip command."


        $cloudFixtureDir = Join-Path $tempRoot "cloud-api-fixture"
        $cloudPort = Get-LoomSmokePort
        $cloudJob = Start-LoomCloudApiFixtureJob -Port $cloudPort -OutputDir $cloudFixtureDir -MaxRequests 1
        Wait-ForPath -Path (Join-Path $cloudFixtureDir "ready.txt") -TimeoutSeconds 20

        $savedFixtureMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture" -Body @{
            id = "fixture"
            name = "Fixture MCP"
            transport = "stdio"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal "fixture" $savedFixtureMcpServer.server.id "Loom fixture MCP server save id mismatch."

        $savedDeleteMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture-delete" -Body @{
            id = "fixture-delete"
            name = "Fixture Delete MCP"
            transport = "stdio"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal "fixture-delete" $savedDeleteMcpServer.server.id "Loom delete MCP server save id mismatch."
        # DELETE /v1/mcp/servers/fixture-delete
        $deletedMcpServer = Invoke-JsonDelete -Uri "$baseUrl/v1/mcp/servers/fixture-delete"
        Assert-Equal $true ([bool]$deletedMcpServer.deleted) "Loom MCP server deletion mismatch."

        $savedTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/release-workflow-tool" -Body @{
            id = "release-workflow-tool"
            name = "Release Workflow Tool"
            description = "Release smoke workflow-backed tool"
            enabled = $true
            execution = @{
                type = "workflow"
                workflowId = "release-workflow"
            }
        }
        Assert-Equal "release-workflow-tool" $savedTool.tool.id "Loom registry save id mismatch."
        Assert-Equal "workflow" $savedTool.tool.execution.type "Loom registry execution type mismatch."
        $tools = Invoke-JsonGet -Uri "$baseUrl/v1/tools"
        Assert-Equal "release-workflow-tool" ([string]$tools.tools[0].id) "Loom registry list id mismatch."

        $savedDeleteTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-delete-tool" -Body @{
            id = "fixture-delete-tool"
            name = "Fixture Delete Tool"
            description = "Release smoke deletable registry tool"
            enabled = $true
            execution = @{
                type = "workflow"
                workflowId = "fixture-delete-workflow"
            }
        }
        Assert-Equal "fixture-delete-tool" $savedDeleteTool.tool.id "Loom delete tool save id mismatch."
        # DELETE /v1/tools/fixture-delete-tool
        $deletedTool = Invoke-JsonDelete -Uri "$baseUrl/v1/tools/fixture-delete-tool"
        Assert-Equal $true ([bool]$deletedTool.deleted) "Loom registry tool deletion mismatch."

        $savedMcpTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-echo" -Body @{
            id = "fixture-echo"
            name = "Fixture Echo"
            description = "Release smoke MCP-backed tool"
            enabled = $true
            execution = @{
                type = "mcp"
                serverId = "fixture"
                toolName = "echo"
            }
        }
        Assert-Equal "fixture-echo" $savedMcpTool.tool.id "Loom MCP-backed tool save id mismatch."
        $executedMcpTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-echo/execute" -Body @{
            arguments = @{
                text = "release mcp runtime"
            }
        }
        Assert-Equal "succeeded" $executedMcpTool.status "Loom MCP-backed tool execution status mismatch."
        Assert-Equal "release mcp runtime" ([string]$executedMcpTool.result.content[0].text) "Loom MCP-backed tool execution content mismatch."
        $savedCloudTextTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-text" -Body @{
            id = "fixture-cloud-text"
            name = "Fixture Cloud Text"
            description = "Release smoke cloud API text tool"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/text"
                method = "POST"
            }
            # A cloud Art only reaches a loopback endpoint when it declares that it wants to, so the
            # fixture-backed smoke tools declare it the way a real local-service Art would.
            metadata = @{
                permissionPolicy = @{ network = @{ allowLocalhost = $true } }
            }
        }
        Assert-Equal "fixture-cloud-text" $savedCloudTextTool.tool.id "Loom cloud API text tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudTextTool.tool.execution.type "Loom cloud API text execution type mismatch."
        $executedCloudTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-cloud-text/execute" -Body @{
            arguments = @{
                prompt = "release cloud runtime"
            }
        }
        Assert-Equal "succeeded" $executedCloudTool.status "Loom cloud API tool execution status mismatch."
        Assert-Equal "cloud saw release cloud runtime" ([string]$executedCloudTool.result.content[0].text) "Loom cloud API tool execution content mismatch."

        $savedCloudArtTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-art" -Body @{
            id = "fixture-cloud-art"
            name = "Fixture Cloud Art"
            description = "Release smoke cloud API image Art"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/process"
                method = "POST"
            }
            metadata = @{
                permissionPolicy = @{ network = @{ allowLocalhost = $true } }
            }
        }
        Assert-Equal "fixture-cloud-art" $savedCloudArtTool.tool.id "Loom cloud API Art tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudArtTool.tool.execution.type "Loom cloud API Art execution type mismatch."

        $savedCloudMultipartArtTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-multipart-art" -Body @{
            id = "fixture-cloud-multipart-art"
            name = "Fixture Cloud Multipart Art"
            description = "Release smoke multipart cloud API image Art"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/multipart/{{inputs.route.value}}"
                method = "POST"
                contentType = "multipart/form-data"
                headers = '{"X-Trace":"{{inputs.trace.value}}"}'
                body = '{"file":"{{inputs.input.path}}","prompt":"{{inputs.prompt.value}}"}'
            }
            metadata = @{
                permissionPolicy = @{ network = @{ allowLocalhost = $true } }
            }
        }
        Assert-Equal "fixture-cloud-multipart-art" $savedCloudMultipartArtTool.tool.id "Loom cloud multipart Art tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudMultipartArtTool.tool.execution.type "Loom cloud multipart Art execution type mismatch."
        Assert-Equal "multipart/form-data" ([string]$savedCloudMultipartArtTool.tool.execution.contentType) "Loom cloud multipart contentType save mismatch."

        $workflowYaml = @"
name: Release Workflow Runtime
nodes:
  - id: image
    uses: core.image.invert
"@
        $savedWorkflow = Invoke-JsonPut -Uri "$baseUrl/v1/workflows/release-workflow" -Body @{
            data = $workflowYaml
        }
        Assert-Equal "release-workflow" $savedWorkflow.workflow.id "Loom workflow save id mismatch."
        Assert-Equal 1 ([int]$savedWorkflow.workflow.nodeCount) "Loom workflow node count mismatch."
        $workflows = Invoke-JsonGet -Uri "$baseUrl/v1/workflows"
        Assert-Equal "release-workflow" ([string]$workflows.workflows[0].id) "Loom workflow list id mismatch."

        # GET /v1/workflows/release-workflow
        $loadedWorkflow = Invoke-JsonGet -Uri "$baseUrl/v1/workflows/release-workflow"
        Assert-Equal "release-workflow" ([string]$loadedWorkflow.workflow.id) "Loom workflow load id mismatch."
        Assert-Contains "Release Workflow Runtime" ([string]$loadedWorkflow.workflow.data) "Loom workflow load data mismatch."

        $deleteWorkflowYaml = @"
name: Delete Workflow Runtime
nodes:
  - id: image
    uses: core.image.invert
"@
        $savedDeleteWorkflow = Invoke-JsonPut -Uri "$baseUrl/v1/workflows/fixture-delete-workflow" -Body @{
            data = $deleteWorkflowYaml
        }
        Assert-Equal "fixture-delete-workflow" ([string]$savedDeleteWorkflow.workflow.id) "Loom delete workflow save id mismatch."
        # DELETE /v1/workflows/fixture-delete-workflow
        $deletedWorkflow = Invoke-JsonDelete -Uri "$baseUrl/v1/workflows/fixture-delete-workflow"
        Assert-Equal $true ([bool]$deletedWorkflow.deleted) "Loom workflow deletion mismatch."
        $workflowInput = New-LoomNativeImageSmokePngDataUrl
        $executedWorkflowTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/release-workflow-tool/execute" -Body @{
            arguments = @{ input_base64 = $workflowInput }
        }
        Assert-Equal "succeeded" $executedWorkflowTool.status "Loom workflow-backed tool execution status mismatch."
        Assert-Equal "image" ([string]$executedWorkflowTool.result.content[0].type) "Loom workflow-backed tool content type mismatch."
        if ([string]::IsNullOrWhiteSpace([string]$executedWorkflowTool.result.content[0].data)) {
            throw "Loom workflow-backed image output missing."
        }

        $imageHelperConvert = Test-LoomImageHelperConvert -BaseUrl $baseUrl

        $hookBridge = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status"
        Assert-Equal 19820 ([int]$hookBridge.port) "Loom Hook Bridge default port mismatch."
        Assert-Equal $false ([bool]$hookBridge.running) "Loom Hook Bridge protocol-only smoke state mismatch."
        Assert-Equal "loom.hook.v1" ([string]$hookBridge.protocol) "Loom Hook Bridge protocol mismatch."
        $hookBridgeMethods = @($hookBridge.methods | ForEach-Object { [string]$_ })
        Assert-Contains "loom.hook.handshake" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing handshake."
        Assert-Contains "loom.hook.workflow.node.update" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing workflow node update."
        Assert-Contains "loom.hook.workflow.instantiate" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing workflow instantiate."
        Assert-Contains "loom.hook.art.execute" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing Art execution."
        $hookSession = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/session"
        Assert-Equal "loom.hook.v1" ([string]$hookSession.protocolVersion) "Loom Hook session protocol mismatch."

        $hookBridgeStarted = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/start" -Body @{
            port = 0
        }
        Assert-Equal $true ([bool]$hookBridgeStarted.running) "Loom Hook Bridge start state mismatch."
        if ([int]$hookBridgeStarted.port -le 0) {
            throw "Loom Hook Bridge start should allocate a port."
        }
        $hookBridgeRunning = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status"
        Assert-Equal $true ([bool]$hookBridgeRunning.running) "Loom Hook Bridge running status mismatch."
        Assert-Equal ([int]$hookBridgeStarted.port) ([int]$hookBridgeRunning.port) "Loom Hook Bridge running port mismatch."
        $hookBridgeStopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Body @{}
        Assert-Equal $false ([bool]$hookBridgeStopped.running) "Loom Hook Bridge stop state mismatch."

        $completedCloudJob = Wait-Job -Job $cloudJob -Timeout 20
        if ($null -eq $completedCloudJob) {
            throw "Timed out waiting for Loom cloud API fixture job."
        }
        try {
            Receive-Job -Job $completedCloudJob -ErrorAction Stop | Out-Null
        } catch {
            throw "Loom cloud API fixture job reported errors: $($_.Exception.Message)"
        }
        Remove-Job -Job $cloudJob -Force -ErrorAction SilentlyContinue
        $cloudJob = $null

        $invoke = Invoke-JsonPost -Uri "$baseUrl/v1/invoke" -Body @{
            requestId = "release-loom-1"
            caller = "hook"
            capability = "brain.plan"
            input = @{
                goal = "release smoke"
                constraints = @("Hook Talk Loom")
            }
        }
        Assert-Equal "succeeded" $invoke.status "Loom invoke status mismatch."

        $runId = [string]$invoke.output.runId
        $run = Invoke-JsonGet -Uri "$baseUrl/v1/runs/$runId"
        Assert-Equal "brain.plan" $run.capability "Loom run capability mismatch."

        $events = Invoke-JsonGet -Uri "$baseUrl/v1/runs/$runId/events"
        $eventKinds = @($events.events | ForEach-Object { $_.kind }) -join ","
        Assert-Equal "run_started,capability_completed" $eventKinds "Loom run event kinds mismatch."

        Stop-SpawnedProcess $process
        $process = $null

        $tokenized = Invoke-LoomTokenizedReleaseSmoke `
            -DaemonExe $loomDaemonExe `
            -TempRoot $tempRoot `
            -ExpectedCapabilityIds $ExpectedLoomCapabilityIds

        return [ordered]@{
            app = "Loom"
            exes = @($loomDesktopExe, $loomDaemonExe)
            cliExe = $loomExe
            desktopExe = $loomDesktopExe
            version = $versionText.Trim()
            daemonVersion = $daemonVersionText.Trim()
            manifestAppId = $manifest.appId
            health = $health.status
            status = $status.status
            capabilities = $capabilityIds
            controlPlane = [ordered]@{
                mcpServerId = [string]$mcpServersAfterSave.servers[0].id
                toolId = [string]$tools.tools[0].id
                workflowId = [string]$workflows.workflows[0].id
                hookBridgePort = [int]$hookBridge.port
                hookBridgeRuntimePort = [int]$hookBridgeStarted.port
                hookBridgeMethods = $hookBridgeMethods
                hookProtocol = [string]$hookBridge.protocol
                hookSessionAvailable = [bool]$hookSession.available
                mcpToolExecution = [string]$executedMcpTool.result.content[0].text
                managementCrud = [ordered]@{
                    mcpServerDeleted = [bool]$deletedMcpServer.deleted
                    toolDeleted = [bool]$deletedTool.deleted
                    workflowLoaded = [string]$loadedWorkflow.workflow.id
                    workflowDeleted = [bool]$deletedWorkflow.deleted
                }
                mcpMarketplace = [ordered]@{
                    registryServerCount = @($mcpRegistry.servers).Count
                    registryServerName = [string]$mcpRegistry.servers[0].server.name
                    connectionTestSuccess = [bool]$mcpConnectionTest.success
                    connectionTestTool = [string]$mcpConnectionTest.tools[0].name
                    connectionTestServer = [string]$mcpConnectionTest.server_info.serverInfo.name
                    packageCheckModule = [string]$mcpPackageCheck.module
                    packageInstallSideEffect = [bool]$mcpPackageInstallPlan.sideEffect
                }
                cloudToolExecution = [string]$executedCloudTool.result.content[0].text
                workflowToolExecution = [string]$executedWorkflowTool.result.content[0].type
                imageHelperConvert = $imageHelperConvert
            }
            invoke = $invoke.status
            runCapability = $run.capability
            eventKinds = $eventKinds
            tokenizedAuth = $tokenized.auth
            tokenizedCapabilities = $tokenized.capabilities
            tokenizedHookBridgePort = $tokenized.hookBridgePort
            tokenizedHookBridgeRuntimePort = $tokenized.hookBridgeRuntimePort
            tokenizedInvoke = $tokenized.invoke
        }
    } catch {
        $smokeFailure = $_
        try {
            Save-SmokeFailureEvidence -TempRoot $tempRoot -Label "loom-release" | Out-Null
        } catch {
            Write-Warning "Failed to save Loom smoke diagnostics: $($_.Exception.Message)"
        }
        throw $smokeFailure
    } finally {
        Stop-SpawnedProcess $process
        if ($null -ne $cloudJob) {
            Stop-Job -Job $cloudJob -ErrorAction SilentlyContinue
            Remove-Job -Job $cloudJob -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $mcpRegistryJob) {
            Stop-Job -Job $mcpRegistryJob -ErrorAction SilentlyContinue
            Remove-Job -Job $mcpRegistryJob -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Milliseconds 300
        Remove-SmokeTempRoot -Path $tempRoot
    }
}
