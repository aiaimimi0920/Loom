param(
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$FrameworkArtifactRoot = ".loom-art-store-data\frameworks"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ([string]$Expected -ne [string]$Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
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

function Invoke-LoomJson {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [AllowNull()][object]$Body
    )

    $json = if ($null -eq $Body) { $null } else { $Body | ConvertTo-Json -Depth 50 -Compress }
    if ($null -eq $json) {
        return Invoke-RestMethod -Method $Method -Uri $Url -TimeoutSec 30
    }
    return Invoke-RestMethod -Method $Method -Uri $Url -ContentType "application/json" -Body $json -TimeoutSec 120
}

function Install-FrameworkZip {
    param(
        [Parameter(Mandatory = $true)][string]$BaseUrl,
        [Parameter(Mandatory = $true)][string]$ZipPath
    )

    $bytes = [System.IO.File]::ReadAllBytes($ZipPath)
    $encoded = "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
    return Invoke-LoomJson -Method Post -Url "$BaseUrl/v1/frameworks/install" -Body @{ zipBase64 = $encoded }
}

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function New-LocalArtTool {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryName,
        [Parameter(Mandatory = $true)][string]$ArtName,
        [Parameter(Mandatory = $true)][object]$Execution,
        [Parameter(Mandatory = $true)][string]$Framework,
        [object[]]$Inputs = @(),
        [object[]]$Outputs = @()
    )

    return [ordered]@{
        id = $RepositoryName
        name = $ArtName
        description = "Authored Art creation execution fixture"
        enabled = $true
        execution = $Execution
        inputs = @($Inputs)
        outputs = @($Outputs)
        params = @()
        metadata = [ordered]@{
            packageSecurity = @{ version = "0.1.0" }
            dependencies = @{ framework = $Framework }
            authoring = @{ origin = "local"; owner = "local-user" }
        }
    }
}

function Invoke-ArtExecution {
    param(
        [Parameter(Mandatory = $true)][string]$BaseUrl,
        [Parameter(Mandatory = $true)][string]$RepositoryName,
        [Parameter(Mandatory = $true)][object]$Arguments
    )

    $executed = Invoke-LoomJson -Method Post -Url "$BaseUrl/v1/tools/$RepositoryName/execute" -Body @{ arguments = $Arguments }
    Assert-Equal "succeeded" ([string]$executed.status) "Authored Art execution failed: $RepositoryName"
    return $executed
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$daemonPath = if ([System.IO.Path]::IsPathRooted($DaemonExecutable)) {
    [System.IO.Path]::GetFullPath($DaemonExecutable)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $DaemonExecutable))
}
$frameworkRootPath = if ([System.IO.Path]::IsPathRooted($FrameworkArtifactRoot)) {
    [System.IO.Path]::GetFullPath($FrameworkArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $FrameworkArtifactRoot))
}
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"

$controlPlane = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-authored-art-create-" + [guid]::NewGuid().ToString("N"))
$configuration = Join-Path $controlPlane "configuration"
$runStore = Join-Path $controlPlane "runs.sqlite3"
$stdoutPath = Join-Path $controlPlane "daemon.stdout.log"
$stderrPath = Join-Path $controlPlane "daemon.stderr.log"
$mcpFixturePath = Join-Path $controlPlane "authored-mcp-fixture.ps1"
$port = Get-FreePort
$baseUrl = "http://127.0.0.1:$port"
$daemon = $null
$succeeded = $false
$oldEnvironment = @{}
foreach ($name in @("LOOM_DAEMON_HOST", "LOOM_DAEMON_PORT", "LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_RUN_STORE_PATH")) {
    $oldEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name)
}

$frameworkIds = @("process", "cloud_api", "mcp", "workflow")
$repositoryNames = [ordered]@{
    cloud = "authored-cloud-repository"
    mcp = "authored-mcp-repository"
    script = "authored-script-repository"
    workflow = "authored-workflow-repository"
}

$mcpFixture = @'
$ErrorActionPreference = "Stop"
function Write-Message {
    param([Parameter(Mandatory = $true)][object]$Value)
    [Console]::Out.WriteLine(($Value | ConvertTo-Json -Depth 30 -Compress))
    [Console]::Out.Flush()
}
while ($null -ne ($line = [Console]::In.ReadLine())) {
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    $request = $line | ConvertFrom-Json
    $method = [string]$request.method
    if ($method -eq "notifications/initialized") { continue }
    if ($method -eq "initialize") {
        Write-Message @{
            jsonrpc = "2.0"
            id = $request.id
            result = @{
                protocolVersion = "2024-11-05"
                capabilities = @{ tools = @{} }
                serverInfo = @{ name = "authored-art-fixture"; version = "1.0.0" }
            }
        }
        continue
    }
    if ($method -eq "tools/list") {
        Write-Message @{
            jsonrpc = "2.0"
            id = $request.id
            result = @{
                tools = @(@{
                    name = "echo"
                    description = "Echo authored Art arguments"
                    inputSchema = @{
                        type = "object"
                        properties = @{ text = @{ type = "string" } }
                        required = @("text")
                    }
                })
            }
        }
        continue
    }
    if ($method -eq "tools/call") {
        $text = [string]$request.params.arguments.text
        Write-Message @{
            jsonrpc = "2.0"
            id = $request.id
            result = @{ content = @(@{ type = "text"; text = "echo:$text" }) }
        }
    }
}
'@

