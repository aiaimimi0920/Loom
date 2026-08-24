<# Owns isolated release-smoke phases that create fixture files or short-lived daemon instances. #>

function New-LoomFixtureMcpServerScript {
    param([Parameter(Mandatory = $true)][string]$TempRoot)

    $fixtureMcpScript = Join-Path $TempRoot "fixture-mcp-server.ps1"
    $fixtureMcpSource = @'
$ErrorActionPreference = "Stop"
while ($null -ne ($line = [Console]::In.ReadLine())) {
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $request = $line | ConvertFrom-Json
    if ($request.method -eq "initialize") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                protocolVersion = "2024-11-05"
                capabilities = @{ tools = @{} }
                serverInfo = [ordered]@{
                    name = "release-fixture"
                    version = "0.1.0"
                }
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    if ($request.method -eq "notifications/initialized") {
        continue
    }
    if ($request.method -eq "tools/list") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                tools = @(
                    [ordered]@{
                        name = "echo"
                        description = "Echo arguments"
                        inputSchema = [ordered]@{
                            type = "object"
                            properties = [ordered]@{
                                text = [ordered]@{ type = "string" }
                            }
                        }
                    }
                )
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    if ($request.method -eq "tools/call") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                content = @(
                    [ordered]@{
                        type = "text"
                        text = [string]$request.params.arguments.text
                    }
                )
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    $errorResponse = [ordered]@{
        jsonrpc = "2.0"
        id = $request.id
        error = [ordered]@{
            code = -32601
            message = "unknown method $($request.method)"
        }
    }
    [Console]::Out.WriteLine(($errorResponse | ConvertTo-Json -Depth 20 -Compress))
    [Console]::Out.Flush()
}
'@
    [System.IO.File]::WriteAllText($fixtureMcpScript, $fixtureMcpSource, [System.Text.UTF8Encoding]::new($false))
    return $fixtureMcpScript
}

function Invoke-LoomTokenizedReleaseSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$DaemonExe,
        [Parameter(Mandatory = $true)][string]$TempRoot,
        [Parameter(Mandatory = $true)][string]$ExpectedCapabilityIds
    )

    $process = $null
    try {
        $port = Get-LoomSmokePort
        $tokenValue = "release-smoke-token-$PID"
        $manifestDir = Join-Path $TempRoot "tokenized-capabilities"
        New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
        $stdout = Join-Path $TempRoot "loom-daemon-token.stdout.log"
        $stderr = Join-Path $TempRoot "loom-daemon-token.stderr.log"
        $controlPlaneRoot = Join-Path $TempRoot "loom-token-control-plane"
        $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
        $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
        $oldToken = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_TOKEN", "Process")
        $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$port, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $tokenValue, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $controlPlaneRoot, "Process")
        try {
            $process = Start-SmokeProcess `
                -FilePath $DaemonExe `
                -ArgumentList @("--manifest-dir", $manifestDir) `
                -StdoutPath $stdout `
                -StderrPath $stderr
        } finally {
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", $oldHost, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", $oldPort, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $oldToken, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $oldControlPlaneRoot, "Process")
        }

        $manifest = Wait-ForFileJson -Path (Join-Path $manifestDir "loom.json")
        Assert-Equal "loom" $manifest.appId "Tokenized Loom manifest appId mismatch."
        Assert-Equal "bearer" $manifest.transport.auth "Tokenized Loom manifest auth mismatch."
        if ($tokenValue -ne [string]$manifest.transport.authToken) {
            throw "Tokenized Loom manifest authToken mismatch."
        }
        $baseUrl = [string]$manifest.transport.baseUrl
        Assert-Equal "http://127.0.0.1:$port" $baseUrl "Tokenized Loom manifest baseUrl mismatch."
        Wait-LoomDaemonHealth -BaseUrl $baseUrl -Message "Timed out waiting for tokenized Loom daemon" | Out-Null

        Assert-HttpStatus -Uri "$baseUrl/v1/capabilities" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/mcp/servers" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/mcp/registry?search=fixture" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/mcp/test" -Method "Post" -Body @{ id = "fixture-test" } -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/mcp/servers/fixture-delete" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/tools/fixture-delete-tool" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/workflows" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/workflows/fixture-delete-workflow" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/hook-bridge/status" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$baseUrl/v1/hook-bridge/start" -Method "Post" -ExpectedStatus 401 -Body @{ port = 0 }
        Assert-HttpStatus -Uri "$baseUrl/v1/invoke" -Method "Post" -ExpectedStatus 401 -Body @{
            requestId = "release-loom-token-unauthorized"
            caller = "hook"
            capability = "brain.plan"
            input = @{ goal = "tokenized release smoke unauthorized" }
        }

        $headers = @{ Authorization = "Bearer $tokenValue" }
        $capabilities = Invoke-JsonGet -Uri "$baseUrl/v1/capabilities" -Headers $headers
        $capabilityIds = @($capabilities.capabilities | ForEach-Object { $_.id }) -join ","
        Assert-Equal $ExpectedCapabilityIds $capabilityIds "Tokenized Loom capability list mismatch."
        $hookBridge = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status" -Headers $headers
        Assert-Equal 19820 ([int]$hookBridge.port) "Tokenized Loom Hook Bridge status port mismatch."
        $hookBridgeStarted = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/start" -Headers $headers -Body @{ port = 0 }
        Assert-Equal $true ([bool]$hookBridgeStarted.running) "Tokenized Loom Hook Bridge start mismatch."
        $hookBridgeStopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Headers $headers -Body @{}
        Assert-Equal $false ([bool]$hookBridgeStopped.running) "Tokenized Loom Hook Bridge stop mismatch."
        $invoke = Invoke-JsonPost -Uri "$baseUrl/v1/invoke" -Headers $headers -Body @{
            requestId = "release-loom-token-1"
            caller = "hook"
            capability = "brain.plan"
            input = @{ goal = "tokenized release smoke" }
        }
        Assert-Equal "succeeded" $invoke.status "Tokenized Loom invoke status mismatch."

        return [ordered]@{
            auth = [string]$manifest.transport.auth
            capabilities = $capabilityIds
            hookBridgePort = [int]$hookBridge.port
            hookBridgeRuntimePort = [int]$hookBridgeStarted.port
            invoke = [string]$invoke.status
        }
    } finally {
        Stop-SpawnedProcess $process
    }
}
