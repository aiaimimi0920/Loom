param(
    [string]$ArtifactRoot = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$automaticRefreshLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("6Ieq5YqoIA=="))
$turnoverLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5o2i5omL"))
$amplitudeLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5oyv5bmF"))
$undeclaredActionMessage = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("6KGM5oOF5Yqo5L2c5pyq6KKr5aOw5piO77yM5bey5ouS57ud5omn6KGM"))
$dayPeriodLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5pelIEs="))
$pendingCurveMessage = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("5puy57q/5bCG5Zyo5LiL5qyh5Yi35paw6KGl6b2Q"))
$fiveMinutePeriodLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("NSDliIbpkp8="))
$periodMismatchMessage = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("6KGM5oOF5ZGo5pyf5LiO6K+35rGC5LiN5LiA6Ie0"))

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptRoot "stock-monitor-art\Helpers.ps1")
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
    Assert-Equal "1.6.0" ([string]$manifest.metadata.packageSecurity.version) "Stock Monitor package version must force the multi-view migration."
    Assert-Equal "neuro.official/custom-stock-monitor" ([string]$manifest.metadata.art.qualifiedId) "Stock Monitor qualified Art identity mismatch."
    Assert-Equal "mcp" ([string]$manifest.execution.framework) "Stock Monitor must execute through the MCP framework."
    Assert-Equal "=2.9.0" ([string]$manifest.metadata.mcp.version) "Stock Monitor stock-api wrapper version must be exact."
    Assert-Equal 4 @($manifest.metadata.mcp.calls).Count "Stock Monitor must declare quote, history, order-book, and favorites MCP calls."
    Assert-True (@($manifest.metadata.mcp.calls | Where-Object { [string]$_.arguments.source -eq "eastmoney" }).Count -eq 3) "Stock Monitor quote, history, and favorites calls must use the bounded eastmoney provider path."
    $orderBookCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "orderbook" })[0]
    Assert-Equal "get_order_book" ([string]$orderBookCall.toolName) "Stock Monitor order-book call must target the Xueqiu ten-level tool."
    Assert-Equal "auto" ([string]$orderBookCall.arguments.source) "Stock Monitor order-book call must request the automatic pysnowball/Xueqiu source."
    Assert-True (@($manifest.metadata.marketData.sources) -contains "xueqiu") "Stock Monitor must declare Xueqiu among its market-data sources."
    Assert-True (@($manifest.metadata.marketData.sources) -contains "pysnowball") "Stock Monitor must declare pysnowball among its market-data sources."
    Assert-Equal 50000 ([int]$manifest.metadata.capabilities.surface.actions[0].timeoutMs) "Stock Monitor refresh action timeout must cover bounded provider fallback."
    Assert-Equal 50000 ([int]$manifest.metadata.capabilities.surface.actions[1].timeoutMs) "Stock Monitor symbol action timeout must cover bounded provider fallback."
    $periodAction = @($manifest.metadata.capabilities.surface.actions | Where-Object { [string]$_.id -eq "stock_period_commit" })[0]
    Assert-Equal 50000 ([int]$periodAction.timeoutMs) "Stock Monitor period action timeout must cover bounded provider fallback."
    Assert-Equal 4 @($manifest.metadata.capabilities.surface.views).Count "Stock Monitor must declare four developer-defined views."
    Assert-Equal "full" ([string]$manifest.metadata.capabilities.surface.defaultViewId) "Stock Monitor must open in its full view."
    Assert-Equal 960 ([int]$manifest.metadata.capabilities.surface.views[0].fullSize.width) "Stock Monitor full view width mismatch."
    Assert-Equal 820 ([int]$manifest.metadata.capabilities.surface.views[0].fullSize.height) "Stock Monitor full view height mismatch."
    Assert-Equal 620 ([int]$manifest.metadata.capabilities.surface.views[2].fullSize.width) "Stock Monitor trade-price view width mismatch."
    Assert-Equal 620 ([int]$manifest.metadata.capabilities.surface.views[2].fullSize.height) "Stock Monitor trade-price view must fit a complete ten-level order book without scrolling."
    Assert-Equal "favorites-summary" ([string]$manifest.metadata.capabilities.surface.views[3].id) "Stock Monitor favorites view is missing."
    $favoritesCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "favorites" })[0]
    Assert-Equal "get_stocks" ([string]$favoritesCall.toolName) "Favorites summary must use the aggregate stock API."
    Assert-Equal 4 @($favoritesCall.arguments.codes).Count "Favorites summary must declare a bounded default favorite list."
    Assert-Equal 0 @($manifest.metadata.mcp.surfaceActions.stock_interval_commit.calls).Count "Interval updates must skip remote MCP calls."
    Assert-Equal "history" ([string]$manifest.metadata.mcp.surfaceActions.stock_period_commit.calls[0]) "Period switches must reuse the current quote and request only history."
    $tickAction = @($manifest.metadata.capabilities.surface.actions | Where-Object { [string]$_.id -eq "stock_tick_refresh" })[0]
    Assert-True ($null -ne $tickAction) "Stock Monitor must declare a near-realtime tick action."
    Assert-Equal 30000 ([int]$tickAction.timeoutMs) "Tick action timeout must cover one bounded live-provider request."
    Assert-True (-not [bool]$tickAction.progress) "Tick action must not raise progress noise at second-level cadence."
    Assert-Equal 1 @($manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls).Count "Tick refresh must request only the live order-book/tape call."
    Assert-Equal "orderbook" ([string]$manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls[0]) "Tick refresh must reuse the authoritative quote and fetch only live data."
    $intervalParam = @($manifest.params | Where-Object { [string]$_.id -eq "interval_seconds" })[0]
    Assert-Equal 1 ([int]$intervalParam.min) "Refresh interval must reach one second for near-realtime monitoring."
    Assert-Equal 5 ([int]$intervalParam.default) "Default refresh interval must be the near-realtime cadence."
    Assert-Equal "neuro.official/stock-api" ([string]$manifest.metadata.dependencies.mcpServers[0].id) "Stock Monitor MCP dependency mismatch."

    $surfacePath = Join-Path $artDirectory "surface\main.js"
    $runtimePath = Join-Path $artDirectory "runtime\main.ps1"
    $runtimeModuleRoot = Join-Path $artDirectory "runtime\lib"
    $runtimeModuleNames = @("Constants.ps1", "Domain.ps1", "Mcp.ps1", "Output.ps1", "Protocol.ps1", "Snapshot.ps1", "Transforms.ps1")
    Assert-True (Test-Path -LiteralPath $surfacePath -PathType Leaf) "Stock Monitor JavaScript Surface is missing."
    $javascriptVariant = @($manifest.metadata.capabilities.surface.variants | Where-Object { [string]$_.runtime -eq "javascript" })[0]
    $surfaceDescriptor = Get-Content -Raw -Encoding UTF8 -LiteralPath ($surfacePath + ".sources.json") | ConvertFrom-Json
    Assert-Equal 1 ([int]$surfaceDescriptor.schemaVersion) "Stock Monitor JavaScript Surface source descriptor schema mismatch."
    Assert-Equal 10 @($surfaceDescriptor.sourceFiles).Count "Stock Monitor JavaScript Surface module count mismatch."
    $surfaceSource = Read-JavaScriptSurfaceSource -PackageRoot $artDirectory -Variant $javascriptVariant
    $actualRuntimeModuleNames = @(Get-ChildItem -LiteralPath $runtimeModuleRoot -Filter *.ps1 -File | ForEach-Object { $_.Name } | Sort-Object)
    Assert-Equal ($runtimeModuleNames -join ",") ($actualRuntimeModuleNames -join ",") "Stock Monitor PowerShell runtime module set mismatch."
    $runtimeSource = @(
        Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath
        $runtimeModuleNames | ForEach-Object { Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $runtimeModuleRoot $_) }
    ) -join [Environment]::NewLine
    Assert-True ($runtimeSource.Contains('$script:StockMonitorRuntimeRoot = $PSScriptRoot') -and $runtimeSource.Contains('"Snapshot.ps1"')) "Stock Monitor runtime entry must load its fixed module graph from the package-local runtime root."
    $runtimeManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $artDirectory "art.runtime.json") | ConvertFrom-Json
    Assert-Equal 50000 ([int]$runtimeManifest.limits.timeoutMs) "Stock Monitor runtime timeout must match Surface action budget."
    Assert-True ($surfaceSource.Contains("MAX_CANVAS_PIXELS") -and $surfaceSource.Contains("averageValues")) "Stock Monitor Surface must cap Canvas allocation and draw intraday average or MA5 data."
    Assert-True ($surfaceSource.Contains("PERIODS") -and $surfaceSource.Contains("stock_period_commit") -and $surfaceSource.Contains("isIntradayPeriod")) "Stock Monitor Surface must expose multi-period controls and distinct intraday rendering."
    Assert-True ($surfaceSource.Contains("downsampleRows") -and $surfaceSource.Contains("maxPoints")) "Stock Monitor Surface must downsample long market series before Canvas rendering."
    Assert-True ($surfaceSource.Contains("point.close") -and $surfaceSource.Contains("formatClock") -and $surfaceSource.Contains($automaticRefreshLabel)) "Stock Monitor Surface must draw a close-price curve and expose automatic refresh recency."
    Assert-True ($surfaceSource.Contains("PENDING_TIMEOUT_MILLIS") -and $surfaceSource.Contains("ACTION_TIMEOUT_MILLIS")) "Stock Monitor Surface must keep its pending deadline aligned with the action budget."
    Assert-True ($surfaceSource.Contains("RED_UP_MARKETS") -and $surfaceSource.Contains("paletteFor")) "Stock Monitor Surface must pick up/down colors per market convention."
    Assert-True ($surfaceSource -match 'RED_UP_MARKETS\s*=\s*Object\.freeze\(\["SH",\s*"SZ",\s*"BJ",\s*"HK"\]\)') "A-share and Hong Kong markets must render gains red and losses green."
    Assert-True ($surfaceSource.Contains("chart-tip") -and $surfaceSource.Contains("drawCrosshair") -and $surfaceSource.Contains("indexAtPointer")) "Stock Monitor Surface must show a hover tooltip with a crosshair at the pointed data point."
    Assert-True ($surfaceSource.Contains("pointermove") -and $surfaceSource.Contains("pointerleave")) "Hover tooltip must bind pointer enter and leave handling."
    Assert-True ($surfaceSource.Contains("TICK_ACTION") -and $surfaceSource.Contains("ticksSinceFullRefresh") -and $surfaceSource.Contains("ticksPerFullRefresh") -and $surfaceSource.Contains("CLOSED_MARKET_MIN_SECONDS")) "Stock Monitor Surface must promote every N-th tick to a full refresh on a single timer and keep the closed-market floor."
    Assert-True ($surfaceSource.Contains("TICK_RETRY_COOLDOWN_MILLIS") -and $surfaceSource.Contains("tickChannelEnabled")) "A refused tick must disable the near-realtime channel for a cooldown only, not for the whole surface lifetime."
    Assert-True ($surfaceSource.Contains("actionBudgetsMillis") -and $surfaceSource.Contains("ACTION_DISPATCH_GRACE_MILLIS") -and $surfaceSource.Contains("clientDeadlineOf")) "Stock Monitor Surface must derive its client timeout from the host action budget instead of a fixed constant."
    Assert-True ($surfaceSource.Contains("TICK_ACTION_TIMEOUT_MILLIS = " + [int]$tickAction.timeoutMs)) "Surface tick budget mirror must match the manifest tick timeout."
    $intervalAction = @($manifest.metadata.capabilities.surface.actions | Where-Object { [string]$_.id -eq "stock_interval_commit" })[0]
    Assert-True ($surfaceSource.Contains("INTERVAL_COMMIT_TIMEOUT_MILLIS = " + [int]$intervalAction.timeoutMs)) "Surface interval-commit budget mirror must match the manifest timeout."
    Assert-True ($surfaceSource.Contains("pendingRequestId") -and $surfaceSource.Contains("lastRequestId") -and $surfaceSource.Contains("settledBy")) "Pending state must be released by the action that produced the revision, not by any revision bump."
    Assert-True (-not ($surfaceSource -match 'refs\.(tip|legend)\.innerHTML')) "Chart tooltip and legend must be built from DOM nodes, not interpolated provider text."
    Assert-True ($surfaceSource.Contains("refs.tip.replaceChildren") -and $surfaceSource.Contains("refs.legend.replaceChildren")) "Chart tooltip and legend must be swapped in through replaceChildren."
    Assert-True ($surfaceSource.Contains("updateOrderBook") -and $surfaceSource.Contains("renderBookSide") -and $surfaceSource.Contains("book-board")) "Stock Monitor Surface must render the ten-level order book panel."
    Assert-True ($surfaceSource.Contains("tapeDefinitions") -and $surfaceSource.Contains($turnoverLabel) -and $surfaceSource.Contains($amplitudeLabel)) "Stock Monitor Surface must expose the intraday realtime tape fields."
    Assert-True ($surfaceSource.Contains('overflow:hidden;background:') -and -not $surfaceSource.Contains('.stock-shell{min-width:0;min-height:100%;height:100%;overflow:auto')) "Stock Monitor must not require root scrolling to reveal its target region."
    Assert-True ($surfaceSource.Contains("chart-table") -and $surfaceSource.Contains("trade-price") -and $surfaceSource.Contains("favorites-summary")) "Stock Monitor Surface must render all declared views."
    Assert-True ($surfaceSource.Contains("updateHistoryTable") -and $surfaceSource.Contains("updateFavorites")) "Stock Monitor specialized views must render their table and favorites data."
    Assert-True ($surfaceSource -match 'renderBookSide\(refs\.bids[^\n]*palette\)') "Order book levels must be colored through the market-aware palette."
    Assert-True ($surfaceSource.Contains("if (canvas.width !== nextWidth)") -and $surfaceSource.Contains("if (canvas.height !== nextHeight)")) "Canvas pixel size must only be assigned when it changed; assigning it reallocates the backing bitmap on every redraw."
    Assert-True ($surfaceSource.Contains("new ResizeObserver(scheduleChartRedraw)") -and $surfaceSource.Contains("resizeFrame")) "Resize redraws must be coalesced into one animation frame instead of calling drawChart per observer callback."
    Assert-True ($surfaceSource.Contains("chartSampleOf") -and $surfaceSource.Contains("seriesCache") -and $surfaceSource.Contains("sampleCache")) "The derived chart series and its downsampled points must be memoized across redraws."
    Assert-True ($surfaceSource.Contains("MOVING_AVERAGE_WINDOW") -and -not ($surfaceSource -match 'points\.slice\(index - 4')) "The moving average must use a rolling accumulator, not a slice/map/reduce per point."
    Assert-True ($surfaceSource.Contains("ensureChildren") -and $surfaceSource.Contains("paintedKey")) "The market, order book, history and favorites blocks must reuse their nodes and skip repaints when the revision did not change."
    Assert-True (-not ($surfaceSource -match 'replaceChildren\(\);\s*\r?\n\s*(levels|metricDefinitions|tapeDefinitions)')) "No updater may rebuild its rows from scratch on every frame."
    Assert-True ($runtimeSource.Contains("ConvertTo-OrderBook") -and $runtimeSource.Contains("ConvertTo-LiveTape") -and $runtimeSource.Contains('CallId "orderbook"')) "Stock Monitor runtime must project the Xueqiu order book and realtime tape into state."
    Assert-True ($runtimeSource -match 'frameworkData' -and $runtimeSource -match 'results') "Stock Monitor runtime must consume MCP framework results."
    Assert-True ($runtimeSource.Contains('$script:MaxHistoryRows = 2000') -and $runtimeSource.Contains('Select-LastBoundedValue -Values $Values -Limit $script:MaxHistoryRows')) "Stock Monitor runtime must select at most the last 2000 provider rows without materializing an unbounded pipeline."
    Assert-True ($runtimeSource.Contains('[System.Collections.Generic.List[object]]::new()') -and $runtimeSource.Contains('Get-StockFromActionState')) "Stock Monitor period switching must avoid quadratic history copies and reuse the current quote."
    Assert-True ($runtimeSource -notmatch 'Invoke-RestMethod|push2\.eastmoney\.com|push2his\.eastmoney\.com') "Stock Monitor runtime must not bypass the stock-api MCP server."
    Assert-True ($runtimeSource.Contains("Get-SurfaceActionBudgets") -and $runtimeSource.Contains("lastRequestId") -and $runtimeSource.Contains("actionBudgetsMillis")) "Stock Monitor runtime must echo the action correlation and publish its effective action budgets."
    Assert-Equal 3 ([regex]::Matches($runtimeSource, 'Add-ActionEcho -StatePatch').Count) "Every Stock Monitor state patch must carry the action echo; statePatch merge semantics would otherwise leave a stale correlation id."
    Assert-True ($runtimeSource.Contains("function Resolve-SurfaceAction") -and $runtimeSource -notmatch 'Find-SurfaceAction') "The Surface action must be resolved from fixed request positions; a recursive search lets any MCP result that carries a surfaceAction key fabricate an action invocation."
    Assert-True ($runtimeSource.Contains("conflicting surfaceAction invocations were provided")) "Two different surfaceAction objects in one request must be rejected instead of silently resolving to one of them."
    Assert-True ($runtimeSource.Contains('[System.Globalization.DateTimeStyles]::None') -and $runtimeSource -notmatch 'DateTimeStyles\]::AssumeUniversal') "Upstream timestamps must not be parsed with AssumeUniversal; a local-time string would be read as UTC and understate the record age by the whole offset."
    Assert-True ($runtimeSource.Contains('(?:[Zz]|[+-]\d{2}:?\d{2})$')) "Upstream timestamps must carry an explicit UTC offset to be accepted."
    Assert-Equal 0 ([regex]::Matches($runtimeSource, 'normalizedFetchedAt = \[DateTimeOffset\]::UtcNow').Count) "The runtime must not synthesize an observation timestamp for a provider record that shipped without one; that turns an unknown age into a fresh one."
    Assert-True ($runtimeSource.Contains('($null -eq $ageSeconds) -or ($ageSeconds -gt $script:MaxOrderBookAgeSeconds)') -and $runtimeSource.Contains('($null -eq $ageSeconds) -or ($ageSeconds -gt $script:MaxLiveAgeSeconds)')) 'An unknown record age must fail closed as stale; $null -gt 90 is false in PowerShell, so the age comparison alone would report an ageless record fresh.'
    Assert-True ($runtimeSource.Contains("function Limit-MessageLength") -and $runtimeSource.Contains("GetTextElementEnumerator")) "Error text must be truncated on text-element boundaries so a surrogate pair or combining mark is never split."
    Assert-True ($runtimeSource.Contains('New-FormalQuote -Snapshot $snapshot -ReferenceState') -and $runtimeSource.Contains("authoritativeState.history")) "The Surface-path formal quote must reference the authoritative state instead of serializing the same collections a second time."
    Assert-True ($runtimeSource.Contains('statePatch = [ordered]@{}')) "The Surface result must carry an empty no-op state patch; omitting the field deserializes to null and the daemon merge replaces the whole authoritative state."
    Assert-True ($runtimeSource.Contains($undeclaredActionMessage) -and $runtimeSource.Contains("-RejectAction")) "An undeclared action must be refused with a fixed message instead of a throw that interpolates the caller's action id."
    Assert-True ($runtimeSource.Contains('$script:MaxRequestBytes = 4 * 1024 * 1024') -and $runtimeSource.Contains("Read-BoundedStandardInput")) "Stock Monitor stdin must be decoded through a fixed byte limit instead of ReadToEnd."
    Assert-True ($runtimeSource.Contains("Assert-JsonTextDepth") -and $runtimeSource.Contains("Assert-RequestObjectGraph") -and $runtimeSource.Contains('$script:MaxJsonDepth = 32')) "Stock Monitor requests must reject hostile JSON nesting before parsing and bound the decoded graph on Windows PowerShell 5.1."
    Assert-True ($runtimeSource.Contains("ConvertTo-StrictBoolean") -and $runtimeSource.Contains("Get-SafeMcpErrorMessage")) "Stock Monitor provider booleans and error projection must reject coercion and credential-bearing messages."
    Assert-True ($surfaceSource.Contains("staleLabel") -and $surfaceSource.Contains("is-stale")) "The Surface must badge stale order-book and tape records; the runtime already decides staleness but the clock string alone looks identical either way."
    Assert-True ($surfaceSource.Contains("state.historyWarning")) "The Surface must render the non-fatal history warning; otherwise a chart-less panel never says why."
    $surfaceTestPath = Join-Path $repoRoot "scripts\tests\Test-LoomStockMonitorSurface.mjs"
    $nodePath = [string](Get-Command node.exe -ErrorAction Stop).Source
    & $nodePath $surfaceTestPath $surfacePath
    Assert-Equal 0 $LASTEXITCODE "Stock Monitor Surface VM contract failed."

    $initialState = @{ code = "SZ000034"; market = "SZ"; intervalSeconds = 60; period = "day"; periodLabel = $dayPeriodLabel; marketStatus = "closed"; lastTradingDate = "2026-08-14" }
    $refresh = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $initialState -FrameworkData (New-McpData)
    Assert-Equal "success" ([string]$refresh.status) "Stock Monitor refresh did not return a runtime success envelope."
    $surfaceAction = $refresh.output.surfaceAction
    Assert-Equal "loom.surface.v1" ([string]$surfaceAction.protocolVersion) "Stock Monitor Surface protocol mismatch."
    Assert-Equal 1 @($surfaceAction.patches).Count "Stock Monitor refresh must return one authoritative patch."
    Assert-Equal 2 ([int]$surfaceAction.patches[0].statePatch.schemaVersion) "Stock Monitor state schema did not migrate."
    Assert-Equal "ready" ([string]$surfaceAction.patches[0].statePatch.status) "Stock Monitor refresh state did not become ready: $([string]$surfaceAction.patches[0].statePatch.error)"
    $quote = $surfaceAction.result.outputs.quote.value
    Assert-Equal "stock-api" ([string]$quote.provider) "Stock Monitor formal quote provider mismatch."
    Assert-Equal "2.9.0" ([string]$quote.providerVersion) "Stock Monitor formal quote provider version mismatch."
    Assert-Equal "2.7.3" ([string]$quote.upstreamVersion) "Stock Monitor formal quote upstream version mismatch."
    Assert-Equal "eastmoney" ([string]$quote.source) "Stock Monitor selected provider source mismatch."
    Assert-Equal "SZ000034" ([string]$quote.code) "Stock Monitor code normalization failed."
    Assert-Equal "Digital China" ([string]$quote.name) "Stock Monitor quote name mismatch."
    Assert-True ([double]$quote.price -eq 24.99) "Stock Monitor quote price parsing failed."
    Assert-True ([double]$quote.changePercent -eq 0.4018) "Stock Monitor quote percent conversion failed."
    Assert-True ($null -eq $quote.history.PSObject.Properties["rows"]) "The Surface-path formal quote must not repeat the K-line rows that the same response already carries in its state patch."
    Assert-Equal 3 ([int]$quote.history.rowCount) "Stock Monitor K-line parsing failed."
    Assert-Equal "authoritativeState.history" ([string]$quote.history.rowsIn) "The Surface-path formal quote must point at the authoritative state instead of duplicating the rows."
    Assert-Equal 3 @($surfaceAction.patches[0].statePatch.history).Count "Stock Monitor state patch must carry the parsed K-line rows."
    Assert-Equal "day" ([string]$quote.history.period) "Stock Monitor K-line period mismatch."
    Assert-Equal "closed" ([string]$quote.marketStatus) "Stock Monitor must mark stale trading dates as closed."
    Assert-Equal "2026-08-14" ([string]$quote.lastTradingDate) "Stock Monitor last trading date mismatch."
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$quote.fetchedAt)) "Stock Monitor formal quote fetch timestamp is missing."
    Assert-True ($null -ne $quote.PSObject.Properties["stale"]) "Stock Monitor formal quote freshness metadata is missing."
    Assert-True ($null -eq $surfaceAction.result.outputs.PSObject.Properties["trade"]) "Stock Monitor must not return a trading output."
    $refreshBook = $surfaceAction.patches[0].statePatch.orderBook
    Assert-Equal 2 ([int]$refreshBook.levels) "Stock Monitor state must carry the Xueqiu order book depth."
    Assert-True ([double]$refreshBook.bids[0].price -eq 24.98 -and [double]$refreshBook.asks[0].price -eq 24.99) "Stock Monitor order book projection lost its best bid or ask."
    Assert-True ([double]$refreshBook.netVolume -eq -11455) "Stock Monitor order book projection lost the order imbalance."
    $refreshTape = $surfaceAction.patches[0].statePatch.liveTape
    Assert-True ([double]$refreshTape.price -eq 24.99 -and [double]$refreshTape.turnoverRate -eq 7.31) "Stock Monitor realtime tape projection mismatch."
    Assert-Equal "authoritativeState" ([string]$quote.collectionsIn) "The Surface-path formal quote must reference the authoritative state for its collections."
    Assert-True ($null -eq $quote.PSObject.Properties["orderBook"] -and $null -eq $quote.PSObject.Properties["liveTape"] -and $null -eq $quote.PSObject.Properties["favoriteQuotes"]) "The Surface-path formal quote must not repeat the order book, tape, and favorites that the same response already carries in its state patch."
    Assert-Equal 2 ([int]$quote.orderBookLevels) "Formal quote must expose the order book depth."
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$quote.liveTapeObservedAt)) "Formal quote must expose the realtime tape observation time."
    Assert-True ([double]$refreshTape.avgPrice -eq 24.91) "Stock Monitor state must expose the intraday average price."
    Assert-Equal 4 ([int]$quote.favoriteQuoteCount) "Formal quote must expose the aggregated favorite count."
    Assert-Equal 4 @($surfaceAction.patches[0].statePatch.favoriteQuotes).Count "Surface state must preserve favorite prices across view changes."
    Assert-True ($null -ne $surfaceAction.result.PSObject.Properties["statePatch"]) "The Surface result must still declare a state patch field; omitting it deserializes to null and the daemon merge replaces the whole authoritative state."
    Assert-Equal 0 @($surfaceAction.result.statePatch.PSObject.Properties).Count "The Surface result patch must be an empty no-op object; the real patch travels once, in patches[0]."

    foreach ($favoritesFailure in @(
        @{ Label = "missing"; Data = (New-McpData -FavoritesOmitted) },
        @{ Label = "error"; Data = (New-McpData -FavoritesError) },
        @{ Label = "malformed"; Data = (New-McpData -FavoritesMalformed) }
    )) {
        $favoritesFallback = Invoke-StockRuntime `
            -ArtDirectory $artDirectory `
            -ActionId "stock_refresh" `
            -Payload @{ code = "SZ000034" } `
            -AuthoritativeState $surfaceAction.patches[0].statePatch `
            -FrameworkData $favoritesFailure.Data
        $favoritesFallbackPatch = $favoritesFallback.output.surfaceAction.patches[0].statePatch
        Assert-Equal "ready" ([string]$favoritesFallbackPatch.status) "A $($favoritesFailure.Label) favorites result must not fail the main quote."
        Assert-Equal 4 @($favoritesFallbackPatch.favoriteQuotes).Count "A $($favoritesFailure.Label) favorites result must retain the last valid summary."
    }

    $firstLoadWithoutFavorites = Invoke-StockRuntime `
        -ArtDirectory $artDirectory `
        -ActionId "stock_refresh" `
        -Payload @{ code = "SZ000034" } `
        -AuthoritativeState $initialState `
        -FrameworkData (New-McpData -FavoritesOmitted)
    $firstLoadWithoutFavoritesPatch = $firstLoadWithoutFavorites.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$firstLoadWithoutFavoritesPatch.status) "A missing first-load favorites result must not fail the main quote."
    Assert-Equal 0 @($firstLoadWithoutFavoritesPatch.favoriteQuotes).Count "A missing first-load favorites result must remain empty instead of fabricating prices."

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
    Assert-True ([string]$quoteOnlyPatch.statusText -match [regex]::Escape($pendingCurveMessage)) "Quote-only fallback status did not explain the pending curve refresh."
    Assert-True ([string]$quoteOnlyPatch.historyWarning -match "fixture history failure") "A chart-less quote must forward the upstream history failure as a non-fatal warning instead of dropping it."
    Assert-True ($null -eq $historyFallbackPatch.historyWarning) "A history failure that kept the previous curve must not raise a warning about a missing chart."
    Assert-True ($null -eq $surfaceAction.patches[0].statePatch.historyWarning) "A fully successful refresh must not raise a history warning."

    $interval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 120 } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    Assert-Equal 120 ([int]$interval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor interval commit failed."
    Assert-True ($null -eq $interval.output.surfaceAction.PSObject.Properties["result"]) "Interval changes must not fabricate a formal quote."

    $liveInterval = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_interval_commit" -Payload @{ value = 1 } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    Assert-Equal 1 ([int]$liveInterval.output.surfaceAction.patches[0].statePatch.intervalSeconds) "Stock Monitor must accept a one-second near-realtime cadence."

    $tickState = $surfaceAction.patches[0].statePatch
    $liveObservedAt = [DateTimeOffset]::UtcNow.ToString("o")
    $tick = Invoke-StockRuntime `
        -ArtDirectory $artDirectory `
        -ActionId "stock_tick_refresh" `
        -Payload @{ code = "SZ000034" } `
        -AuthoritativeState $tickState `
        -FrameworkData (New-McpData -OrderBookOnly -LivePrice 25.53 -LiveTrading -LiveObservedAt $liveObservedAt)
    $tickPatch = $tick.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$tickPatch.status) "Near-realtime tick must produce a ready state: $([string]$tickPatch.error)"
    Assert-Equal 3 @($tickPatch.history).Count "Near-realtime tick must reuse the cached curve instead of refetching history."
    Assert-True ([double]$tickPatch.quote.price -eq 25.53) "Near-realtime tick did not prioritize the fresh live tape price."
    Assert-Equal "pysnowball" ([string]$tickPatch.quote.source) "Near-realtime tick did not expose the selected live source."
    Assert-Equal "open" ([string]$tickPatch.marketStatus) "A trading live tape must mark the market session open."
    Assert-Equal "day" ([string]$tickPatch.period) "Near-realtime tick must preserve the selected period."
    Assert-Equal 2 ([int]$tickPatch.orderBook.levels) "Near-realtime tick must refresh the live order book."
    Assert-True ([double]$tickPatch.liveTape.price -eq 25.53) "Near-realtime tick lost the realtime tape price."
    Assert-True (-not [bool]$tickPatch.liveTape.stale -and [double]$tickPatch.liveTape.ageSeconds -le 90) "Fresh live tape metadata is invalid."

    $bookFallback = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_tick_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $tickPatch -FrameworkData (New-McpData -OrderBookOnly -OrderBookError)
    $bookFallbackPatch = $bookFallback.output.surfaceAction.patches[0].statePatch
    Assert-Equal "ready" ([string]$bookFallbackPatch.status) "A failed order-book call must not break the tick: $([string]$bookFallbackPatch.error)"
    Assert-Equal 2 ([int]$bookFallbackPatch.orderBook.levels) "A failed order-book call must retain the last known depth instead of blanking the panel."
    Assert-True ([double]$bookFallbackPatch.quote.price -eq 25.53) "A failed order-book call discarded the authoritative live quote."

    $staleState = $tickPatch | ConvertTo-Json -Depth 40 | ConvertFrom-Json
    $staleState.orderBook.observedAt = "2020-01-01T00:00:00.000Z"
    $staleState.orderBook.fetchedAt = "2020-01-01T00:00:00.000Z"
    $staleState.liveTape.observedAt = "2020-01-01T00:00:00.000Z"
    $staleState.liveTape.fetchedAt = "2020-01-01T00:00:00.000Z"
    $staleFallback = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_tick_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $staleState -FrameworkData (New-McpData -OrderBookOnly -OrderBookError)
    $staleFallbackPatch = $staleFallback.output.surfaceAction.patches[0].statePatch
    Assert-True ($null -eq $staleFallbackPatch.orderBook) "An expired order book must not be reused indefinitely."
    Assert-True ($null -eq $staleFallbackPatch.liveTape) "An expired live tape must not be reused indefinitely."

    $naiveState = $tickPatch | ConvertTo-Json -Depth 40 | ConvertFrom-Json
    $naiveState.orderBook.observedAt = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fff")
    $naiveState.orderBook.fetchedAt = $naiveState.orderBook.observedAt
    $naiveState.liveTape.observedAt = $naiveState.orderBook.observedAt
    $naiveState.liveTape.fetchedAt = $naiveState.orderBook.observedAt
    $naiveFallback = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_tick_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $naiveState -FrameworkData (New-McpData -OrderBookOnly -OrderBookError)
    $naiveFallbackPatch = $naiveFallback.output.surfaceAction.patches[0].statePatch
    Assert-True ($null -eq $naiveFallbackPatch.orderBook) "A timestamp without an explicit UTC offset must be rejected instead of assumed to be UTC; the order book age is unknown, so it must not be reused."
    Assert-True ($null -eq $naiveFallbackPatch.liveTape) "A timestamp without an explicit UTC offset must not keep the live tape alive."

    $undeclared = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_evil_action" -Payload @{ value = "BJ430047" } -AuthoritativeState $initialState -FrameworkData (New-McpData -Skipped)
    $undeclaredPatch = $undeclared.output.surfaceAction.patches[0].statePatch
    Assert-Equal "error" ([string]$undeclaredPatch.status) "An undeclared action id must produce an explicit error state."
    Assert-Equal $undeclaredActionMessage ([string]$undeclaredPatch.error) "A rejected action must report a fixed message; interpolating the caller's action id lets an undeclared action push arbitrary text into stored state and onto the panel."
    Assert-Equal "SZ000034" ([string]$undeclaredPatch.code) "A rejected action must take its symbol from the authoritative state, not from the rejected payload."
    Assert-True ($null -eq $undeclaredPatch.lastActionId -and $null -eq $undeclaredPatch.lastRequestId) "A rejected action must not be echoed into the stored correlation fields."
    Assert-True ($null -eq $undeclared.output.surfaceAction.PSObject.Properties["result"]) "A rejected action must not produce a formal quote."

    $closedPrice = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{ code = "SZ000034" } -AuthoritativeState $initialState -FrameworkData (New-McpData -QuotePrice 26.00)
    Assert-True ([double]$closedPrice.output.surfaceAction.patches[0].statePatch.quote.price -eq 24.99) "A closed market must display the latest trading-day close."

    $bjState = @{ code = "BJ430047"; market = "BJ"; intervalSeconds = 60; period = "day"; periodLabel = $dayPeriodLabel; marketStatus = "closed"; lastTradingDate = "2026-08-14" }
    $bj = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_refresh" -Payload @{ code = "BJ430047" } -AuthoritativeState $bjState -FrameworkData (New-McpData -Code "BJ430047")
    Assert-Equal "BJ430047" ([string]$bj.output.surfaceAction.patches[0].statePatch.quote.code) "Beijing Exchange code normalization failed."
    Assert-Equal "BJ" ([string]$bj.output.surfaceAction.patches[0].statePatch.quote.market) "Beijing Exchange market classification failed."

    $periodState = $surfaceAction.patches[0].statePatch
    $period = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_period_commit" -Payload @{ value = "minute-5" } -AuthoritativeState $periodState -FrameworkData (New-McpData -Period "minute-5" -HistoryOnly)
    Assert-Equal "minute-5" ([string]$period.output.surfaceAction.patches[0].statePatch.period) "Stock Monitor period commit failed."
    Assert-Equal $fiveMinutePeriodLabel ([string]$period.output.surfaceAction.patches[0].statePatch.periodLabel) "Stock Monitor period label mismatch."
    Assert-Equal "minute-5" ([string]$period.output.surfaceAction.result.outputs.quote.value.history.period) "Formal quote did not preserve the selected period."

    $periodMismatch = Invoke-StockRuntime -ArtDirectory $artDirectory -ActionId "stock_period_commit" -Payload @{ value = "minute-5" } -AuthoritativeState $periodState -FrameworkData (New-McpData -Period "day" -HistoryOnly)
    Assert-Equal "error" ([string]$periodMismatch.output.surfaceAction.patches[0].statePatch.status) "Mismatched provider periods must become an explicit error state."
    Assert-True ([string]$periodMismatch.output.surfaceAction.patches[0].statePatch.error -match [regex]::Escape($periodMismatchMessage)) "Period mismatch detail was not surfaced."

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

    Write-Host "Stock Monitor Art contract passed: wrapper=2.9.0 upstream=2.7.3 source=aggregate+pysnowball+xueqiu periods=13 candles=3 tick=1s order-book=2-levels freshness=bounded BJ=verified red-up=CN/HK no-trading=true"
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}