$pythonAdapter = @'
import importlib.util
import json
import pathlib
import sys

request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
source_path = pathlib.Path(__file__).resolve().parent / "source.py"
spec = importlib.util.spec_from_file_location("loom_authored_art", source_path)
if spec is None or spec.loader is None:
    raise RuntimeError("cannot load authored Art source")
module = importlib.util.module_from_spec(spec)
sys.path.insert(0, str(source_path.parent))
spec.loader.exec_module(module)
entry = next((getattr(module, name) for name in ("main", "entry_point", "run") if hasattr(module, name)), None)
if entry is None:
    raise RuntimeError("Authored Python Art must define main(args), entry_point(args), or run(args)")
arguments = {}
arguments.update(request.get("inputs") or {})
arguments.update(request.get("params") or {})
arguments["context"] = request.get("context") or {}
result = entry(arguments)
print(json.dumps({"status": "success", "output": result}, ensure_ascii=False, separators=(",", ":")))
'@

$pythonSource = @'
def run(args):
    return {
        "kind": "script",
        "message": args.get("message", ""),
        "has_context": isinstance(args.get("context"), dict),
    }
'@

New-Item -ItemType Directory -Force -Path $controlPlane, $configuration | Out-Null
Write-Utf8NoBomFile -Path $mcpFixturePath -Content $mcpFixture

try {
    $env:LOOM_DAEMON_HOST = "127.0.0.1"
    $env:LOOM_DAEMON_PORT = [string]$port
    $env:LOOM_CONTROL_PLANE_ROOT = $controlPlane
    $env:LOOM_CONFIGURATION_ROOT = $configuration
    $env:LOOM_RUN_STORE_PATH = $runStore
    $daemon = Start-Process -FilePath $daemonPath -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath

    $ready = $false
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        Start-Sleep -Milliseconds 250
        try {
            $health = Invoke-WebRequest -Uri "$baseUrl/health" -UseBasicParsing -TimeoutSec 2
            if ([int]$health.StatusCode -eq 200) {
                $ready = $true
                break
            }
        }
        catch {
            if ($daemon.HasExited) {
                throw "Loom daemon exited before authored Art smoke readiness."
            }
        }
    }
    Assert-True $ready "Loom daemon did not become ready for authored Art smoke."

    foreach ($frameworkId in $frameworkIds) {
        $zipPath = Join-Path $frameworkRootPath "$frameworkId.zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Framework ZIP missing: $zipPath"
        $null = Install-FrameworkZip -BaseUrl $baseUrl -ZipPath $zipPath
    }
    $frameworkStatus = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/frameworks" -Body $null
    foreach ($frameworkId in $frameworkIds) {
        $status = @($frameworkStatus.frameworks | Where-Object { [string]$_.id -eq $frameworkId }) | Select-Object -First 1
        Assert-True ($null -ne $status -and [bool]$status.installed -and [bool]$status.enabled -and [bool]$status.ready) "Framework is not ready: $frameworkId"
    }
    $trust = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/plugin-trust/policy" -Body @{
        policy = "require_signed"
    }
    Assert-Equal "require_signed" ([string]$trust.policy) "Authored Art smoke did not enable strict trust policy."

    $cloudTool = New-LocalArtTool `
        -RepositoryName $repositoryNames.cloud `
        -ArtName "Cloud Creation Test" `
        -Framework "cloud_api" `
        -Execution @{
            type = "cloud_api"
            endpoint = "$baseUrl/health"
            method = "GET"
            contentType = "application/json"
            headers = "{}"
            body = "{}"
        }
    $null = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/arts/create" -Body @{ tool = $cloudTool; files = @() }

    $null = Invoke-LoomJson -Method Put -Url "$baseUrl/v1/mcp/servers/authored-art-fixture" -Body @{
        id = "authored-art-fixture"
        name = "Authored Art Fixture"
        description = "MCP fixture for authored Art creation"
        command = "powershell.exe"
        args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $mcpFixturePath)
        env = @{}
        enabled = $true
    }
    $mcpTool = New-LocalArtTool `
        -RepositoryName $repositoryNames.mcp `
        -ArtName "MCP Creation Test" `
        -Framework "mcp" `
        -Execution @{ type = "mcp"; serverId = "authored-art-fixture"; toolName = "echo" } `
        -Inputs @(@{ name = "text"; label = "Text"; type = "string"; executionType = "string" }) `
        -Outputs @(@{ name = "result"; label = "Result"; type = "string"; executionType = "string" })
    $null = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/arts/create" -Body @{ tool = $mcpTool; files = @() }

    $scriptTool = New-LocalArtTool `
        -RepositoryName $repositoryNames.script `
        -ArtName "Script Creation Test" `
        -Framework "process" `
        -Execution @{ type = "framework_art"; framework = "process" } `
        -Inputs @(@{ name = "message"; label = "Message"; type = "string"; executionType = "string" }) `
        -Outputs @(@{ name = "result"; label = "Result"; type = "string"; executionType = "string" })
    $null = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/arts/create" -Body @{
        tool = $scriptTool
        runtime = @{
            protocolVersion = "loom.art.runtime.v1"
            entry = @{ command = "python.exe"; args = @("runtime/adapter.py") }
        }
        files = @(
            @{ path = "runtime/adapter.py"; content = $pythonAdapter }
            @{ path = "runtime/source.py"; content = $pythonSource }
        )
    }

    $workflowYaml = @"
