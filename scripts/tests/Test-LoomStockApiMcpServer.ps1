[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Quote-ProcessArgument {
    param([Parameter(Mandatory = $true)][string]$Value)
    return '"' + $Value.Replace('"', '\"') + '"'
}

function Start-RedirectedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [hashtable]$Environment = @{}
    )

    $processInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $processInfo.FileName = $Executable
    $processInfo.Arguments = @($Arguments | ForEach-Object { Quote-ProcessArgument -Value $_ }) -join " "
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
    Assert-True $process.Start() "Failed to start process: $Executable"
    return $process
}

function Read-JsonRpcResponse {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][int]$ExpectedId
    )

    $readTask = $Process.StandardOutput.ReadLineAsync()
    if (-not $readTask.Wait(20000)) { throw "Timed out waiting for JSON-RPC response id $ExpectedId" }
    $line = $readTask.Result
    if ([string]::IsNullOrWhiteSpace($line)) {
        throw "stock-api MCP server exited before response id ${ExpectedId}: $($Process.StandardError.ReadToEnd())"
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
$runtimeRoot = Join-Path $repoRoot "mcp-server-packages\stock-api\runtime"
$entryPath = Join-Path $runtimeRoot "stock-api-entry.js"
$nodePath = [string](Get-Command node.exe -ErrorAction Stop).Source
$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
$workRoot = Join-Path $tempBase ("loom-stock-api-mcp-test-" + [Guid]::NewGuid().ToString("N"))
$fixtureScript = Join-Path $workRoot "fixture-http.ps1"
$readyPath = Join-Path $workRoot "fixture-ready"
$requestPath = Join-Path $workRoot "requests.txt"
$fixtureProcess = $null
$mcpProcess = $null
$unsafeProcess = $null

Assert-True (Test-Path -LiteralPath $entryPath -PathType Leaf) "stock-api MCP entry is missing: $entryPath"
Assert-True (Test-Path -LiteralPath (Join-Path $runtimeRoot "vendor\stock-api\LICENSE") -PathType Leaf) "Vendored stock-api license is missing."
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
$captured = @()
try {
    [System.IO.File]::WriteAllText($ReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
    for ($index = 0; $index -lt 2; $index++) {
        $client = $listener.AcceptTcpClient()
        try {
            $stream = $client.GetStream()
            $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::ASCII, $false, 4096, $true)
            try {
                $requestLine = $reader.ReadLine()
                while ($null -ne ($line = $reader.ReadLine())) {
                    if ($line.Length -eq 0) { break }
                }
            }
            finally {
                $reader.Dispose()
            }
            $requestTarget = ($requestLine -split ' ')[1]
            $proxyUri = [Uri]("http://127.0.0.1:$Port$requestTarget")
            $encodedTarget = ($proxyUri.Query.TrimStart('?') -split '&' | Where-Object { $_ -like 'url=*' } | Select-Object -First 1).Substring(4)
            $target = [Uri]::UnescapeDataString($encodedTarget)
            $captured += $target
            if ($target -match '/api/qt/stock/get\?' -and $target -match 'secid=0(?:%2E|\.)000034') {
                $body = @{ rc = 0; data = @{ f57 = "000034"; f58 = "Digital China"; f43 = 24.99; f44 = 25.20; f45 = 24.60; f60 = 24.89; f170 = 0.4 } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/api/qt/stock/kline/get\?' -and $target -match 'secid=0(?:%2E|\.)000034') {
                $body = @{ rc = 0; data = @{ klines = @(
                    "2026-08-12,24.50,24.60,24.80,24.30,100000",
                    "2026-08-13,24.62,24.75,24.90,24.55,120000",
                    "2026-08-14,24.80,24.99,25.20,24.60,150000"
                ) } } | ConvertTo-Json -Depth 8 -Compress
            }
            else {
                throw "Unexpected stock-api target: $target"
            }
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
    [System.IO.File]::WriteAllLines($RequestPath, $captured, [System.Text.UTF8Encoding]::new($false))
}
finally {
    $listener.Stop()
}
'@
[System.IO.File]::WriteAllText($fixtureScript, $fixtureSource, [System.Text.UTF8Encoding]::new($false))

try {
    $fixtureProcess = Start-RedirectedProcess `
        -Executable "powershell.exe" `
        -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureScript, "-Port", [string]$port, "-ReadyPath", $readyPath, "-RequestPath", $requestPath)
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $readyPath -PathType Leaf)) {
        if ($fixtureProcess.HasExited) { throw "Fixture HTTP server exited early: $($fixtureProcess.StandardError.ReadToEnd())" }
        if ([DateTime]::UtcNow -ge $readyDeadline) { throw "Timed out waiting for stock-api fixture HTTP server" }
        Start-Sleep -Milliseconds 50
    }

    $mcpProcess = Start-RedirectedProcess `
        -Executable $nodePath `
        -Arguments @($entryPath) `
        -Environment @{ LOOM_STOCK_API_TEST_BASE_URL = "http://127.0.0.1:$port/" }
    $initialize = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 1 -Request ([ordered]@{
        jsonrpc = "2.0"
        id = 1
        method = "initialize"
        params = [ordered]@{ protocolVersion = "2025-06-18"; capabilities = [ordered]@{}; clientInfo = [ordered]@{ name = "loom-test"; version = "1" } }
    })
    Assert-True ([string]$initialize.result.protocolVersion -eq "2025-06-18") "stock-api MCP initialize protocol mismatch."
    Assert-True ([string]$initialize.result.serverInfo.name -eq "stock-api") "stock-api MCP identity mismatch."
    Assert-True ([string]$initialize.result.serverInfo.version -eq "2.7.3") "stock-api MCP version mismatch."
    Write-Utf8JsonLine -Process $mcpProcess -Line '{"jsonrpc":"2.0","method":"notifications/initialized"}'

    $tools = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 2 -Request ([ordered]@{ jsonrpc = "2.0"; id = 2; method = "tools/list"; params = [ordered]@{} })
    $toolNames = @($tools.result.tools | ForEach-Object { [string]$_.name } | Sort-Object)
    Assert-True ($toolNames.Count -eq 5) "stock-api MCP must expose exactly five tools."
    foreach ($requiredTool in @("get_stock", "get_stocks", "get_klines", "search_stocks", "inspect_stock")) {
        Assert-True ($toolNames -contains $requiredTool) "stock-api MCP tool is missing: $requiredTool"
    }

    $quote = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 3 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 3; method = "tools/call"
        params = [ordered]@{ name = "get_stock"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney" } }
    })
    $quoteErrorProperty = $quote.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $quoteErrorProperty -or -not [bool]$quoteErrorProperty.Value) "stock-api get_stock returned isError."
    Assert-True ([string]$quote.result.structuredContent.response.stock.code -eq "SZ000034") "stock-api quote code mismatch."
    Assert-True ([string]$quote.result.structuredContent.response.stock.name -eq "Digital China") "stock-api quote name mismatch."
    Assert-True ([double]$quote.result.structuredContent.response.stock.now -eq 24.99) "stock-api quote price mismatch."
    Assert-True ([double]$quote.result.structuredContent.response.stock.percent -eq 0.004) "stock-api quote percent normalization mismatch."

    $history = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 4 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 4; method = "tools/call"
        params = [ordered]@{ name = "get_klines"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = "day"; count = 3; adjust = "none" } }
    })
    $historyErrorProperty = $history.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $historyErrorProperty -or -not [bool]$historyErrorProperty.Value) "stock-api get_klines returned isError."
    Assert-True ([int]$history.result.structuredContent.response.count -eq 3) "stock-api K-line count mismatch."
    Assert-True ([double]$history.result.structuredContent.response.klines[2].close -eq 24.99) "stock-api K-line close mismatch."
    Assert-True ([string]$history.result.structuredContent.response.klines[2].source -eq "eastmoney") "stock-api K-line source mismatch."

    Assert-True $fixtureProcess.WaitForExit(10000) "stock-api fixture did not receive both provider requests."
    Assert-True ($fixtureProcess.ExitCode -eq 0) "stock-api fixture failed: $($fixtureProcess.StandardError.ReadToEnd())"
    $capturedRequests = @(Get-Content -Encoding UTF8 -LiteralPath $requestPath)
    Assert-True ($capturedRequests.Count -eq 2) "stock-api fixture request count mismatch."
    Assert-True ($capturedRequests[0] -match 'push2delay\.eastmoney\.com/.*/stock/get') "stock-api quote did not use the declared upstream library."
    Assert-True ($capturedRequests[1] -match 'push2his\.eastmoney\.com/.*/kline/get') "stock-api K-line did not use the declared upstream library."

    $unsafeProcess = Start-RedirectedProcess -Executable $nodePath -Arguments @($entryPath) -Environment @{ LOOM_STOCK_API_TEST_BASE_URL = "https://example.com/" }
    Assert-True $unsafeProcess.WaitForExit(5000) "stock-api unsafe fixture override did not exit."
    Assert-True ($unsafeProcess.ExitCode -ne 0) "stock-api unsafe fixture override must fail closed."
    Assert-True ($unsafeProcess.StandardError.ReadToEnd() -match "loopback") "stock-api unsafe override error is missing."
}
finally {
    foreach ($process in @($mcpProcess, $fixtureProcess, $unsafeProcess)) {
        if ($null -eq $process) { continue }
        try { $process.StandardInput.Close() } catch {}
        if (-not $process.HasExited -and -not $process.WaitForExit(5000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
        $process.Dispose()
    }
    $resolvedWorkRoot = [System.IO.Path]::GetFullPath($workRoot)
    if ($resolvedWorkRoot.StartsWith($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedWorkRoot).StartsWith("loom-stock-api-mcp-test-")) {
        Remove-Item -LiteralPath $resolvedWorkRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Independent stock-api MCP server contract passed: version=2.7.3 tools=5 quote=24.99 candles=3"
