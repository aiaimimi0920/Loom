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
Assert-True (Test-Path -LiteralPath (Join-Path $runtimeRoot "vendor\pysnowball\LICENSE") -PathType Leaf) "pysnowball license is missing."
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
$dayHistoryRequests = 0
try {
    [System.IO.File]::WriteAllText($ReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
    for ($index = 0; $index -lt 26; $index++) {
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
            $statusCode = 200
            if ($target -match '/api/qt/stock/get\?' -and $target -match 'secid=0(?:%2E|\.)000034') {
                $body = @{ rc = 0; data = @{ f57 = "000034"; f58 = "Digital China"; f43 = 24.99; f44 = 25.20; f45 = 24.60; f60 = 24.89; f170 = 0.4 } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/api/qt/stock/get\?' -and $target -match 'secid=0(?:%2E|\.)430047') {
                $body = @{ rc = 0; data = @{ f57 = "430047"; f58 = "BJ Fixture"; f43 = 12.34; f44 = 12.50; f45 = 12.10; f60 = 12.20; f170 = 1.15 } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/api/qt/stock/kline/get\?' -and $target -match 'secid=0(?:%2E|\.)000034' -and $target -match 'klt=101' -and $target -match 'lmt=3') {
                $dayHistoryRequests++
                if ($dayHistoryRequests -eq 6) {
                    $body = @{ rc = 0; data = @{ klines = @(
                        "2026-08-12,24.50,24.60,24.80,24.30,100000",
                        "2026-08-13,24.62,24.75,24.90,24.55,120000",
                        "2026-08-14,24.80,24.99,25.20,24.60,150000"
                    ) } } | ConvertTo-Json -Depth 8 -Compress
                }
                else {
                    $statusCode = 503
                    $body = @{ error = "transient fixture failure"; attempt = $dayHistoryRequests } | ConvertTo-Json -Compress
                }
            }
            elseif ($target -match '/api/qt/stock/kline/get\?' -and $target -match 'secid=0(?:%2E|\.)000034' -and $target -match 'klt=5' -and $target -match 'lmt=2000') {
                $body = @{ rc = 0; data = @{ klines = @(
                    "2026-08-10 14:55,24.30,24.40,24.45,24.20,45000",
                    "2026-08-11 14:55,24.40,24.50,24.55,24.35,46000",
                    "2026-08-12 14:55,24.50,24.60,24.80,24.30,100000",
                    "2026-08-13 14:55,24.62,24.75,24.90,24.55,120000",
                    "2026-08-14 14:55,24.80,24.99,25.20,24.60,150000"
                ) } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/api/qt/stock/kline/get\?' -and $target -match 'secid=0(?:%2E|\.)000034' -and $target -match 'klt=5') {
                $body = @{ rc = 0; data = @{ klines = @(
                    "2026-08-14 14:50,24.80,24.90,25.00,24.75,50000",
                    "2026-08-14 14:55,24.90,24.99,25.05,24.88,65000"
                ) } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/api/qt/stock/kline/get\?' -and $target -match 'secid=0(?:%2E|\.)000034') {
                $body = @{ rc = 0; data = @{ klines = @(
                    "2026-08-12,24.50,24.60,24.80,24.30,100000",
                    "2026-08-13,24.62,24.75,24.90,24.55,120000",
                    "2026-08-14,24.80,24.99,25.20,24.60,150000"
                ) } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/v5/stock/realtime/pankou\.json' -and $target -match 'symbol=SZ000034') {
                $body = @{ error_code = 0; error_description = ""; data = @{
                    current = 25.53; bp1 = 25.53; bc1 = 152340; bn1 = 88; bp2 = 25.52; bc2 = 61200; bn2 = 41
                    sp1 = 25.54; sc1 = 98700; sn1 = 55; sp2 = 25.55; sc2 = 44100; sn2 = 30
                    buypct = 49.24; sellpct = 50.76; diff = -11455; ratio = 1.08; timestamp = 1786000485000
                } } | ConvertTo-Json -Depth 8 -Compress
            }
            elseif ($target -match '/v5/stock/realtime/quotec\.json' -and $target -match 'symbol=SZ000034') {
                $body = @{ error_code = 0; error_description = ""; data = @(@{
                    symbol = "SZ000034"; current = 25.53; last_close = 25.20; open = 25.25; high = 25.60; low = 24.44
                    chg = 0.33; percent = 1.31; avg_price = 25.199; volume = 18220000; amount = 459000000
                    turnover_rate = 7.31; amplitude = 4.6; market_capital = 39800000000; is_trade = $false
                    trade_session = 0; timestamp = 1786000485000
                }) } | ConvertTo-Json -Depth 8 -Compress
            }
            else {
                throw "Unexpected stock-api target: $target"
            }
            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
            $statusText = if ($statusCode -eq 200) { "OK" } else { "Service Unavailable" }
            $header = "HTTP/1.1 $statusCode $statusText`r`nContent-Type: application/json; charset=utf-8`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
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
        -Environment @{ LOOM_STOCK_API_TEST_BASE_URL = "http://127.0.0.1:$port/"; LOOM_PYSNOWBALL_TOKEN = "xq_a_token=fixture-only" }
    $initialize = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 1 -Request ([ordered]@{
        jsonrpc = "2.0"
        id = 1
        method = "initialize"
        params = [ordered]@{ protocolVersion = "2025-06-18"; capabilities = [ordered]@{}; clientInfo = [ordered]@{ name = "loom-test"; version = "1" } }
    })
    Assert-True ([string]$initialize.result.protocolVersion -eq "2025-06-18") "stock-api MCP initialize protocol mismatch."
    Assert-True ([string]$initialize.result.serverInfo.name -eq "stock-api") "stock-api MCP identity mismatch."
    Assert-True ([string]$initialize.result.serverInfo.version -eq "2.9.0") "stock-api MCP version mismatch."
    Write-Utf8JsonLine -Process $mcpProcess -Line '{"jsonrpc":"2.0","method":"notifications/initialized"}'

    $tools = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 2 -Request ([ordered]@{ jsonrpc = "2.0"; id = 2; method = "tools/list"; params = [ordered]@{} })
    $toolNames = @($tools.result.tools | ForEach-Object { [string]$_.name } | Sort-Object)
    Assert-True ($toolNames.Count -eq 7) "stock-api MCP must expose exactly seven tools."
    foreach ($requiredTool in @("get_stock", "get_stocks", "get_klines", "get_market_series", "get_order_book", "search_stocks", "inspect_stock")) {
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
    Assert-True ([string]$quote.result.structuredContent.provider.wrapperVersion -eq "2.9.0") "stock-api quote provider metadata mismatch."
    Assert-True (-not [bool]$quote.result.structuredContent.response.cached -and [int]$quote.result.structuredContent.response.cacheAgeMillis -eq 0) "stock-api fresh quote cache metadata mismatch."

    $history = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 4 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 4; method = "tools/call"
        params = [ordered]@{ name = "get_klines"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = "day"; count = 3; adjust = "none" } }
    })
    $historyErrorProperty = $history.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $historyErrorProperty -or -not [bool]$historyErrorProperty.Value) "stock-api get_klines returned isError."
    Assert-True ([int]$history.result.structuredContent.response.count -eq 3) "stock-api K-line count mismatch."
    Assert-True ([double]$history.result.structuredContent.response.klines[2].close -eq 24.99) "stock-api K-line close mismatch."
    Assert-True ([string]$history.result.structuredContent.response.klines[2].source -eq "eastmoney") "stock-api K-line source mismatch."

    $series = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 5 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 5; method = "tools/call"
        params = [ordered]@{ name = "get_market_series"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = "minute-5"; count = 2; adjust = "none" } }
    })
    $seriesErrorProperty = $series.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $seriesErrorProperty -or -not [bool]$seriesErrorProperty.Value) "stock-api get_market_series returned isError."
    Assert-True ([string]$series.result.structuredContent.response.period -eq "minute-5") "stock-api market-series period mismatch."
    Assert-True ([int]$series.result.structuredContent.response.count -eq 2) "stock-api market-series count mismatch."
    Assert-True ([double]$series.result.structuredContent.response.klines[1].close -eq 24.99) "stock-api market-series close mismatch."

    $fiveDay = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 6 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 6; method = "tools/call"
        params = [ordered]@{ name = "get_market_series"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = "five-day"; count = 2000; adjust = "none" } }
    })
    $fiveDayErrorProperty = $fiveDay.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $fiveDayErrorProperty -or -not [bool]$fiveDayErrorProperty.Value) "stock-api five-day market-series returned isError."
    Assert-True ([int]$fiveDay.result.structuredContent.response.count -eq 5) "stock-api five-day series must retain the latest five trading dates."
    Assert-True ([string]$fiveDay.result.structuredContent.response.period -eq "five-day") "stock-api five-day period mismatch."

    $cachedHistory = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 7 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 7; method = "tools/call"
        params = [ordered]@{ name = "get_klines"; arguments = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = "day"; count = 3; adjust = "none" } }
    })
    $cachedHistoryErrorProperty = $cachedHistory.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $cachedHistoryErrorProperty -or -not [bool]$cachedHistoryErrorProperty.Value) "stock-api cached history fallback returned isError."
    Assert-True ([bool]$cachedHistory.result.structuredContent.response.cached) "stock-api did not mark the last-success history fallback."
    Assert-True ([int]$cachedHistory.result.structuredContent.response.count -eq 3) "stock-api cached history fallback count mismatch."
    Assert-True ([int]$cachedHistory.result.structuredContent.response.cacheAgeMillis -ge 0) "stock-api cached history age metadata is missing."
    Assert-True ([int]$cachedHistory.result.structuredContent.response.cacheTtlMillis -eq 900000) "stock-api cached history TTL mismatch."

    $beijingQuote = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 8 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 8; method = "tools/call"
        params = [ordered]@{ name = "get_stock"; arguments = [ordered]@{ code = "BJ430047"; source = "eastmoney" } }
    })
    Assert-True ([string]$beijingQuote.result.structuredContent.response.stock.code -eq "BJ430047") "stock-api Beijing Exchange quote code mismatch."
    Assert-True ([double]$beijingQuote.result.structuredContent.response.stock.now -eq 12.34) "stock-api Beijing Exchange secid mapping failed."

    $orderBook = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 9 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 9; method = "tools/call"
        params = [ordered]@{ name = "get_order_book"; arguments = [ordered]@{ code = "SZ000034"; source = "xueqiu" } }
    })
    $orderBookErrorProperty = $orderBook.result.PSObject.Properties["isError"]
    Assert-True ($null -eq $orderBookErrorProperty -or -not [bool]$orderBookErrorProperty.Value) "stock-api get_order_book returned isError."
    $book = $orderBook.result.structuredContent.response.orderBook
    Assert-True ([int]$book.levels -eq 2) "stock-api order book level count mismatch."
    Assert-True ([double]$book.bids[0].price -eq 25.53 -and [double]$book.bids[0].volume -eq 152340) "stock-api order book best bid mismatch."
    Assert-True ([double]$book.asks[0].price -eq 25.54) "stock-api order book best ask mismatch."
    Assert-True ([double]$book.buyPercent -eq 49.24 -and [double]$book.netVolume -eq -11455) "stock-api order book imbalance mismatch."
    Assert-True ([string]$book.source -eq "xueqiu") "stock-api order book source mismatch."
    $tape = $orderBook.result.structuredContent.response.realtime
    Assert-True ([double]$tape.now -eq 25.53) "stock-api realtime tape price mismatch."
    Assert-True ([double]$tape.percent -eq 0.0131) "stock-api realtime tape percent normalization mismatch."
    Assert-True ([double]$tape.avgPrice -eq 25.199 -and [double]$tape.turnoverRate -eq 7.31) "stock-api realtime tape intraday fields mismatch."

    $pysnowball = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 10 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 10; method = "tools/call"
        params = [ordered]@{ name = "get_order_book"; arguments = [ordered]@{ code = "SZ000034"; source = "pysnowball" } }
    })
    Assert-True ([string]$pysnowball.result.structuredContent.response.orderBook.source -eq "pysnowball") "pysnowball depth source mismatch."
    Assert-True ([string]$pysnowball.result.structuredContent.response.realtime.source -eq "pysnowball") "pysnowball quotec source mismatch."
    Assert-True ([string]$pysnowball.result.structuredContent.provider.pysnowballVersion -eq "0.1.8") "pysnowball provider version metadata mismatch."
    Assert-True ([bool]$pysnowball.result.structuredContent.provider.pysnowballTokenConfigured) "pysnowball fixture token was not detected."

    $automatic = Send-JsonRpcRequest -Process $mcpProcess -ExpectedId 11 -Request ([ordered]@{
        jsonrpc = "2.0"; id = 11; method = "tools/call"
        params = [ordered]@{ name = "get_order_book"; arguments = [ordered]@{ code = "SZ000034"; source = "auto" } }
    })
    Assert-True ([string]$automatic.result.structuredContent.provider.requestedSource -eq "auto") "automatic live-source metadata mismatch."
    Assert-True ([string]$automatic.result.structuredContent.provider.liveSources.realtime -eq "pysnowball") "automatic live source did not prefer pysnowball quotec."
    Assert-True ([int]$automatic.result.structuredContent.response.cacheTtlMillis -eq 45000) "automatic live-source TTL mismatch."

    Assert-True $fixtureProcess.WaitForExit(10000) "stock-api fixture did not receive all provider requests."
    Assert-True ($fixtureProcess.ExitCode -eq 0) "stock-api fixture failed: $($fixtureProcess.StandardError.ReadToEnd())"
    $capturedRequests = @(Get-Content -Encoding UTF8 -LiteralPath $requestPath)
    Assert-True ($capturedRequests.Count -eq 26) "stock-api fixture request count mismatch."
    Assert-True ($capturedRequests[0] -match 'push2delay\.eastmoney\.com/.*/stock/get') "stock-api quote did not use the declared upstream library."
    Assert-True ($capturedRequests[1] -match '7\.push2his\.eastmoney\.com/.*/kline/get') "stock-api K-line did not prioritize the responsive upstream host."
    Assert-True ($capturedRequests[1] -match 'end=20500101' -and $capturedRequests[1] -match 'lmt=3') "stock-api K-line request is not bounded to the requested row count."
    Assert-True ($capturedRequests[1] -notmatch '(?:[?&])beg=') "stock-api K-line request must not ask Eastmoney for the full history."
    Assert-True ($capturedRequests[5] -match '91\.push2his\.eastmoney\.com/.*/kline/get') "stock-api did not exhaust the bounded first host round."
    Assert-True ($capturedRequests[6] -match '7\.push2his\.eastmoney\.com/.*/kline/get' -and $capturedRequests[6] -match 'klt=101') "stock-api did not retry the provider host set after a transient failure."
    Assert-True ($capturedRequests[7] -match '7\.push2his\.eastmoney\.com/.*/kline/get' -and $capturedRequests[7] -match 'klt=5') "stock-api market-series did not use the declared Eastmoney interval."
    Assert-True ($capturedRequests[8] -match '7\.push2his\.eastmoney\.com/.*/kline/get' -and $capturedRequests[8] -match 'klt=5' -and $capturedRequests[8] -match 'lmt=2000') "stock-api five-day series did not use the multi-day Eastmoney interval."
    Assert-True ($capturedRequests[18] -match '91\.push2his\.eastmoney\.com/.*/kline/get' -and $capturedRequests[18] -match 'klt=101') "stock-api cached fallback did not exhaust the bounded retry attempts first."
    Assert-True ($capturedRequests[19] -match 'secid=0(?:%2E|\.)430047') "stock-api did not map BJ430047 to the bounded Eastmoney secid."
    Assert-True (($capturedRequests[20..21] -join "`n") -match 'stock\.xueqiu\.com/v5/stock/realtime/pankou\.json') "stock-api legacy order book did not query the Xueqiu ten-level endpoint."
    Assert-True (($capturedRequests[20..21] -join "`n") -match 'stock\.xueqiu\.com/v5/stock/realtime/quotec\.json') "stock-api legacy order book did not query the Xueqiu realtime endpoint."
    Assert-True (($capturedRequests[22..25] -join "`n") -match 'stock\.xueqiu\.com/v5/stock/realtime/pankou\.json') "pysnowball-compatible depth endpoint was not called."
    Assert-True (($capturedRequests[22..25] -join "`n") -match 'stock\.xueqiu\.com/v5/stock/realtime/quotec\.json') "pysnowball-compatible quotec endpoint was not called."
    $entrySource = Get-Content -Raw -Encoding UTF8 -LiteralPath $entryPath
    Assert-True ($entrySource.Contains("MAX_RESPONSE_BYTES") -and $entrySource.Contains("response.body.getReader")) "stock-api provider responses must be byte-bounded before JSON parsing."

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

Write-Host "Independent stock-api MCP server contract passed: version=2.9.0 tools=7 quote=24.99 BJ=verified candles=3 bounded-history=verified series=2 five-day=5 retry=verified ttl-cache=verified order-book=2-levels sources=xueqiu+pysnowball+auto"
