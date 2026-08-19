param(
    [string]$ArtifactRoot = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if (-not [object]::Equals($Expected, $Actual)) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)
    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) { return $Argument }
    return '"' + $Argument.Replace('\', '\\').Replace('"', '\"') + '"'
}

function New-McpData {
    param(
        [switch]$QuoteError,
        [switch]$HistoryError,
        [switch]$Skipped,
        [switch]$HistoryOnly,
        [switch]$QuoteOnly,
        [switch]$OrderBookError,
        [string]$Period = "day"
    )

    if ($Skipped) {
        return [ordered]@{ mcp = [ordered]@{ serverId = "stock-api"; skipped = $true } }
    }
    $quoteResult = if ($QuoteError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture quote failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = "SZ000034"; source = "eastmoney" }
                response = [ordered]@{
                    stock = [ordered]@{
                        code = "SZ000034"
                        name = "Digital China"
                        percent = 0.004
                        now = 24.99
                        low = 24.60
                        high = 25.20
                        yesterday = 24.89
                        source = "eastmoney"
                    }
                }
            }
        }
    }
    $historyResult = if ($HistoryError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture history failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = "SZ000034"; source = "eastmoney"; period = $Period; count = 2000; adjust = "none" }
                response = [ordered]@{
                    count = 3
                    period = $Period
                    lastTradingDate = "2026-08-14"
                    klines = @(
                        [ordered]@{ date = "2026-08-12"; open = 24.50; close = 24.60; high = 24.80; low = 24.30; volume = 100000; source = "tencent" },
                        [ordered]@{ date = "2026-08-13"; open = 24.62; close = 24.75; high = 24.90; low = 24.55; volume = 120000; source = "tencent" },
                        [ordered]@{ date = "2026-08-14"; open = 24.80; close = 24.99; high = 25.20; low = 24.60; volume = 150000; source = "tencent" }
                    )
                }
            }
        }
    }
    $orderBookResult = if ($OrderBookError) {
        [ordered]@{
            isError = $true
            structuredContent = [ordered]@{
                response = [ordered]@{ code = "STOCK_API_TOOL_ERROR"; message = "fixture order book failure" }
            }
        }
    }
    else {
        [ordered]@{
            structuredContent = [ordered]@{
                input = [ordered]@{ code = "SZ000034"; source = "xueqiu"; symbol = "SZ000034" }
                response = [ordered]@{
                    orderBook = [ordered]@{
                        code = "SZ000034"
                        bids = @(
                            [ordered]@{ level = 1; price = 24.98; volume = 152340; orders = 88 },
                            [ordered]@{ level = 2; price = 24.97; volume = 61200; orders = 41 }
                        )
                        asks = @(
                            [ordered]@{ level = 1; price = 24.99; volume = 98700; orders = 55 },
                            [ordered]@{ level = 2; price = 25.00; volume = 44100; orders = 30 }
                        )
                        buyPercent = 49.24
                        sellPercent = 50.76
                        netVolume = -11455
                        ratio = 1.08
                        levels = 2
                        observedAt = "2026-08-14T07:00:00.000Z"
                        source = "xueqiu"
                    }
                    realtime = [ordered]@{
                        code = "SZ000034"
                        now = 24.99
                        avgPrice = 24.91
                        volume = 18220000
                        amount = 459000000
                        turnoverRate = 7.31
                        amplitude = 4.6
                        marketCapital = 39800000000
                        isTrade = $false
                        tradeSession = 0
                        observedAt = "2026-08-14T07:00:00.000Z"
                        source = "xueqiu"
                    }
                }
            }
        }
    }
    $results = if ($HistoryOnly) {
        [ordered]@{
            history = [ordered]@{ toolName = "get_market_series"; result = $historyResult }
        }
    }
    elseif ($QuoteOnly) {
        [ordered]@{
            quote = [ordered]@{ toolName = "get_stock"; result = $quoteResult }
            orderbook = [ordered]@{ toolName = "get_order_book"; result = $orderBookResult }
        }
    }
    else {
        [ordered]@{
            quote = [ordered]@{ toolName = "get_stock"; result = $quoteResult }
            history = [ordered]@{ toolName = "get_market_series"; result = $historyResult }
            orderbook = [ordered]@{ toolName = "get_order_book"; result = $orderBookResult }
        }
    }
    return [ordered]@{
        mcp = [ordered]@{
            serverId = "stock-api"
            results = $results
        }
    }
}

