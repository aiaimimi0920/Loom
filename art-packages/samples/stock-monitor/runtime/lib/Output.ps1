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
    # 按 Unicode 文本元素截断，不按 UTF-16 码元。Substring(0, 400) 可能正好切在代理对中间，
    # 留下一个孤立高位代理；那不是合法 Unicode，ConvertTo-Json 会把它写成 \ud83d 这类无配对
    # 转义，宿主再解析就得到损坏字符串。上游错误消息是原样回显的（含 emoji），所以这条路径
    # 真的会被走到。
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
        # 非 Surface 路径此前完全不截断，一条上游长消息能把整份错误响应撑到任意大小。
        error = [ordered]@{ code = "stock_monitor_failed"; message = (Limit-MessageLength -Message $Message) }
    } | ConvertTo-Json -Depth 20 -Compress))
}

function Write-SurfaceErrorState {
    # -RejectAction：动作 id 不在声明列表里。此时这个动作的任何字段都不可信，既不读它的
    # payload，也不把它的 actionId/requestId 回显进状态；错误文案用固定常量，避免调用方
    # 控制的文本进入渲染节点与持久状态。
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
        marketScope = "A 股 / 港股 / 美股"
        code = $code
        intervalSeconds = $interval
        period = $period
        periodLabel = $periodLabel
        marketStatus = $marketStatus
        lastTradingDate = $lastTradingDate
        status = "error"
        statusText = "行情获取失败"
        error = $Message
        disclaimer = $script:Disclaimer
    }
    # 被拒动作不参与关联回显：客户端等不到自己的 requestId，会退回只看 revision 的解锁分支。
    $echoAction = if ($RejectAction) { $null } else { $Action }
    [void](Add-ActionEcho -StatePatch $statePatch -Action $echoAction)
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = [object[]]@(
            (New-SetOperation -NodeId "status" -Path "/props/text" -Value "行情获取失败"),
            (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value $Message),
            (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $code),
            (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
            (New-SetOperation -NodeId "period" -Path "/props/value" -Value $period)
        )
        statePatch = $statePatch
    }))
}

function New-FormalQuote {
    # -ReferenceState：Surface 路径用。此时 history/orderBook/liveTape/favoriteQuotes 已经在
    # 同一份响应的 statePatch 里，Surface 也只从 authoritativeState 读它们，再在 result.outputs
    # 里重复一遍就是把 2000 行 K 线第二次写上 stdout、第二次经过宿主存储、第二次推给客户端。
    # 这里只留计数与指向状态的引用标记。非 Surface 路径（Write-RuntimeSuccess）没有状态可引用，
    # 仍然带完整数组，那是这个 art 的 quote 输出端口契约。
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
