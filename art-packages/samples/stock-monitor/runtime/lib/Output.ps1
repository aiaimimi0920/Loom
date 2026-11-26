# Owns runtime envelopes, Surface patches, error projection, and formal quote output.

function Format-Price {
    param([AllowNull()][object]$Value)
    if ($null -eq $Value) { return "--" }
    return ([double]$Value).ToString("0.00", [System.Globalization.CultureInfo]::InvariantCulture)
}

function Format-SignedNumber {
    param(
        [AllowNull()][object]$Value,
        [string]$Suffix = ""
    )
    if ($null -eq $Value) { return "--" }
    $number = [double]$Value
    $text = if ($number -gt 0) {
        "+" + $number.ToString("0.00", [System.Globalization.CultureInfo]::InvariantCulture)
    }
    else {
        $number.ToString("0.00", [System.Globalization.CultureInfo]::InvariantCulture)
    }
    return $text + $Suffix
}

function New-SetOperation {
    param(
        [string]$NodeId,
        [string]$Path,
        [AllowNull()][object]$Value
    )
    return [ordered]@{ op = "set"; nodeId = $NodeId; path = $Path; value = $Value }
}

function Write-SurfaceResponse {
    param(
        [Parameter(Mandatory = $true)][object[]]$Patches,
        [AllowNull()][object]$Result = $null
    )

    $surfaceAction = [ordered]@{
        protocolVersion = "loom.surface.v1"
        patches = $Patches
    }
    if ($null -ne $Result) { $surfaceAction.result = $Result }
    [Console]::Out.Write(([ordered]@{
        status = "success"
        output = [ordered]@{ surfaceAction = $surfaceAction }
    } | ConvertTo-Json -Depth 100 -Compress))
}

function Write-RuntimeSuccess {
    param([object]$FormalQuote)
    [Console]::Out.Write(([ordered]@{
        status = "success"
        output = [ordered]@{ quote = $FormalQuote }
    } | ConvertTo-Json -Depth 100 -Compress))
}

function Limit-MessageLength {
    # Truncate on Unicode text-element boundaries rather than UTF-16 code units. Substring(0, 400)
    # can split a surrogate pair and leave a lone high surrogate that ConvertTo-Json serializes as
    # an unpaired \uD83D escape, corrupting the string when decoded by the host.
    param(
        [AllowNull()][string]$Message,
        [int]$MaxLength = 400
    )

    if ([string]::IsNullOrEmpty($Message)) { return $Message }
    if ($Message.Length -le $MaxLength) { return $Message }
    $builder = [System.Text.StringBuilder]::new()
    $enumerator = [System.Globalization.StringInfo]::GetTextElementEnumerator($Message)
    while ($enumerator.MoveNext()) {
        $element = [string]$enumerator.Current
        if (($builder.Length + $element.Length) -gt $MaxLength) { break }
        [void]$builder.Append($element)
    }
    return $builder.ToString()
}

function Write-RuntimeError {
    param([string]$Message)
    [Console]::Out.Write(([ordered]@{
        status = "error"
        # Bound non-Surface error payloads so oversized upstream diagnostics cannot bloat stdout.
        error = [ordered]@{ code = "stock_monitor_failed"; message = (Limit-MessageLength -Message $Message) }
    } | ConvertTo-Json -Depth 20 -Compress))
}