function Invoke-StockRuntime {
    param(
        [string]$ArtDirectory,
        [AllowEmptyString()][string]$ActionId,
        [AllowNull()][object]$Payload,
        [AllowNull()][object]$AuthoritativeState,
        [AllowNull()][object]$FrameworkData,
        [AllowNull()][object]$Params = @{}
    )

    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ArtDirectory "art.runtime.json") | ConvertFrom-Json
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = [string]$runtime.entry.command
    $psi.Arguments = @($runtime.entry.args | ForEach-Object { ConvertTo-ProcessArgument ([string]$_) }) -join " "
    $psi.WorkingDirectory = $ArtDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $psi
    Assert-True $process.Start() "Failed to start Stock Monitor runtime."
    $request = [ordered]@{
        protocolVersion = "loom.framework.v1"
        frameworkId = "mcp"
        artId = "custom-stock-monitor"
        inputs = @{}
        params = $Params
        frameworkData = $FrameworkData
    }
    if (-not [string]::IsNullOrWhiteSpace($ActionId)) {
        $request.surfaceAction = [ordered]@{
            actionId = $ActionId
            payload = if ($null -eq $Payload) { @{} } else { $Payload }
            authoritativeState = if ($null -eq $AuthoritativeState) { @{} } else { $AuthoritativeState }
        }
    }
    $process.StandardInput.WriteLine(($request | ConvertTo-Json -Depth 40 -Compress))
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    Assert-True $process.WaitForExit(20000) "Stock Monitor runtime timed out."
    Assert-Equal 0 $process.ExitCode "Stock Monitor runtime exited with an error: $stderr"
    Assert-True (-not [string]::IsNullOrWhiteSpace($stdout)) "Stock Monitor runtime returned no stdout: $stderr"
    return $stdout.Trim() | ConvertFrom-Json
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("loom-stock-monitor-test-" + [Guid]::NewGuid().ToString("N"))
$artDirectory = Join-Path $repoRoot "art-packages\samples\stock-monitor"

