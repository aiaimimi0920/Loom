[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Quote-ProcessArgument {
    param([Parameter(Mandatory = $true)][string]$Value)

    return '"' + $Value.Replace('"', '\"') + '"'
}

function Start-RedirectedPowerShell {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [hashtable]$Environment = @{}
    )

    $processInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $processInfo.FileName = "powershell.exe"
    $processInfo.Arguments = @(
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        (Quote-ProcessArgument -Value $ScriptPath)
    ) + @($Arguments | ForEach-Object { Quote-ProcessArgument -Value $_ }) -join " "
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    $processInfo.RedirectStandardInput = $true
    $processInfo.RedirectStandardOutput = $true
    $processInfo.RedirectStandardError = $true
    foreach ($entry in $Environment.GetEnumerator()) {
        $processInfo.EnvironmentVariables[[string]$entry.Key] = [string]$entry.Value
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $processInfo
    Assert-True $process.Start() "Failed to start PowerShell process: $ScriptPath"
    return $process
}

function Read-JsonRpcResponse {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][int]$ExpectedId
    )

    $readTask = $Process.StandardOutput.ReadLineAsync()
    if (-not $readTask.Wait(15000)) {
        throw "Timed out waiting for JSON-RPC response id $ExpectedId"
    }
    $line = $readTask.Result
    if ([string]::IsNullOrWhiteSpace($line)) {
        $stderr = $Process.StandardError.ReadToEnd()
        throw "MCP server exited before response id ${ExpectedId}: $stderr"
    }
    $response = $line | ConvertFrom-Json
    Assert-True ([int]$response.id -eq $ExpectedId) "Unexpected JSON-RPC response id: $line"
    Assert-True ($null -eq $response.PSObject.Properties["error"]) "JSON-RPC response failed: $line"
    return $response
}

function Write-Utf8JsonLine {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$Line
    )

    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($Line + "`n")
    $Process.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
    $Process.StandardInput.BaseStream.Flush()
}

function Send-JsonRpcRequest {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][object]$Request,
        [Parameter(Mandatory = $true)][int]$ExpectedId
    )

    Write-Utf8JsonLine -Process $Process -Line ($Request | ConvertTo-Json -Depth 20 -Compress)
    return Read-JsonRpcResponse -Process $Process -ExpectedId $ExpectedId
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$serverScript = Join-Path $repoRoot "mcp-server-packages\image-search\runtime\image-search-mcp.ps1"
$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
$workRoot = Join-Path $tempBase ("loom-image-search-mcp-test-" + [Guid]::NewGuid().ToString("N"))
$fixtureScript = Join-Path $workRoot "fixture-http.ps1"
$readyPath = Join-Path $workRoot "fixture-ready"
$requestPath = Join-Path $workRoot "request.txt"
$fixtureProcess = $null
$mcpProcess = $null

Assert-True (Test-Path -LiteralPath $serverScript -PathType Leaf) "Image-search MCP server is missing: $serverScript"
New-Item -ItemType Directory -Force -Path $workRoot | Out-Null

$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$listener.Start()
$port = [int]$listener.LocalEndpoint.Port
$listener.Stop()