function Write-SurfaceErrorState {
    # -RejectAction: actionId is undeclared. Treat every field on the action as untrusted: ignore
    # its payload, omit actionId/requestId from correlation echoes, and emit a fixed error string so
    # caller-controlled text cannot enter rendered nodes or persisted state.
    param(
        [object]$Action,
        [string]$Message,
        [switch]$RejectAction
    )

    $Message = Limit-MessageLength -Message $Message
    $rawCode = if ((-not $RejectAction) -and [string](Get-ObjectPropertyValue -Value $Action -Name "actionId") -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $Action -Name "value" -DefaultValue "SZ000034"
    }
    else {
        Get-ActionStateValue -Action $Action -Name "code" -DefaultValue "SZ000034"
    }
    try { $code = Resolve-StockCode $rawCode }
    catch {
        try { $code = Resolve-StockCode (Get-ActionStateValue -Action $Action -Name "code" -DefaultValue "SZ000034") }
        catch { $code = "SZ000034" }
    }
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $Action -Name "intervalSeconds" -DefaultValue 60)
    $period = Resolve-MarketPeriod (Get-ActionStateValue -Action $Action -Name "period" -DefaultValue "day")
    $periodLabel = Get-MarketPeriodLabel -Period $period
    $marketStatus = [string](Get-ActionStateValue -Action $Action -Name "marketStatus" -DefaultValue "closed")
    $lastTradingDate = Resolve-TradingDate (Get-ActionStateValue -Action $Action -Name "lastTradingDate")
    $statePatch = [ordered]@{
        schemaVersion = 2
        provider = [ordered]@{ id = "stock-api"; name = "stock-api" }
        marketScope = "A-Share / HK / US"
        code = $code
        intervalSeconds = $interval
        period = $period
        periodLabel = $periodLabel
        marketStatus = $marketStatus
        lastTradingDate = $lastTradingDate
        status = "error"
        statusText = "Failed to fetch quote"
        error = $Message
        disclaimer = $script:Disclaimer
    }
    # Rejected actions omit correlation echoes so the client falls back to revision-based unlock.
    $echoAction = if ($RejectAction) { $null } else { $Action }
    [void](Add-ActionEcho -StatePatch $statePatch -Action $echoAction)
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = [object[]]@(
            (New-SetOperation -NodeId "status" -Path "/props/text" -Value "Failed to fetch quote"),
            (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value $Message),
            (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $code),
            (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
            (New-SetOperation -NodeId "period" -Path "/props/value" -Value $period)
        )
        statePatch = $statePatch
    }))
}

function New-FormalQuote {
    # -ReferenceState: used on the Surface path where history/orderBook/liveTape/favoriteQuotes are
    # already carried in statePatch of the same response. Referencing authoritativeState avoids
    # serializing up to 2,000 K-line rows a second time across stdout, host storage, and client push.
    # Non-Surface execution (Write-RuntimeSuccess) still embeds the full arrays per the quote port contract.
    param(
        [object]$Snapshot,
        [switch]$ReferenceState
    )
    $quote = $Snapshot.quote
    $history = [ordered]@{
        period = [string]$Snapshot.period
        adjust = "none"
    }
    if ($ReferenceState) {
        $history.rowCount = @($Snapshot.history).Count
        $history.rowsIn = "authoritativeState.history"
    }
    else {
        $history.rows = @($Snapshot.history)
    }
    $formal = [ordered]@{
        provider = "stock-api"
        providerVersion = $script:ProviderVersion
        upstreamVersion = $script:UpstreamVersion
        source = [string]$quote.source
        sourceName = [string]$quote.sourceName
        code = [string]$quote.code
        market = [string]$quote.market
        name = [string]$quote.name
        currency = [string]$quote.currency
        price = $quote.price
        change = $quote.change
        changePercent = $quote.changePercent
        observedAt = [string]$quote.observedAt
        fetchedAt = [string]$quote.fetchedAt
        ageSeconds = $quote.ageSeconds
        maxAgeSeconds = $quote.maxAgeSeconds
        stale = ConvertTo-StrictBoolean $quote.stale
        metrics = [ordered]@{
            open = $quote.open
            high = $quote.high
            low = $quote.low
            previousClose = $quote.previousClose
        }
        marketStatus = [string]$Snapshot.marketStatus
        lastTradingDate = [string]$Snapshot.lastTradingDate
        history = $history
    }
    if ($ReferenceState) {
        $formal.collectionsIn = "authoritativeState"
        $formal.orderBookLevels = [int](Get-ObjectPropertyValue -Value $Snapshot.orderBook -Name "levels" -DefaultValue 0)
        $formal.liveTapeObservedAt = [string](Get-ObjectPropertyValue -Value $Snapshot.liveTape -Name "observedAt")
        $formal.favoriteQuoteCount = @($Snapshot.favoriteQuotes).Count
    }
    else {
        $formal.orderBook = $Snapshot.orderBook
        $formal.liveTape = $Snapshot.liveTape
        $formal.favoriteQuotes = @($Snapshot.favoriteQuotes)
    }
    $formal.disclaimer = $script:Disclaimer
    return $formal
}
