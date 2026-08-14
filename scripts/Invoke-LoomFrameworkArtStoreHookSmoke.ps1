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

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$imageSearchLabel = ConvertFrom-UnicodeCodePoints @(0x56FE, 0x7247, 0x641C, 0x7D22)

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$frameworkIds = @(
    "process",
    "cloud_api",
    "mcp",
    "workflow"
)
$smokePortHelperPath = Join-Path $repoRoot "scripts\LoomSmokePorts.ps1"
. $smokePortHelperPath
$packageFullPath = $null
if (-not [string]::IsNullOrWhiteSpace($PackageDir)) {
    $packageFullPath = if ([System.IO.Path]::IsPathRooted($PackageDir)) {
        [System.IO.Path]::GetFullPath($PackageDir)
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $repoRoot $PackageDir))
    }
}
$frameworkArtifactFullPath = if ($null -ne $packageFullPath -and -not $PSBoundParameters.ContainsKey("FrameworkArtifactRoot")) {
    [System.IO.Path]::GetFullPath((Join-Path $packageFullPath "packages\frameworks"))
} elseif ([System.IO.Path]::IsPathRooted($FrameworkArtifactRoot)) {
    [System.IO.Path]::GetFullPath($FrameworkArtifactRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $FrameworkArtifactRoot))
}
$EvidenceRoot = if ([System.IO.Path]::IsPathRooted($EvidenceRoot)) {
    [System.IO.Path]::GetFullPath($EvidenceRoot)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $EvidenceRoot))
}

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    $matches = if ($Expected -is [string] -or $Actual -is [string]) {
        [string]::Equals([string]$Expected, [string]$Actual, [System.StringComparison]::Ordinal)
    } else {
        $Expected -eq $Actual
    }
    if (-not $matches) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Contains {
    param(
        [string]$Needle,
        [string]$Haystack,
        [string]$Message
    )

    if (-not $Haystack.Contains($Needle)) {
        throw "$Message Missing=[$Needle]"
    }
}

function Write-Utf8NoBomFile {
    param(
        [string]$Path,
        [string]$Content
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)

    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) {
        return $Argument
    }

    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashCount = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]0x5c) {
            $backslashCount += 1
            continue
        }

        if ($character -eq [char]0x22) {
            if ($backslashCount -gt 0) {
                [void]$builder.Append("\" * (($backslashCount * 2) + 1))
            } else {
                [void]$builder.Append("\")
            }
            [void]$builder.Append('"')
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void]$builder.Append("\" * $backslashCount)
            $backslashCount = 0
        }
        [void]$builder.Append($character)
    }

    if ($backslashCount -gt 0) {
        [void]$builder.Append("\" * ($backslashCount * 2))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )

    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ }) -join " "
    $parameters = @{
        FilePath = $FilePath
        PassThru = $true
        WindowStyle = "Hidden"
    }
    if (-not [string]::IsNullOrWhiteSpace($argumentLine)) {
        $parameters.ArgumentList = $argumentLine
    }
    if (-not [string]::IsNullOrWhiteSpace($WorkingDirectory)) {
        $parameters.WorkingDirectory = $WorkingDirectory
    }
    if (-not [string]::IsNullOrWhiteSpace($StdoutPath)) {
        $parameters.RedirectStandardOutput = $StdoutPath
    }
    if (-not [string]::IsNullOrWhiteSpace($StderrPath)) {
        $parameters.RedirectStandardError = $StderrPath
    }

    return Start-Process @parameters
}

function Stop-SpawnedProcess {
    param([System.Diagnostics.Process]$Process)

    if ($null -eq $Process) {
        return
    }

    try {
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            [void]$Process.WaitForExit(5000)
        }
    } finally {
        $Process.Dispose()
    }
}