$fixtureSource = @'
param(
    [int]$Port,
    [string]$ReadyPath,
    [string]$RequestPath
)
$ErrorActionPreference = "Stop"
$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
$listener.Start()
try {
    [System.IO.File]::WriteAllText($ReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
    $client = $listener.AcceptTcpClient()
    try {
        $stream = $client.GetStream()
        $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::ASCII, $false, 1024, $true)
        $requestLines = @()
        try {
            while ($null -ne ($line = $reader.ReadLine())) {
                if ($line.Length -eq 0) { break }
                $requestLines += $line
            }
        }
        finally {
            $reader.Dispose()
        }
        [System.IO.File]::WriteAllLines($RequestPath, $requestLines, [System.Text.UTF8Encoding]::new($false))
        $body = @{
            results = @(
                @{
                    title = "Loom first"
                    source = "https://example.test/source/1"
                    thumbnail = @{ src = "https://cdn.example.test/thumb-1.jpg" }
                    properties = @{ url = "https://cdn.example.test/image-1.png"; width = 640; height = 480 }
                },
                @{
                    title = "Loom second"
                    url = "https://cdn.example.test/image-2.png"
                    source = "https://example.test/source/2"
                    thumbnail = @{ src = "https://cdn.example.test/thumb-2.jpg" }
                    properties = @{}
                },
                @{
                    title = "Loom thumbnail fallback"
                    source = "https://example.test/source/3"
                    thumbnail = @{ src = "https://cdn.example.test/thumb-3.jpg" }
                    properties = @{}
                }
            )
        } | ConvertTo-Json -Depth 10 -Compress
        $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
        $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json; charset=utf-8`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
        $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
        $stream.Write($headerBytes, 0, $headerBytes.Length)
        $stream.Write($bodyBytes, 0, $bodyBytes.Length)
        $stream.Flush()
    }
    finally {
        $client.Dispose()
    }
}
finally {
    $listener.Stop()
}
'@
[System.IO.File]::WriteAllText($fixtureScript, $fixtureSource, [System.Text.UTF8Encoding]::new($false))

try {
    $fixtureProcess = Start-RedirectedPowerShell `
        -ScriptPath $fixtureScript `
        -Arguments @("-Port", [string]$port, "-ReadyPath", $readyPath, "-RequestPath", $requestPath)
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $readyPath -PathType Leaf)) {
        if ($fixtureProcess.HasExited) {
            throw "Fixture HTTP server exited early: $($fixtureProcess.StandardError.ReadToEnd())"
        }
        if ([DateTime]::UtcNow -ge $readyDeadline) {
            throw "Timed out waiting for fixture HTTP server"
        }
        Start-Sleep -Milliseconds 50
    }

    $mcpProcess = Start-RedirectedPowerShell `
        -ScriptPath $serverScript `
        -Arguments @("-Endpoint", "http://127.0.0.1:$port/res/v1/images/search") `
        -Environment @{ BRAVE_API_KEY = "fixture-api-key" }

    $initialize = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 1 -Request ([ordered]@{
        jsonrpc = "2.0"
        id = 1
        method = "initialize"
        params = [ordered]@{
            protocolVersion = "2024-11-05"
            capabilities = [ordered]@{}
            clientInfo = [ordered]@{ name = "loom-test"; version = "1" }
        }
    })
    Assert-True ([string]$initialize.result.protocolVersion -eq "2024-11-05") "MCP initialize protocol version mismatch."
    Assert-True ([string]$initialize.result.serverInfo.name -eq "neuro-image-search-mcp") "MCP server identity mismatch."

    Write-Utf8JsonLine -Process $mcpProcess -Line '{"jsonrpc":"2.0","method":"notifications/initialized"}'

    $tools = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 2 -Request ([ordered]@{
        jsonrpc = "2.0"
        id = 2
        method = "tools/list"
        params = [ordered]@{}
    })
    Assert-True (@($tools.result.tools).Count -eq 1) "Image-search MCP server must expose exactly one tool."
    Assert-True ([string]$tools.result.tools[0].name -eq "brave_image_search") "Image-search MCP tool name mismatch."
    Assert-True (@($tools.result.tools[0].inputSchema.required) -contains "query") "Image-search MCP tool must require query."

    $call = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 3 -Request ([ordered]@{
        jsonrpc = "2.0"
        id = 3
        method = "tools/call"
        params = [ordered]@{
            name = "brave_image_search"
            arguments = [ordered]@{ query = "loom framework"; count = 3 }
        }
    })
    Assert-True (-not [bool]$call.result.isError) "Image-search MCP tool returned isError."
    Assert-True ([int]$call.result.structuredContent.count -eq 3) "Image-search MCP candidate count mismatch."
    Assert-True ([string]$call.result.structuredContent.candidates[0].imageUrl -eq "https://cdn.example.test/image-1.png") "Primary image URL was not normalized."
    Assert-True ([string]$call.result.structuredContent.candidates[0].thumbnailUrl -eq "https://cdn.example.test/thumb-1.jpg") "Thumbnail URL was not preserved."
    Assert-True ([string]$call.result.structuredContent.candidates[0].sourcePageUrl -eq "https://example.test/source/1") "Source page URL was not preserved."
    Assert-True ([int]$call.result.structuredContent.candidates[0].width -eq 640) "Image width was not preserved."
    Assert-True ([string]$call.result.structuredContent.candidates[1].imageUrl -eq "https://cdn.example.test/image-2.png") "Top-level Brave image URL was not preserved."
    Assert-True ([string]$call.result.structuredContent.candidates[2].imageUrl -eq "https://cdn.example.test/thumb-3.jpg") "Thumbnail-only Brave result was not preserved."
    Assert-True ([string]$call.result.structuredContent.candidates[2].sourcePageUrl -eq "https://example.test/source/3") "Thumbnail-only source page URL was not preserved."

    Assert-True $fixtureProcess.WaitForExit(10000) "Fixture HTTP server did not exit after one request."
    Assert-True ($fixtureProcess.ExitCode -eq 0) "Fixture HTTP server failed: $($fixtureProcess.StandardError.ReadToEnd())"
    $capturedRequest = Get-Content -Raw -Encoding UTF8 -LiteralPath $requestPath
    Assert-True ($capturedRequest -match 'GET /res/v1/images/search\?q=loom%20framework&count=3&safesearch=strict HTTP/1\.1') "Image-search request URI is invalid: $capturedRequest"
    Assert-True ($capturedRequest -match '(?im)^X-Subscription-Token:\s*fixture-api-key\s*$') "Brave API credential header was not sent."
}
finally {
    if ($null -ne $mcpProcess) {
        try { $mcpProcess.StandardInput.Close() } catch {}
        if (-not $mcpProcess.WaitForExit(5000)) {
            Stop-Process -Id $mcpProcess.Id -Force -ErrorAction SilentlyContinue
        }
        $mcpProcess.Dispose()
    }
    if ($null -ne $fixtureProcess) {
        if (-not $fixtureProcess.HasExited) {
            Stop-Process -Id $fixtureProcess.Id -Force -ErrorAction SilentlyContinue
        }
        $fixtureProcess.Dispose()
    }
    $resolvedWorkRoot = [System.IO.Path]::GetFullPath($workRoot)
    if ($resolvedWorkRoot.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedWorkRoot).StartsWith("loom-image-search-mcp-test-")) {
        Remove-Item -LiteralPath $resolvedWorkRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Independent image-search MCP server contract passed."
