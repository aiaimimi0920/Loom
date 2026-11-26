$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# The JSON ABI is UTF-8 regardless of the Windows console/OEM code page.
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

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
            # Do not throw with an interpolated $actionId: the catch block would project caller text
            # into state.error and the quote_change node. Emit a fixed message with -RejectAction so
            # neither the symbol nor correlation fields read from the rejected invocation.
            Write-SurfaceErrorState -Action $script:SurfaceAction -Message "Action is not declared in the manifest and was rejected" -RejectAction
            exit 0
        }
        if ($actionId -eq "stock_interval_commit") {
            $interval = Resolve-RefreshInterval (Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue 5)
            $statePatch = [ordered]@{
                intervalSeconds = $interval
                statusText = "Refresh every ${interval}s"
                error = $null
            }
            [void](Add-ActionEcho -StatePatch $statePatch -Action $script:SurfaceAction)
            Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
                operations = [object[]]@(
                    (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
                    (New-SetOperation -NodeId "status" -Path "/props/text" -Value "Refresh every ${interval}s")
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
        throw "Stock code returned by stock-api does not match requested code"
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
        throw "Invalid market period: $requestedPeriodText"
    }
    if ((Resolve-MarketPeriod $requestedPeriodText) -ne $period) {
        throw "Action period does not match requested period"
    }
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $script:SurfaceAction -Name "intervalSeconds" -DefaultValue 5)
    $statusText = if ($history.Count -eq 0) {
        "Quote updated · Quote curve will refresh on the next cycle"
    }
    elseif ([string]$snapshot.marketStatus -eq "closed") {
        "Closed · Last trading day $($snapshot.lastTradingDate)"
    }
    elseif ($actionId -eq "stock_tick_refresh") {
        if ($null -ne $snapshot.orderBook) {
            "Near-realtime · $periodLabel · Every ${interval}s · $($snapshot.orderBook.levels)-level book"
        }
        else {
            "Near-realtime · $periodLabel · Every ${interval}s"
        }
    }
    else {
        "Trading · $periodLabel"
    }
    $statePatch = [ordered]@{
        schemaVersion = 2
        provider = [ordered]@{ id = "stock-api"; name = "stock-api"; source = [string]$quote.source }
        marketScope = "A-Share / HK / US"
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
        # Forward upstream K-line failures as a non-fatal warning when the quote itself succeeded so
        # a chart-less panel can explain why while keeping status=ready.
        historyWarning = if ([string]::IsNullOrWhiteSpace([string]$snapshot.historyError)) { $null } else { Limit-MessageLength -Message ([string]$snapshot.historyError) }
        disclaimer = $script:Disclaimer
    }
    [void](Add-ActionEcho -StatePatch $statePatch -Action $script:SurfaceAction)
    $metricsText = "Open $(Format-Price $quote.open)  High $(Format-Price $quote.high)  Low $(Format-Price $quote.low)  Prev $(Format-Price $quote.previousClose)"
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
        # Carry the real statePatch once in patches[0]. Both apply_patch and commit_result call
        # merge_json in surface_store.rs, so repeating it here would serialize and store up to 2,000
        # K-line rows twice. Keep an empty object rather than omitting statePatch: serde defaults an
        # omitted field to Value::Null, which merge_json treats as a full-state replacement.
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