function Start-InheritedEnvProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [hashtable]$Environment = @{},
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )

    $previous = @{}
    foreach ($entry in $Environment.GetEnumerator()) {
        $previous[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
        [Environment]::SetEnvironmentVariable($entry.Key, [string]$entry.Value, "Process")
    }

    try {
        return Start-SmokeProcess `
            -FilePath $FilePath `
            -ArgumentList $ArgumentList `
            -WorkingDirectory $WorkingDirectory `
            -StdoutPath $StdoutPath `
            -StderrPath $StderrPath
    } finally {
        foreach ($entry in $previous.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "Process")
        }
    }
}

function Invoke-JsonGet {
    param([string]$Uri)
    return Invoke-RestMethod -Uri $Uri -Method Get -TimeoutSec 15
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body
    )

    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $Uri -Method Post -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonPut {
    param(
        [string]$Uri,
        [object]$Body
    )

    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $Uri -Method Put -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonDelete {
    param([string]$Uri)
    return Invoke-RestMethod -Uri $Uri -Method Delete -TimeoutSec 20
}

function Invoke-BinaryGetBytes {
    param([string]$Uri)

    $tempPath = Join-Path $env:TEMP ("loom-binary-" + [System.Guid]::NewGuid().ToString("N") + ".bin")
    try {
        Invoke-WebRequest -Uri $Uri -Method Get -OutFile $tempPath -TimeoutSec 20 | Out-Null
        return [System.IO.File]::ReadAllBytes($tempPath)
    } finally {
        Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
    }
}

function Wait-HttpJson {
    param(
        [string]$Uri,
        [string]$Message,
        [int]$TimeoutSeconds = 20
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        try {
            return Invoke-JsonGet -Uri $Uri
        } catch {
            Start-Sleep -Milliseconds 150
        }
    } while ((Get-Date) -lt $deadline)

    throw "$Message ($Uri)"
}

function Wait-TcpPort {
    param(
        [string]$HostName,
        [int]$Port,
        [string]$Message,
        [int]$TimeoutSeconds = 10
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $client = $null
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $async = $client.BeginConnect($HostName, $Port, $null, $null)
            if ($async.AsyncWaitHandle.WaitOne(250) -and $client.Connected) {
                $client.EndConnect($async)
                return
            }
        } catch {
        } finally {
            if ($null -ne $client) {
                $client.Dispose()
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "$Message (${HostName}:$Port)"
}

function New-LoomHookBridgeWebSocket {
    param([int]$Port)

    $client = [System.Net.WebSockets.ClientWebSocket]::new()
    $uri = [Uri]::new("ws://127.0.0.1:$Port")
    $connectCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$client.ConnectAsync($uri, $connectCts.Token).GetAwaiter().GetResult()
    } finally {
        $connectCts.Dispose()
    }
    return $client
}

function Send-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$Json
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
    $sendCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$Client.SendAsync(
            [ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text,
            $true,
            $sendCts.Token
        ).GetAwaiter().GetResult()
    } finally {
        $sendCts.Dispose()
    }
}

function Receive-LoomHookBridgeWebSocketJson {
    param([System.Net.WebSockets.ClientWebSocket]$Client)

    $buffer = New-Object byte[] 4096
    $builder = [System.Text.StringBuilder]::new()
    do {
        $receiveCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
        try {
            $result = $Client.ReceiveAsync(
                [ArraySegment[byte]]::new($buffer),
                $receiveCts.Token
            ).GetAwaiter().GetResult()
        } finally {
            $receiveCts.Dispose()
        }

        if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
            throw "Hook Bridge WebSocket closed before sending a JSON response."
        }
        [void]$builder.Append([System.Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count))
    } while (-not $result.EndOfMessage)

    return $builder.ToString() | ConvertFrom-Json
}

function Receive-LoomHookResponse {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$RequestId
    )

    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $message = Receive-LoomHookBridgeWebSocketJson -Client $Client
        if ([string]$message.protocolVersion -ne "loom.hook.v1") {
            continue
        }
        $requestIdProperty = $message.PSObject.Properties["requestId"]
        $statusProperty = $message.PSObject.Properties["status"]
        if (
            $null -ne $requestIdProperty -and
            $null -ne $statusProperty -and
            [string]$requestIdProperty.Value -eq $RequestId -and
            -not [string]::IsNullOrWhiteSpace([string]$statusProperty.Value)
        ) {
            return $message
        }
    }
    throw "Loom Hook did not return a terminal response for requestId $RequestId."
}

function Invoke-LoomHookArtExecution {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$RequestId,
        [string]$NodeId,
        [string]$ArtId,
        [hashtable]$Inputs,
        [hashtable]$Parameters
    )

    Send-LoomHookBridgeWebSocketJson -Client $Client -Json (@{
        method = "loom.hook.art.execute"
        params = @{
            protocolVersion = "loom.hook.v1"
            requestId = $RequestId
            nodeId = $NodeId
            artId = $ArtId
            generation = 1
            deviceId = "device:release-smoke"
            outputTransports = @("shared_memory", "websocket")
            inputs = $Inputs
            parameters = $Parameters
            disabledParameters = @()
        }
    } | ConvertTo-Json -Depth 30 -Compress)
    $response = Receive-LoomHookResponse -Client $Client -RequestId $RequestId
    if ([string]$response.status -ne "succeeded") {
        throw "Loom Hook Art execution failed for ${ArtId}: $($response | ConvertTo-Json -Depth 30 -Compress)"
    }
    return $response
}

function Close-LoomHookBridgeWebSocket {
    param([System.Net.WebSockets.ClientWebSocket]$Client)

    if ($null -eq $Client) {
        return
    }

    try {
        if ($Client.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
            $closeCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
            try {
                try {
                    [void]$Client.CloseAsync(
                        [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
                        "done",
                        $closeCts.Token
                    ).GetAwaiter().GetResult()
                } catch {
                }
            } finally {
                $closeCts.Dispose()
            }
        }
    } finally {
        $Client.Dispose()
    }
}

function New-ZipFixture {
    param(
        [string]$ZipPath,
        [hashtable]$TextFiles = @{},
        [hashtable]$FileCopies = @{},
        [hashtable]$DirectoryCopies = @{}
    )

    $stage = Join-Path $env:TEMP ("loom-zip-stage-" + [System.Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    try {
        foreach ($entry in $TextFiles.GetEnumerator()) {
            $target = Join-Path $stage $entry.Key
            Write-Utf8NoBomFile -Path $target -Content ([string]$entry.Value)
        }
        foreach ($entry in $FileCopies.GetEnumerator()) {
            $target = Join-Path $stage $entry.Key
            $parent = Split-Path -Parent $target
            if (-not [string]::IsNullOrWhiteSpace($parent)) {
                New-Item -ItemType Directory -Force -Path $parent | Out-Null
            }
            Copy-Item -LiteralPath ([string]$entry.Value) -Destination $target -Force
        }
        foreach ($entry in $DirectoryCopies.GetEnumerator()) {
            $target = Join-Path $stage $entry.Key
            $parent = Split-Path -Parent $target
            if (-not [string]::IsNullOrWhiteSpace($parent)) {
                New-Item -ItemType Directory -Force -Path $parent | Out-Null
            }
            Copy-Item -LiteralPath ([string]$entry.Value) -Destination $target -Recurse -Force
        }
        if (Test-Path -LiteralPath $ZipPath) {
            Remove-Item -LiteralPath $ZipPath -Force
        }
        $zipParent = Split-Path -Parent $ZipPath
        if (-not [string]::IsNullOrWhiteSpace($zipParent)) {
            New-Item -ItemType Directory -Force -Path $zipParent | Out-Null
        }
        Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $ZipPath -CompressionLevel Optimal
        $zipHash = (Get-FileHash -LiteralPath $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $zipName = Split-Path -Leaf $ZipPath
        Write-Utf8NoBomFile -Path "$ZipPath.sha256" -Content ("$zipHash  $zipName" + [Environment]::NewLine)
    } finally {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
}

function ConvertTo-NormalizedJson {
    param([object]$Value)
    return (($Value | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
}

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

Assert-True (Test-Path -LiteralPath $embeddedPython -PathType Leaf) "Missing packaged Python runtime: $embeddedPython"
Assert-True (Test-Path -LiteralPath (Join-Path $pythonResourcesRoot "Launcher.py") -PathType Leaf) "Missing packaged Python launcher resources."
Assert-True (-not [string]::IsNullOrWhiteSpace($fixturePythonCommand)) "No host Python interpreter was found for the temporary cloud/MCP fixture servers."

if ($null -ne $packageFullPath) {
    $daemonExe = Join-Path $packageFullPath "runtime\loom-daemon.exe"
    $daemonWorkingDirectory = $packageFullPath
    Assert-True (Test-Path -LiteralPath $daemonExe -PathType Leaf) "Missing packaged Loom daemon for framework/store smoke: $daemonExe"
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

Assert-True (Test-Path -LiteralPath $daemonExe -PathType Leaf) "Missing daemon binary: $daemonExe"
Assert-True (Test-Path -LiteralPath $artStoreExe -PathType Leaf) "Missing art store binary: $artStoreExe"

$runId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-framework-store-$PID-$([System.Guid]::NewGuid().ToString('N'))"
$runRoot = Join-Path $EvidenceRoot $runId
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
    Assert-True (Test-Path -LiteralPath $sourceZip -PathType Leaf) "Missing framework package artifact: $sourceZip. Run Build-LoomArtFrameworkPackages.ps1 first."
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

$cloudScript = @"
import base64
import http.server
import json
import sys

PORT = int(sys.argv[1])
EVIDENCE_PATH = sys.argv[2]
IMAGE_DATA = sys.argv[3]
ALT_IMAGE_DATA = sys.argv[4]
IMAGE_BYTES = base64.b64decode(IMAGE_DATA.split(",", 1)[1])
ALT_IMAGE_BYTES = base64.b64decode(ALT_IMAGE_DATA.split(",", 1)[1])


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/raw-image.png":
            body = IMAGE_BYTES
        elif self.path == "/raw-image-alt.png":
            body = ALT_IMAGE_BYTES
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        payload = {
            "path": self.path,
            "contentType": self.headers.get("Content-Type", ""),
            "bodyLength": length,
            "bodyPreview": body[:256].decode("utf-8", "replace"),
        }
        with open(EVIDENCE_PATH, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
        response = {
            "content": [
                {
                    "type": "image",
                    "data": IMAGE_DATA,
                    "mimeType": "image/png",
                }
            ]
        }
        encoded = json.dumps(response, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, format, *args):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"@
Write-Utf8NoBomFile -Path $cloudScriptPath -Content $cloudScript

$mcpScript = @"
import json
import sys

EVIDENCE_PATH = sys.argv[1]
IMAGE_URL = sys.argv[2]
ALT_IMAGE_URL = sys.argv[3]


def write_message(message):
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()


for raw_line in sys.stdin:
    raw_line = raw_line.strip()
    if not raw_line:
        continue
    request = json.loads(raw_line)
    method = request.get("method", "")
    if method == "initialize":
        write_message({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "store-fixture", "version": "1.0.0"},
            },
        })
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        write_message({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo fixture text",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "text": {"type": "string"}
                            }
                        },
                    },
                    {
                        "name": "brave_image_search",
                        "description": "Return structured image-search results",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {"type": "string"},
                                "count": {"type": "integer"},
                                "safesearch": {"type": "string"},
                                "spellcheck": {"type": "boolean"},
                            },
                            "required": ["query"],
                        },
                    }
                ]
            },
        })
    elif method == "tools/call":
        arguments = request.get("params", {}).get("arguments", {})
        payload = {
            "toolName": request.get("params", {}).get("name"),
            "arguments": arguments,
        }
        with open(EVIDENCE_PATH, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
        tool_name = request.get("params", {}).get("name", "")
        if tool_name == "brave_image_search":
            query = str(arguments.get("query", ""))
            count = max(1, int(arguments.get("count", 1)))
            items = [
                {
                    "title": "Fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": IMAGE_URL,
                        "width": 1,
                        "height": 1,
                    },
                }
            ]
            if count >= 2:
                items.append({
                    "title": "Fixture image alt",
                    "url": "https://example.invalid/page-alt",
                    "properties": {
                        "url": ALT_IMAGE_URL,
                        "width": 1,
                        "height": 1,
                    },
                })
            write_message({
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": f"fixture brave_image_search results for {query}",
                        }
                    ],
                    "structuredContent": {
                        "type": "object",
                        "items": items,
                    },
                },
            })
        else:
            text = str(arguments.get("text", ""))
            write_message({
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": text,
                        }
                    ]
                },
            })
"@
Write-Utf8NoBomFile -Path $mcpScriptPath -Content $mcpScript

$fixturePythonLiteral = $fixturePythonCommand.Replace("'", "''")
$fixturePythonPrefix = if ($fixturePythonArgsPrefix.Count -gt 0) {
    "@(" + (($fixturePythonArgsPrefix | ForEach-Object { "'" + ([string]$_).Replace("'", "''") + "'" }) -join ", ") + ")"
} else {
    "@()"
}
$mcpLauncher = @"
param(
    [Parameter(ValueFromRemainingArguments = `$true)]
    [string[]]`$McpArguments
)
`$pythonPrefix = $fixturePythonPrefix
& '$fixturePythonLiteral' @pythonPrefix (Join-Path `$PSScriptRoot 'fake-mcp-server.py') @McpArguments
exit `$LASTEXITCODE
"@
$mcpServerManifest = @{
    schemaVersion = 1
    id = "store-fixture"
    name = "Store Fixture MCP"
    description = "Independent MCP package used by the framework/store smoke"
    version = "1.0.0"
    publisher = @{ id = "neuro.official"; name = "Neuro" }
    transport = "stdio"
    entry = @{
        command = "runtime/server.ps1"
        args = @($mcpEvidencePath, "http://127.0.0.1:$cloudPort/raw-image.png", "http://127.0.0.1:$cloudPort/raw-image-alt.png")
    }
    tools = @("echo", "brave_image_search")
    credentials = @()
}
New-ZipFixture -ZipPath $mcpPackagePath -TextFiles @{
    "mcp.server.json" = (ConvertTo-NormalizedJson $mcpServerManifest)
    "runtime/server.ps1" = $mcpLauncher
    "runtime/fake-mcp-server.py" = $mcpScript
}


$processImageRuntime = @'
$ErrorActionPreference = "Stop"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$inputValue = $request.inputs.input
if ($null -eq $inputValue) { $inputValue = $request.inputs.input_base64 }
if ($inputValue -isnot [string]) {
    $inputValue = [string]$inputValue.data
}
$response = [ordered]@{
    status = "success"
    output = [ordered]@{
        output_base64 = $inputValue
        content = @(
            [ordered]@{
                type = "image"
                data = $inputValue
                mimeType = "image/png"
            }
        )
    }
}
[Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
'@
$processRuntimeManifest = @{
    protocolVersion = "loom.art.runtime.v1"
    entry = @{
        command = "powershell.exe"
        args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "runtime/main.ps1")
    }
}
$cliManifest = @{
    id = "store-cli-art"
    name = "Store CLI Art"
    description = "Fake store command-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-cli-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-cli-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $cliManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $processRuntimeManifest)
    "runtime/main.ps1" = $processImageRuntime
}

$scriptManifest = @{
    id = "store-script-art"
    name = "Store Script Art"
    description = "Fake store script-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-script-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-script-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $scriptManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $processRuntimeManifest)
    "runtime/main.ps1" = $processImageRuntime
}

$cloudManifest = @{
    id = "store-cloud-art"
    name = "Store Cloud Art"
    description = "Fake store cloud_api Art"
    enabled = $true
    execution = @{
        type = "cloud_api"
        endpoint = "http://127.0.0.1:$cloudPort/image"
        method = "POST"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-cloud-art" }
        dependencies = @{ framework = "neuro.official/cloud_api" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-cloud-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $cloudManifest)
}

$pythonMain = @'
#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
arguments = {}
arguments.update(request.get("inputs") or {})
arguments.update(request.get("params") or {})
text = str(arguments.get("text", ""))
print(json.dumps({
    "status": "success",
    "output": {
        "content": [{"type": "text", "text": f"python art saw {text}"}],
        "pythonExecutable": sys.executable,
    },
}, separators=(",", ":")))
'@
$pythonRuntimeManifest = @{
    protocolVersion = "loom.art.runtime.v1"
    entry = @{
        command = "python.exe"
        args = @("runtime/main.py")
    }
}
$pythonManifest = @{
    id = "store-python-art"
    name = "Store Python Art"
    description = "Fake store Python-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    params = @(
        @{
            id = "text"
            label = "Text"
            widget = "text"
            default = ""
        }
    )
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-python-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-python-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $pythonManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $pythonRuntimeManifest)
    "runtime/main.py" = $pythonMain
}

