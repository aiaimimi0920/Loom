# Owns the ordered MCP-to-Stock Monitor snapshot aggregation workflow.

function Get-StockSnapshot {
    param([object]$Request)

    $actionId = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -MaxLength 64 -DefaultValue ""
    $quoteAttempt = Try-Get-McpToolContent -Request $Request -CallId "quote"
    $quoteContent = Get-ObjectPropertyValue -Value $quoteAttempt -Name "content"
    $historyAttempt = Try-Get-McpToolContent -Request $Request -CallId "history"
    $historyContent = Get-ObjectPropertyValue -Value $historyAttempt -Name "content"
    $historyError = [string](Get-ObjectPropertyValue -Value $historyAttempt -Name "error")
    $quoteResponse = Get-ObjectPropertyValue -Value $quoteContent -Name "response"
    $historyResponse = Get-ObjectPropertyValue -Value $historyContent -Name "response"
    $historyInput = Get-ObjectPropertyValue -Value $historyContent -Name "input"
    $stock = Get-ObjectPropertyValue -Value $quoteResponse -Name "stock"
    if ($null -eq $stock -and $null -ne $script:SurfaceAction) {
        $stock = Get-StockFromActionState -Action $script:SurfaceAction
    }
    if ($null -eq $stock) {
        $quoteError = [string](Get-ObjectPropertyValue -Value $quoteAttempt -Name "error" -DefaultValue "stock-api 未返回股票报价")
        throw $quoteError
    }

    $code = Resolve-StockCode (Get-ObjectPropertyValue -Value $stock -Name "code")
    $name = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $stock -Name "name") -MaxLength 128
    $price = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "now")
    if ([string]::IsNullOrWhiteSpace($name) -or $name -eq "---" -or $null -eq $price -or $price -le 0) {
        throw "stock-api 未找到该代码的有效报价"
    }
    $fetchCompletedAt = [DateTimeOffset]::UtcNow.ToString("o")
    $quoteFetchedAt = Resolve-UtcTimestamp `
        -Value (Get-ObjectPropertyValue -Value $quoteResponse -Name "fetchedAt") `
        -FallbackValue (Get-ObjectPropertyValue -Value $stock -Name "fetchedAt" -DefaultValue $fetchCompletedAt)
    if ($null -eq $quoteFetchedAt) { $quoteFetchedAt = $fetchCompletedAt }
    $quoteObservedAt = Resolve-UtcTimestamp `
        -Value (Get-ObjectPropertyValue -Value $stock -Name "observedAt") `
        -FallbackValue $quoteFetchedAt
    $quoteProviderStale = ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $quoteResponse -Name "stale" -DefaultValue (Get-ObjectPropertyValue -Value $stock -Name "stale" -DefaultValue $false))

    $requestedPeriodValue = if ($actionId -eq "stock_period_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue "day"
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "period" -DefaultValue "day"
    }
    $period = if ($null -ne $historyInput) {
        Resolve-MarketPeriod (Get-ObjectPropertyValue -Value $historyInput -Name "period" -DefaultValue $requestedPeriodValue)
    }
    else {
        Resolve-MarketPeriod $requestedPeriodValue
    }
    $history = @(ConvertTo-HistoryRows (Get-ObjectPropertyValue -Value $historyResponse -Name "klines" -DefaultValue @()))
    if ($history.Count -eq 0 -and $null -ne $script:SurfaceAction) {
        $requestedCodeValue = if ($actionId -eq "stock_symbol_commit") {
            Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $code
        }
        else {
            Get-ActionStateValue -Action $script:SurfaceAction -Name "code" -DefaultValue $code
        }
        $requestedCode = try { Resolve-StockCode $requestedCodeValue } catch { "" }
        $statePeriod = Resolve-MarketPeriod (Get-ActionStateValue -Action $script:SurfaceAction -Name "period" -DefaultValue $period)
        if ($requestedCode -eq $code -and $statePeriod -eq $period) {
            $history = @(ConvertTo-HistoryRows (Get-ActionStateValue -Action $script:SurfaceAction -Name "history" -DefaultValue @()))
        }
    }
    $source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $stock -Name "source") -MaxLength 32 -DefaultValue "").ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($source) -and $history.Count -gt 0) {
        $source = [string]$history[-1].source
    }
    if ([string]::IsNullOrWhiteSpace($source)) { $source = "unknown" }

    $market = Get-MarketFromCode -Code $code
    $previousClose = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "yesterday")
    $percentFraction = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $stock -Name "percent") -Digits 8
    $change = if ($null -ne $previousClose -and $previousClose -gt 0) {
        [Math]::Round($price - $previousClose, 4)
    }
    else { $null }
    $changePercent = if ($null -ne $percentFraction) { [Math]::Round($percentFraction * 100, 4) } else { $null }
    $latest = if ($history.Count -gt 0) {
        $history[-1]
    }
    else {
        $fallbackHigh = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "high")
        $fallbackLow = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "low")
        [ordered]@{
            date = (Get-Date).ToString("yyyy-MM-dd")
            open = if ($null -ne $previousClose -and $previousClose -gt 0) { $previousClose } else { $price }
            close = $price
            high = if ($null -ne $fallbackHigh -and $fallbackHigh -gt 0) { $fallbackHigh } else { $price }
            low = if ($null -ne $fallbackLow -and $fallbackLow -gt 0) { $fallbackLow } else { $price }
            volume = $null
            source = $source
        }
    }
    $lastTradingDate = Resolve-TradingDate (Get-ObjectPropertyValue -Value $historyResponse -Name "lastTradingDate")
    if ($null -eq $lastTradingDate -and $null -ne $script:SurfaceAction) {
        $lastTradingDate = Resolve-TradingDate (Get-ActionStateValue -Action $script:SurfaceAction -Name "lastTradingDate")
    }
    if ($null -eq $lastTradingDate) {
        $lastTradingDate = Resolve-TradingDate -Value $latest.date
    }
    if ($null -eq $lastTradingDate) {
        throw "stock-api 返回的最近交易日无效"
    }
    $bookAttempt = Try-Get-McpToolContent -Request $Request -CallId "orderbook"
    $bookResponse = Get-ObjectPropertyValue -Value (Get-ObjectPropertyValue -Value $bookAttempt -Name "content") -Name "response"
    $bookFetchedAt = Resolve-UtcTimestamp -Value (Get-ObjectPropertyValue -Value $bookResponse -Name "fetchedAt") -FallbackValue $fetchCompletedAt
    $orderBook = ConvertTo-OrderBook -Value (Get-ObjectPropertyValue -Value $bookResponse -Name "orderBook") -Code $code -FetchedAt $bookFetchedAt
    $liveTape = ConvertTo-LiveTape -Value (Get-ObjectPropertyValue -Value $bookResponse -Name "realtime") -Code $code -FetchedAt $bookFetchedAt
    # 盘口是可选增强：雪球对港股不返回十档，失败时沿用上一次快照而不是清空面板。
    if (($null -eq $orderBook -or $null -eq $liveTape) -and $null -ne $script:SurfaceAction) {
        $cachedBook = Get-ActionStateValue -Action $script:SurfaceAction -Name "orderBook"
        $cachedTape = Get-ActionStateValue -Action $script:SurfaceAction -Name "liveTape"
        if ($null -eq $orderBook -and [string](Get-ObjectPropertyValue -Value $cachedBook -Name "code") -eq $code) {
            $cachedOrderBook = ConvertTo-OrderBook -Value $cachedBook -Code $code
            if ($null -ne $cachedOrderBook -and -not (ConvertTo-StrictBoolean -Value (Get-ObjectPropertyValue -Value $cachedOrderBook -Name "stale" -DefaultValue $true) -DefaultValue $true)) {
                $orderBook = $cachedOrderBook
            }
        }
        if ($null -eq $liveTape -and [string](Get-ObjectPropertyValue -Value $cachedTape -Name "code") -eq $code) {
            $cachedLiveTape = ConvertTo-LiveTape -Value $cachedTape -Code $code
            if ($null -ne $cachedLiveTape -and -not (ConvertTo-StrictBoolean -Value (Get-ObjectPropertyValue -Value $cachedLiveTape -Name "stale" -DefaultValue $true) -DefaultValue $true)) {
                $liveTape = $cachedLiveTape
            }
        }
    }
    $liveTapeFresh = $null -ne $liveTape -and -not (ConvertTo-StrictBoolean -Value (Get-ObjectPropertyValue -Value $liveTape -Name "stale" -DefaultValue $true) -DefaultValue $true)
    $marketStatus = Get-MarketSessionState -Market $market -LastTradingDate $lastTradingDate
    if ($liveTapeFresh -and (ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $liveTape -Name "isTrade" -DefaultValue $false))) {
        $marketStatus = "open"
    }
    $usesLivePrice = $marketStatus -eq "open" -and $liveTapeFresh
    $displayPrice = if ($marketStatus -eq "closed") {
        $latest.close
    }
    elseif ($usesLivePrice) {
        $liveTape.price
    }
    else { $price }
    $effectiveSource = if ($usesLivePrice) { [string]$liveTape.source } else { $source }
    $effectiveObservedAt = if ($usesLivePrice) {
        [string]$liveTape.observedAt
    }
    elseif ($marketStatus -eq "closed") {
        Resolve-UtcTimestamp -Value ([string]$latest.date) -FallbackValue $quoteObservedAt
    }
    else { $quoteObservedAt }
    $effectiveFetchedAt = if ($usesLivePrice) { [string]$liveTape.fetchedAt } else { $quoteFetchedAt }
    $effectiveAgeSeconds = Get-ObservationAgeSeconds -Value $effectiveObservedAt
    # 年龄未知（上游时间戳缺失或没有时区偏移）时按过期处理。PowerShell 里 $null -gt 90 是
    # $false，少了这个判断，一份连观察时间都给不出的报价反而会被判成新鲜。休市时不强制判过期：
    # 收盘价本来就旧。
    $quoteStale = $quoteProviderStale -or ($marketStatus -eq "open" -and ($null -eq $effectiveAgeSeconds -or $effectiveAgeSeconds -gt $script:MaxLiveAgeSeconds))
    $effectivePreviousClose = if ($usesLivePrice -and $null -ne $liveTape.previousClose) { $liveTape.previousClose } else { $previousClose }
    $quote = [ordered]@{
        provider = "stock-api"
        providerVersion = $script:ProviderVersion
        upstreamVersion = $script:UpstreamVersion
        source = $effectiveSource
        sourceName = Get-ProviderName -Source $effectiveSource
        code = $code
        market = $market
        name = $name
        currency = Get-CurrencyForMarket -Market $market
        price = $displayPrice
        rawPrice = $price
        change = if ($null -ne $effectivePreviousClose) { [Math]::Round($displayPrice - $effectivePreviousClose, 4) } else { $change }
        changePercent = if ($null -ne $effectivePreviousClose -and $effectivePreviousClose -gt 0) { [Math]::Round((($displayPrice - $effectivePreviousClose) / $effectivePreviousClose) * 100, 4) } else { $changePercent }
        open = if ($usesLivePrice -and $null -ne $liveTape.open) { $liveTape.open } else { $latest.open }
        high = if ($usesLivePrice -and $null -ne $liveTape.high) { $liveTape.high } else { Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "high") }
        low = if ($usesLivePrice -and $null -ne $liveTape.low) { $liveTape.low } else { Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "low") }
        previousClose = $effectivePreviousClose
        observedAt = $effectiveObservedAt
        fetchedAt = $effectiveFetchedAt
        ageSeconds = $effectiveAgeSeconds
        maxAgeSeconds = $script:MaxLiveAgeSeconds
        stale = $quoteStale
        marketStatus = $marketStatus
        lastTradingDate = $lastTradingDate
    }
    $favoritesAttempt = Try-Get-McpToolContent -Request $Request -CallId "favorites"
    $favoritesResponse = Get-ObjectPropertyValue -Value (Get-ObjectPropertyValue -Value $favoritesAttempt -Name "content") -Name "response"
    $favoriteQuotes = @(ConvertTo-FavoriteQuotes (Get-ObjectPropertyValue -Value $favoritesResponse -Name "stocks" -DefaultValue @()))
    if ($favoriteQuotes.Count -eq 0 -and $null -ne $script:SurfaceAction) {
        $favoriteQuotes = @(ConvertTo-FavoriteQuotes (Get-ActionStateValue -Action $script:SurfaceAction -Name "favoriteQuotes" -DefaultValue @()))
    }
    return [ordered]@{
        quote = $quote
        history = $history
        latestKline = $latest
        period = $period
        lastTradingDate = $lastTradingDate
        marketStatus = $marketStatus
        orderBook = $orderBook
        liveTape = $liveTape
        favoriteQuotes = $favoriteQuotes
        historyError = if ($history.Count -eq 0) { $historyError } else { $null }
    }
}
