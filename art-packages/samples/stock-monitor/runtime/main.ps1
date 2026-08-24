$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Keep package-root resolution anchored to the public entry. Dot-sourced modules have their own
# $PSScriptRoot value, which points at runtime/lib rather than the runtime directory.
$script:StockMonitorRuntimeRoot = $PSScriptRoot
$moduleRoot = Join-Path $script:StockMonitorRuntimeRoot "lib"
$moduleNames = @(
    "Constants.ps1",
    "Protocol.ps1",
    "Domain.ps1",
    "Mcp.ps1",
    "Transforms.ps1",
    "Snapshot.ps1",
    "Output.ps1"
)
foreach ($moduleName in $moduleNames) {
    . (Join-Path $moduleRoot $moduleName)
}

try {
    $requestText = Read-BoundedStandardInput
    if ([string]::IsNullOrWhiteSpace($requestText)) { throw "Stock Monitor request is required" }
    Assert-JsonTextDepth -Value $requestText
    $request = $requestText | ConvertFrom-Json
    if (-not (($request -is [System.Collections.IDictionary]) -or ($request -is [pscustomobject]))) {
        throw "Stock Monitor request must be a JSON object"
    }
    Assert-RequestObjectGraph -Value $request
    $script:SurfaceAction = Resolve-SurfaceAction -Value $request

    if ($null -ne $script:SurfaceAction) {
        $actionId = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -MaxLength 64 -DefaultValue ""
        if ($actionId -notin @("stock_refresh", "stock_symbol_commit", "stock_interval_commit", "stock_period_commit", "stock_tick_refresh")) {
            # 不能 throw：throw 的消息会插值调用方给的 $actionId，catch 再把它写进 error 状态与
            # quote_change 显示节点，等于让未声明的动作把任意文本送上界面。这里改成固定文案，
            # 并用 -RejectAction 让符号/关联字段都不取自这个动作。
            Write-SurfaceErrorState -Action $script:SurfaceAction -Message "行情动作未被声明，已拒绝执行" -RejectAction
            exit 0
        }
        if ($actionId -eq "stock_interval_commit") {
            $interval = Resolve-RefreshInterval (Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue 5)
            $statePatch = [ordered]@{
                intervalSeconds = $interval
                statusText = "每 $interval 秒刷新"
                error = $null
            }
            [void](Add-ActionEcho -StatePatch $statePatch -Action $script:SurfaceAction)
            Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
                operations = [object[]]@(
                    (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
                    (New-SetOperation -NodeId "status" -Path "/props/text" -Value "每 $interval 秒刷新")
                )
                statePatch = $statePatch
            }))
            exit 0
        }
    }

    $snapshot = Get-StockSnapshot -Request $request
    $quote = $snapshot.quote
    $history = @($snapshot.history)
    $period = [string]$snapshot.period
    $periodLabel = Get-MarketPeriodLabel -Period $period
    if ($null -eq $script:SurfaceAction) {
        Write-RuntimeSuccess -FormalQuote (New-FormalQuote -Snapshot $snapshot)
        exit 0
    }
    $formalQuote = New-FormalQuote -Snapshot $snapshot -ReferenceState

    $requestedCode = if ((ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -MaxLength 64 -DefaultValue "") -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $quote.code
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "code" -DefaultValue $quote.code
    }
    if ((Resolve-StockCode $requestedCode) -ne [string]$quote.code) {
        throw "stock-api 返回的股票代码与请求不一致"
    }
    $actionId = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -MaxLength 64 -DefaultValue ""
    $requestedPeriodValue = if ($actionId -eq "stock_period_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $period
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "period" -DefaultValue $period
    }
    $requestedPeriodText = (ConvertTo-BoundedText -Value $requestedPeriodValue -MaxLength 32 -DefaultValue "").ToLowerInvariant()
    if ($requestedPeriodText -notin $script:AllowedPeriods) {
        throw "行情周期无效：$requestedPeriodText"
    }
    if ((Resolve-MarketPeriod $requestedPeriodText) -ne $period) {
        throw "stock-api 返回的行情周期与请求不一致"
    }
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $script:SurfaceAction -Name "intervalSeconds" -DefaultValue 5)
    $statusText = if ($history.Count -eq 0) {
        "报价已更新 · 曲线将在下次刷新补齐"
    }
    elseif ([string]$snapshot.marketStatus -eq "closed") {
        "休市 · 最近交易日 $($snapshot.lastTradingDate)"
    }
    elseif ($actionId -eq "stock_tick_refresh") {
        if ($null -ne $snapshot.orderBook) {
            "准实时 · $periodLabel · 每 $interval 秒 · 盘口 $($snapshot.orderBook.levels) 档"
        }
        else {
            "准实时 · $periodLabel · 每 $interval 秒"
        }
    }
    else {
        "交易中 · $periodLabel"
    }
    $statePatch = [ordered]@{
        schemaVersion = 2
        provider = [ordered]@{ id = "stock-api"; name = "stock-api"; source = [string]$quote.source }
        marketScope = "A 股 / 港股 / 美股"
        code = [string]$quote.code
        market = [string]$quote.market
        intervalSeconds = $interval
        period = $period
        periodLabel = $periodLabel
        marketStatus = [string]$snapshot.marketStatus
        lastTradingDate = [string]$snapshot.lastTradingDate
        status = "ready"
        statusText = $statusText
        quote = $quote
        history = $history
        orderBook = $snapshot.orderBook
        liveTape = $snapshot.liveTape
        favoriteQuotes = @($snapshot.favoriteQuotes)
        lastUpdatedAt = [string]$quote.fetchedAt
        error = $null
        # K 线抓取失败但报价成功时，此前这条上游错误只留在快照里就被丢掉：面板画不出曲线，
        # 也说不出为什么。作为非致命告警送到 Surface，状态本身仍是 ready。
        historyWarning = if ([string]::IsNullOrWhiteSpace([string]$snapshot.historyError)) { $null } else { Limit-MessageLength -Message ([string]$snapshot.historyError) }
        disclaimer = $script:Disclaimer
    }
    [void](Add-ActionEcho -StatePatch $statePatch -Action $script:SurfaceAction)
    $metricsText = "开 $(Format-Price $quote.open)  高 $(Format-Price $quote.high)  低 $(Format-Price $quote.low)  昨收 $(Format-Price $quote.previousClose)"
    $operations = [object[]]@(
        (New-SetOperation -NodeId "status" -Path "/props/text" -Value $statusText),
        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value ([string]$quote.code)),
        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
        (New-SetOperation -NodeId "period" -Path "/props/value" -Value $period),
        (New-SetOperation -NodeId "quote_name" -Path "/props/text" -Value "$($quote.name) $($quote.code) · $($quote.market)"),
        (New-SetOperation -NodeId "quote_price" -Path "/props/text" -Value (Format-Price $quote.price)),
        (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value "$(Format-SignedNumber $quote.change)  $(Format-SignedNumber $quote.changePercent '%')"),
        (New-SetOperation -NodeId "quote_metrics" -Path "/props/text" -Value $metricsText)
    )
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = $operations
        statePatch = $statePatch
    })) -Result ([ordered]@{
        # 状态补丁只在 patches 里带一次。宿主两侧都会 merge（surface_store.rs 的 apply_patch 与
        # commit_result 都走 merge_json），第二份纯属重复传输：2000 行 K 线会第二次写上 stdout、
        # 第二次进存储、第二次推给客户端。
        # 这里给空对象而不是省略字段：state_patch 是 #[serde(default)]，省略后是 Value::Null，
        # 而 merge_json 对非对象补丁执行整体替换（surface_store.rs:1415），会把整份权威状态置空。
        outputs = [ordered]@{
            quote = [ordered]@{ kind = "value"; value = $formalQuote }
        }
        statePatch = [ordered]@{}
    })
}
catch {
    if ($null -ne $script:SurfaceAction) {
        Write-SurfaceErrorState -Action $script:SurfaceAction -Message $_.Exception.Message
    }
    else {
        Write-RuntimeError -Message $_.Exception.Message
    }
}