name: Authored Workflow Creation Test
nodes:
  - id: script
    uses: $($repositoryNames.script)
    with:
      message: workflow-created
"@
    $null = Invoke-LoomJson -Method Put -Url "$baseUrl/v1/workflows/authored-creation-workflow" -Body @{ data = $workflowYaml }
    $workflowTool = New-LocalArtTool `
        -RepositoryName $repositoryNames.workflow `
        -ArtName "Workflow Creation Test" `
        -Framework "workflow" `
        -Execution @{ type = "workflow"; workflowId = "authored-creation-workflow" } `
        -Outputs @(@{ name = "result"; label = "Result"; type = "string"; executionType = "string" })
    $null = Invoke-LoomJson -Method Post -Url "$baseUrl/v1/arts/create" -Body @{ tool = $workflowTool; files = @() }

    $tools = Invoke-LoomJson -Method Get -Url "$baseUrl/v1/tools" -Body $null
    $expectedNames = [ordered]@{
        $repositoryNames.cloud = "Cloud Creation Test"
        $repositoryNames.mcp = "MCP Creation Test"
        $repositoryNames.script = "Script Creation Test"
        $repositoryNames.workflow = "Workflow Creation Test"
    }
    foreach ($repositoryName in $repositoryNames.Values) {
        $tool = @($tools.tools | Where-Object { [string]$_.id -eq $repositoryName }) | Select-Object -First 1
        Assert-True ($null -ne $tool) "Authored Art is not registered: $repositoryName"
        Assert-Equal $expectedNames[$repositoryName] ([string]$tool.name) "Repository name and Art display name were not preserved independently."
        $toolJson = $tool | ConvertTo-Json -Depth 40 -Compress
        Assert-True (-not $toolJson.Contains('"globalId"')) "Locally authored Art must not contain a platform global ID: $repositoryName"
    }

    $cloudExecuted = Invoke-ArtExecution -BaseUrl $baseUrl -RepositoryName $repositoryNames.cloud -Arguments @{}
    $cloudPayload = [string]$cloudExecuted.result.content[0].text | ConvertFrom-Json
    Assert-Equal "ok" ([string]$cloudPayload.status) "Cloud authored Art did not call the configured endpoint."
    Write-Host "PASS created/executed cloud Art $($repositoryNames.cloud)"

    $mcpExecuted = Invoke-ArtExecution -BaseUrl $baseUrl -RepositoryName $repositoryNames.mcp -Arguments @{ text = "mcp-created" }
    Assert-Equal "echo:mcp-created" ([string]$mcpExecuted.result.content[0].text) "MCP authored Art did not call the selected MCP tool."
    Write-Host "PASS created/executed MCP Art $($repositoryNames.mcp)"

    $scriptExecuted = Invoke-ArtExecution -BaseUrl $baseUrl -RepositoryName $repositoryNames.script -Arguments @{ message = "script-created" }
    Assert-Equal "script-created" ([string]$scriptExecuted.result.message) "Script authored Art did not execute the packaged Python source."
    Assert-True ([bool]$scriptExecuted.result.has_context) "Script authored Art did not receive the Loom execution context."
    Write-Host "PASS created/executed script Art $($repositoryNames.script)"

    $workflowExecuted = Invoke-ArtExecution -BaseUrl $baseUrl -RepositoryName $repositoryNames.workflow -Arguments @{}
    Assert-Equal "workflow-created" ([string]$workflowExecuted.result.message) "Workflow authored Art did not execute its saved workflow."
    Write-Host "PASS created/executed workflow Art $($repositoryNames.workflow)"

    $succeeded = $true
}
finally {
    if ($null -ne $daemon -and -not $daemon.HasExited) {
        Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
        $null = $daemon.WaitForExit(5000)
    }
    if (-not $succeeded) {
        Write-Host "--- daemon stdout ---"
        Get-Content -LiteralPath $stdoutPath -ErrorAction SilentlyContinue
        Write-Host "--- daemon stderr ---"
        Get-Content -LiteralPath $stderrPath -ErrorAction SilentlyContinue
    }
    foreach ($name in $oldEnvironment.Keys) {
        [System.Environment]::SetEnvironmentVariable($name, $oldEnvironment[$name])
    }
    Remove-Item -LiteralPath $controlPlane -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Loom authored Art create/execution smoke passed for 4 creation modes."