New-Item -ItemType Directory -Force -Path $workRoot | Out-Null
try {
    if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
        $artifactRootPath = if ([IO.Path]::IsPathRooted($ArtifactRoot)) {
            [IO.Path]::GetFullPath($ArtifactRoot)
        }
        else {
            [IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
        }
        $zipPath = Join-Path $artifactRootPath "custom-stock-monitor.zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Packaged Stock Monitor ZIP is missing: $zipPath"
        $artDirectory = Join-Path $workRoot "packaged-art"
        Expand-Archive -LiteralPath $zipPath -DestinationPath $artDirectory -Force
    }

    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $artDirectory "manifest.json") | ConvertFrom-Json
    Assert-Equal "1.4.0" ([string]$manifest.metadata.packageSecurity.version) "Stock Monitor package version must force the Xueqiu order-book migration."
    Assert-Equal "neuro.official/custom-stock-monitor" ([string]$manifest.metadata.art.qualifiedId) "Stock Monitor qualified Art identity mismatch."
    Assert-Equal "mcp" ([string]$manifest.execution.framework) "Stock Monitor must execute through the MCP framework."
    Assert-Equal "=2.8.0" ([string]$manifest.metadata.mcp.version) "Stock Monitor stock-api wrapper version must be exact."
    Assert-Equal 3 @($manifest.metadata.mcp.calls).Count "Stock Monitor must declare quote, history, and order-book MCP calls."
    Assert-True (@($manifest.metadata.mcp.calls | Where-Object { [string]$_.arguments.source -eq "eastmoney" }).Count -eq 2) "Stock Monitor quote and history calls must use the bounded eastmoney provider path."
    $orderBookCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "orderbook" })[0]
    Assert-Equal "get_order_book" ([string]$orderBookCall.toolName) "Stock Monitor order-book call must target the Xueqiu ten-level tool."
    Assert-Equal "xueqiu" ([string]$orderBookCall.arguments.source) "Stock Monitor order-book call must request the Xueqiu source."
    Assert-True (@($manifest.metadata.marketData.sources) -contains "xueqiu") "Stock Monitor must declare Xueqiu among its market-data sources."
    Assert-Equal 50000 ([int]$manifest.metadata.capabilities.surface.actions[0].timeoutMs) "Stock Monitor refresh action timeout must cover bounded provider fallback."
    Assert-Equal 50000 ([int]$manifest.metadata.capabilities.surface.actions[1].timeoutMs) "Stock Monitor symbol action timeout must cover bounded provider fallback."
    $periodAction = @($manifest.metadata.capabilities.surface.actions | Where-Object { [string]$_.id -eq "stock_period_commit" })[0]
    Assert-Equal 50000 ([int]$periodAction.timeoutMs) "Stock Monitor period action timeout must cover bounded provider fallback."
    Assert-Equal 760 ([int]$manifest.metadata.capabilities.surface.minimumSize.width) "Stock Monitor Surface minimum width must keep all controls visible."
    Assert-Equal 640 ([int]$manifest.metadata.capabilities.surface.minimumSize.height) "Stock Monitor Surface minimum height must keep the chart visible."
    Assert-Equal 0 @($manifest.metadata.mcp.surfaceActions.stock_interval_commit.calls).Count "Interval updates must skip remote MCP calls."
    Assert-Equal "history" ([string]$manifest.metadata.mcp.surfaceActions.stock_period_commit.calls[0]) "Period switches must reuse the current quote and request only history."
    $tickAction = @($manifest.metadata.capabilities.surface.actions | Where-Object { [string]$_.id -eq "stock_tick_refresh" })[0]
    Assert-True ($null -ne $tickAction) "Stock Monitor must declare a near-realtime tick action."
    Assert-Equal 12000 ([int]$tickAction.timeoutMs) "Tick action timeout must stay far below the full refresh budget."
    Assert-True (-not [bool]$tickAction.progress) "Tick action must not raise progress noise at second-level cadence."
    Assert-Equal 2 @($manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls).Count "Tick refresh must request the quote and the order book only."
    Assert-Equal "quote" ([string]$manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls[0]) "Tick refresh must fetch the quote first and reuse cached history."
    Assert-Equal "orderbook" ([string]$manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls[1]) "Tick refresh must fetch the live order book alongside the quote."
    $intervalParam = @($manifest.params | Where-Object { [string]$_.id -eq "interval_seconds" })[0]
    Assert-Equal 1 ([int]$intervalParam.min) "Refresh interval must reach one second for near-realtime monitoring."
    Assert-Equal 5 ([int]$intervalParam.default) "Default refresh interval must be the near-realtime cadence."
    Assert-Equal "neuro.official/stock-api" ([string]$manifest.metadata.dependencies.mcpServers[0].id) "Stock Monitor MCP dependency mismatch."

    $surfacePath = Join-Path $artDirectory "surface\main.js"
    $runtimePath = Join-Path $artDirectory "runtime\main.ps1"
    Assert-True (Test-Path -LiteralPath $surfacePath -PathType Leaf) "Stock Monitor JavaScript Surface is missing."
    $surfaceSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $surfacePath
    $runtimeSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath
    $runtimeManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $artDirectory "art.runtime.json") | ConvertFrom-Json
    Assert-Equal 50000 ([int]$runtimeManifest.limits.timeoutMs) "Stock Monitor runtime timeout must match Surface action budget."
    Assert-True ($surfaceSource.Contains("MAX_CANVAS_PIXELS") -and $surfaceSource.Contains("averageValues")) "Stock Monitor Surface must cap Canvas allocation and draw intraday average or MA5 data."
    Assert-True ($surfaceSource.Contains("PERIODS") -and $surfaceSource.Contains("stock_period_commit") -and $surfaceSource.Contains("isIntradayPeriod")) "Stock Monitor Surface must expose multi-period controls and distinct intraday rendering."
    Assert-True ($surfaceSource.Contains("downsampleRows") -and $surfaceSource.Contains("maxPoints")) "Stock Monitor Surface must downsample long market series before Canvas rendering."
    Assert-True ($surfaceSource.Contains("point.close") -and $surfaceSource.Contains("formatClock") -and $surfaceSource.Contains("自动 ")) "Stock Monitor Surface must draw a close-price curve and expose automatic refresh recency."
    Assert-True ($surfaceSource.Contains("PENDING_TIMEOUT_MILLIS") -and $surfaceSource.Contains("ACTION_TIMEOUT_MILLIS")) "Stock Monitor Surface must keep its pending deadline aligned with the action budget."
    Assert-True ($surfaceSource.Contains("RED_UP_MARKETS") -and $surfaceSource.Contains("paletteFor")) "Stock Monitor Surface must pick up/down colors per market convention."
    Assert-True ($surfaceSource -match 'RED_UP_MARKETS\s*=\s*Object\.freeze\(\["SH",\s*"SZ",\s*"BJ",\s*"HK"\]\)') "A-share and Hong Kong markets must render gains red and losses green."
    Assert-True ($surfaceSource.Contains("chart-tip") -and $surfaceSource.Contains("drawCrosshair") -and $surfaceSource.Contains("indexAtPointer")) "Stock Monitor Surface must show a hover tooltip with a crosshair at the pointed data point."
    Assert-True ($surfaceSource.Contains("pointermove") -and $surfaceSource.Contains("pointerleave")) "Hover tooltip must bind pointer enter and leave handling."
    Assert-True ($surfaceSource.Contains("TICK_ACTION") -and $surfaceSource.Contains("fullRefreshTimer") -and $surfaceSource.Contains("CLOSED_MARKET_MIN_SECONDS")) "Stock Monitor Surface must drive near-realtime ticks with a slower full-refresh channel and a closed-market floor."
    Assert-True ($surfaceSource.Contains("updateOrderBook") -and $surfaceSource.Contains("renderBookSide") -and $surfaceSource.Contains("book-board")) "Stock Monitor Surface must render the ten-level order book panel."
    Assert-True ($surfaceSource.Contains("tapeDefinitions") -and $surfaceSource.Contains("换手") -and $surfaceSource.Contains("振幅")) "Stock Monitor Surface must expose the intraday realtime tape fields."
    Assert-True ($surfaceSource -match 'renderBookSide\(refs\.bids[^\n]*palette\)') "Order book levels must be colored through the market-aware palette."
    Assert-True ($runtimeSource.Contains("ConvertTo-OrderBook") -and $runtimeSource.Contains("ConvertTo-LiveTape") -and $runtimeSource.Contains('CallId "orderbook"')) "Stock Monitor runtime must project the Xueqiu order book and realtime tape into state."
    Assert-True ($runtimeSource -match 'frameworkData' -and $runtimeSource -match 'results') "Stock Monitor runtime must consume MCP framework results."
    Assert-True ($runtimeSource.Contains('$script:MaxHistoryRows = 2000') -and $runtimeSource.Contains('Select-Object -Last $script:MaxHistoryRows')) "Stock Monitor runtime must bound provider history to 2000 rows."
    Assert-True ($runtimeSource.Contains('[System.Collections.Generic.List[object]]::new()') -and $runtimeSource.Contains('Get-StockFromActionState')) "Stock Monitor period switching must avoid quadratic history copies and reuse the current quote."
    Assert-True ($runtimeSource -notmatch 'Invoke-RestMethod|push2\.eastmoney\.com|push2his\.eastmoney\.com') "Stock Monitor runtime must not bypass the stock-api MCP server."

    $initialState = @{ code = "SZ000034"; market = "SZ"; intervalSeconds = 60; period = "day"; periodLabel = "日 K"; marketStatus = "closed"; lastTradingDate = "2026-08-14" }
    $refresh = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $initialState -FrameworkData (New-McpData)
    Assert-Equal "success" ([string]$refresh.status) "Stock Monitor refresh did not return a runtime success envelope."
    $surfaceAction = $refresh.output.surfaceAction
    Assert-Equal "loom.surface.v1" ([string]$surfaceAction.protocolVersion) "Stock Monitor Surface protocol mismatch."
    Assert-Equal 1 @($surfaceAction.patches).Count "Stock Monitor refresh must return one authoritative patch."
    Assert-Equal 2 ([int]$surfaceAction.patches[0].statePatch.schemaVersion) "Stock Monitor state schema did not migrate."
    Assert-Equal "ready" ([string]$surfaceAction.patches[0].statePatch.status) "Stock Monitor refresh state did not become ready: $([string]$surfaceAction.patches[0].statePatch.error)"
    $quote = $surfaceAction.result.outputs.quote.value
    Assert-Equal "stock-api" ([string]$quote.provider) "Stock Monitor formal quote provider mismatch."
    Assert-Equal "2.7.3" ([string]$quote.providerVersion) "Stock Monitor formal quote provider version mismatch."
    Assert-Equal "eastmoney" ([string]$quote.source) "Stock Monitor selected provider source mismatch."
    Assert-Equal "SZ000034" ([string]$quote.code) "Stock Monitor code normalization failed."
    Assert-Equal "Digital China" ([string]$quote.name) "Stock Monitor quote name mismatch."
    Assert-True ([double]$quote.price -eq 24.99) "Stock Monitor quote price parsing failed."
    Assert-True ([double]$quote.changePercent -eq 0.4018) "Stock Monitor quote percent conversion failed."
    Assert-Equal 3 @($quote.history.rows).Count "Stock Monitor K-line parsing failed."
    Assert-Equal "day" ([string]$quote.history.period) "Stock Monitor K-line period mismatch."
    Assert-Equal "closed" ([string]$quote.marketStatus) "Stock Monitor must mark stale trading dates as closed."
    Assert-Equal "2026-08-14" ([string]$quote.lastTradingDate) "Stock Monitor last trading date mismatch."
    Assert-True ($null -eq $surfaceAction.result.outputs.PSObject.Properties["trade"]) "Stock Monitor must not return a trading output."
    $refreshBook = $surfaceAction.patches[0].statePatch.orderBook
    Assert-Equal 2 ([int]$refreshBook.levels) "Stock Monitor state must carry the Xueqiu order book depth."
    Assert-True ([double]$refreshBook.bids[0].price -eq 24.98 -and [double]$refreshBook.asks[0].price -eq 24.99) "Stock Monitor order book projection lost its best bid or ask."
    Assert-True ([double]$refreshBook.netVolume -eq -11455) "Stock Monitor order book projection lost the order imbalance."
    $refreshTape = $surfaceAction.patches[0].statePatch.liveTape
    Assert-True ([double]$refreshTape.price -eq 24.99 -and [double]$refreshTape.turnoverRate -eq 7.31) "Stock Monitor realtime tape projection mismatch."
    Assert-Equal 2 ([int]$quote.orderBook.levels) "Formal quote must expose the order book depth."
    Assert-True ([double]$quote.liveTape.avgPrice -eq 24.91) "Formal quote must expose the intraday average price."

    $historyFallback = Invoke-StockRuntime `
        -ArtDirectory $artDirectory `
        -ActionId "stock_refresh" `
        -Payload @{ code = "SZ000034" } `
        -AuthoritativeState $surfaceAction.patches[0].statePatch `
        -FrameworkData (New-McpData -HistoryError)
    $historyFallbackPatch = $historyFallback.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$historyFallbackPatch.status) "A transient history failure must retain a valid quote and previous curve."
    Assert-Equal 3 @($historyFallbackPatch.history).Count "A transient history failure discarded the previous valid curve."
    Assert-True ([double]$historyFallbackPatch.quote.price -eq 24.99) "A transient history failure discarded the refreshed quote."

    $quoteOnly = Invoke-StockRuntime `
        -ArtDirectory $artDirectory `
        -ActionId "stock_refresh" `
        -Payload @{ code = "SZ000034" } `
        -AuthoritativeState $initialState `
        -FrameworkData (New-McpData -HistoryError)
    $quoteOnlyPatch = $quoteOnly.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$quoteOnlyPatch.status) "A first-load history failure must not discard a valid real quote."
    Assert-True ([double]$quoteOnlyPatch.quote.price -eq 24.99) "A first-load history failure lost the real quote price."
    Assert-Equal 0 @($quoteOnlyPatch.history).Count "Quote-only fallback must not fabricate market-history rows."
    Assert-True ([string]$quoteOnlyPatch.statusText -match "曲线将在下次刷新补齐") "Quote-only fallback status did not explain the pending curve refresh."

    $interval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 120 } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    Assert-Equal 120 ([int]$interval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor interval commit failed."
    Assert-True ($null -eq $interval.output.surfaceAction.PSObject.Properties["result"]) "Interval changes must not fabricate a formal quote."

    $liveInterval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 1 } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    Assert-Equal 1 ([int]$liveInterval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor must accept a one-second near-realtime cadence."

    $tickState = $surfaceAction.patches[0].statePatch
    $tick = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_tick_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $tickState -FrameworkData (New-McpData -QuoteOnly)
    $tickPatch = $tick.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$tickPatch.status) "Near-realtime tick must produce a ready state: $([string]$tickPatch.error)"
    Assert-Equal 3 @($tickPatch.history).Count "Near-realtime tick must reuse the cached curve instead of refetching history."
    Assert-True ([double]$tickPatch.quote.price -eq 24.99) "Near-realtime tick lost the refreshed quote price."
    Assert-Equal "day" ([string]$tickPatch.period) "Near-realtime tick must preserve the selected period."
    Assert-Equal 2 ([int]$tickPatch.orderBook.levels) "Near-realtime tick must refresh the live order book."
    Assert-True ([double]$tickPatch.liveTape.price -eq 24.99) "Near-realtime tick lost the realtime tape price."

    $bookFallback = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_tick_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $tickPatch -FrameworkData (New-McpData -QuoteOnly -OrderBookError)
    $bookFallbackPatch = $bookFallback.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$bookFallbackPatch.status) "A failed order-book call must not break the tick: $([string]$bookFallbackPatch.error)"
    Assert-Equal 2 ([int]$bookFallbackPatch.orderBook.levels) "A failed order-book call must retain the last known depth instead of blanking the panel."
    Assert-True ([double]$bookFallbackPatch.quote.price -eq 24.99) "A failed order-book call discarded the refreshed quote."

    $periodState = $surfaceAction.patches[0].statePatch
    $period = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_period_commit" -Payload @{ value = "minute-5" } -AuthoritativeState $periodState -FrameworkData (New-McpData -Period "minute-5" -HistoryOnly)
    Assert-Equal "minute-5" ([string]$period.output.surfaceAction.patches[0].statePatch.period) "Stock Monitor period commit failed."
    Assert-Equal "5 分钟" ([string]$period.output.surfaceAction.patches[0].statePatch.periodLabel) "Stock Monitor period label mismatch."
    Assert-Equal "minute-5" ([string]$period.output.surfaceAction.result.outputs.quote.value.history.period) "Formal quote did not preserve the selected period."

    $periodMismatch = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_period_commit" -Payload @{ value = "minute-5" } -AuthoritativeState $periodState -FrameworkData (New-McpData -Period "day" -HistoryOnly)
    Assert-Equal "error" ([string]$periodMismatch.output.surfaceAction.patches[0].statePatch.status) "Mismatched provider periods must become an explicit error state."
    Assert-True ([string]$periodMismatch.output.surfaceAction.patches[0].statePatch.error -match "行情周期与请求不一致") "Period mismatch detail was not surfaced."

    $invalid = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_symbol_commit" -Payload @{ value = "INVALID" } -AuthoritativeState $initialState -FrameworkData (New-McpData -QuoteError)
    Assert-Equal "error" ([string]$invalid.output.surfaceAction.patches[0].statePatch.status) "Invalid Stock Monitor symbols must become an explicit error state."
    Assert-True ([string]$invalid.output.surfaceAction.patches[0].statePatch.error -match "fixture quote failure") "MCP error detail was not surfaced."
    Assert-Equal "day" ([string]$invalid.output.surfaceAction.patches[0].statePatch.period) "Error state must preserve the selected period."
    Assert-Equal "2026-08-14" ([string]$invalid.output.surfaceAction.patches[0].statePatch.lastTradingDate) "Error state must preserve the last trading date."
    Assert-True ($null -eq $invalid.output.surfaceAction.PSObject.Properties["result"]) "Invalid symbols must not produce a formal quote."

    $plain = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "" -Payload $null -AuthoritativeState $null -FrameworkData (New-McpData) -Params @{ code = "SZ000034"; interval_seconds = 60 }
    Assert-Equal "success" ([string]$plain.status) "Non-Surface Stock Monitor execution failed."
    Assert-Equal "SZ000034" ([string]$plain.output.quote.code) "Non-Surface Stock Monitor output mismatch."
    Assert-Equal 3 @($plain.output.quote.history.rows).Count "Non-Surface Stock Monitor history mismatch."

    Write-Host "Stock Monitor Art contract passed: wrapper=2.8.0 upstream=2.7.3 source=eastmoney+xueqiu periods=13 candles=3 tick=1s order-book=2-levels red-up=CN/HK no-trading=true"
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}