$mcpManifest = @{
    id = "store-mcp-art"
    name = $imageSearchLabel
    description = "Fake store MCP image-search Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "mcp"
    }
    outputs = @(
        @{
            name = "output"
            label = "output"
            type = "image"
            execution_type = "image_buffer"
        }
    )
    params = @(
        @{ id = "query"; default = "smoke mcp image search" },
        @{ id = "count"; default = 2 },
        @{ id = "result_index"; default = 0 }
    )
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-mcp-art" }
        dependencies = @{
            framework = "mcp"
            frameworkVersion = "^0.2"
            mcpServers = @(
                @{ id = "neuro.official/store-fixture"; version = "^1.0" }
            )
        }
        mcp = @{
            serverId = "store-fixture"
            packageId = "neuro.official/store-fixture"
            version = "^1.0"
            toolName = "brave_image_search"
        }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-mcp-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $mcpManifest)
} -FileCopies @{
    "art.runtime.json" = (Join-Path $repoRoot "art-packages\samples\image-search\art.runtime.json")
    "runtime/main.ps1" = (Join-Path $repoRoot "art-packages\samples\image-search\runtime\main.ps1")
    "runtime/common.ps1" = (Join-Path $repoRoot "art-packages\shared\image-runtime-common.ps1")
}

$workflowYaml = @"
name: Store Script Workflow
nodes:
  - id: image
    uses: neuro.official/store-script-art
"@
$workflowManifest = @{
    id = "store-workflow-art"
    name = "Store Workflow Art"
    description = "Fake store workflow Art"
    enabled = $true
    execution = @{
        type = "workflow"
        workflowId = "store-script-workflow"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-workflow-art" }
        dependencies = @{ framework = "neuro.official/workflow" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-workflow-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $workflowManifest)
    "workflow.yaml" = $workflowYaml
}

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
            LOOM_CONTROL_PLANE_ROOT = $controlPlaneRoot
            LOOM_ART_STORE_URL = "http://127.0.0.1:$storePort"
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
        }
    }

    if (Test-Path -LiteralPath $cloudEvidencePath) {
        $summary.cloudEvidence = Get-Content -Raw -LiteralPath $cloudEvidencePath | ConvertFrom-Json
    }
    if (Test-Path -LiteralPath $mcpEvidencePath) {
        $summary.mcpEvidence = Get-Content -Raw -LiteralPath $mcpEvidencePath | ConvertFrom-Json
    }

    $summary.result = "passed"
    Write-Utf8NoBomFile -Path $summaryPath -Content (ConvertTo-NormalizedJson $summary)
    Write-Output ($summary | ConvertTo-Json -Depth 20)
} catch {
    $summary.result = "failed"
    $summary.error = $_.Exception.ToString()
    Write-Utf8NoBomFile -Path $summaryPath -Content (ConvertTo-NormalizedJson $summary)
    throw
} finally {
    if ($null -ne $daemonProcess -and -not $daemonProcess.HasExited) {
        try {
            Invoke-JsonDelete -Uri "$($summary.baseUrl)/v1/mcp/servers/store-fixture" | Out-Null
        } catch {
        }
    }
    Stop-SpawnedProcess $daemonProcess
    Stop-SpawnedProcess $artStoreProcess
    Stop-SpawnedProcess $cloudProcess
}
