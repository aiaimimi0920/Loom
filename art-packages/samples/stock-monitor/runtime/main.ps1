$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:SurfaceAction = $null
$script:AllowedIntervals = @(30, 60, 120, 300)
$script:Disclaimer = "行情可能延迟，仅用于信息展示，不构成投资建议或交易指令"

function Get-ObjectPropertyValue {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string]$Name,
        [AllowNull()][object]$DefaultValue = $null
    )

    if ($null -eq $Value) { return $DefaultValue }
    if ($Value -is [System.Collections.IDictionary]) {
        if ($Value.Contains($Name)) { return $Value[$Name] }
        return $DefaultValue
    }
    $property = $Value.PSObject.Properties[$Name]
    if ($null -ne $property) { return $property.Value }
    return $DefaultValue
}

function Find-SurfaceAction {
    param([AllowNull()][object]$Value)

    if ($null -eq $Value) { return $null }
    if ($Value -is [System.Collections.IDictionary]) {
        if ($Value.Contains("surfaceAction")) { return $Value["surfaceAction"] }
        foreach ($item in $Value.Values) {
            $found = Find-SurfaceAction -Value $item
            if ($null -ne $found) { return $found }
        }
        return $null
    }
    if ($Value -is [pscustomobject]) {
        $surfaceActionProperty = $Value.PSObject.Properties["surfaceAction"]
        if ($null -ne $surfaceActionProperty) { return $surfaceActionProperty.Value }
        foreach ($property in $Value.PSObject.Properties) {
            $found = Find-SurfaceAction -Value $property.Value
            if ($null -ne $found) { return $found }
        }
        return $null
    }
    if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
        foreach ($item in $Value) {
            $found = Find-SurfaceAction -Value $item
            if ($null -ne $found) { return $found }
        }
    }
    return $null
}

function Get-ActionPayloadValue {
    param(
        [object]$Action,
        [string]$Name,
        [AllowNull()][object]$DefaultValue = $null
    )

    $payload = Get-ObjectPropertyValue -Value $Action -Name "payload"
    return Get-ObjectPropertyValue -Value $payload -Name $Name -DefaultValue $DefaultValue
}

function Get-ActionStateValue {
    param(
        [object]$Action,
        [string]$Name,
        [AllowNull()][object]$DefaultValue = $null
    )

    $state = Get-ObjectPropertyValue -Value $Action -Name "authoritativeState"
    return Get-ObjectPropertyValue -Value $state -Name $Name -DefaultValue $DefaultValue
}

function Get-RequestValue {
    param(
        [object]$Request,
        [string]$Name,
        [AllowNull()][object]$DefaultValue = $null
    )

    foreach ($containerName in @("params", "inputs")) {
        $container = Get-ObjectPropertyValue -Value $Request -Name $containerName
        $value = Get-ObjectPropertyValue -Value $container -Name $Name
        if ($null -ne $value) { return $value }
    }
    return $DefaultValue
}

function Convert-NullableNumber {
    param(
        [AllowNull()][object]$Value,
        [int]$Digits = 4
    )

    if ($null -eq $Value) { return $null }
    $number = 0.0
    $style = [System.Globalization.NumberStyles]::Float
    $culture = [System.Globalization.CultureInfo]::InvariantCulture
    if (-not [double]::TryParse(([string]$Value).Trim(), $style, $culture, [ref]$number)) {
        return $null
    }
    if ([double]::IsNaN($number) -or [double]::IsInfinity($number)) { return $null }
    return [Math]::Round($number, $Digits)
}

function Resolve-StockCode {
    param([AllowNull()][object]$Value)

    $input = ([string]$Value).Trim().ToUpperInvariant().Replace(" ", "")
    if ([string]::IsNullOrWhiteSpace($input)) {
        throw "请输入股票代码，例如 SZ000034、SH600519、HK00700 或 USAAPL"
    }
    if ($input -match '^(SH|SZ)[:._-]?(\d{6})$') {
        return "$($Matches[1])$($Matches[2])"
    }
    if ($input -match '^(\d{6})[:._-]?(SH|SZ)$') {
        return "$($Matches[2])$($Matches[1])"
    }
    if ($input -match '^(\d{6})$') {
        $market = if ($input.StartsWith("5") -or $input.StartsWith("6") -or $input.StartsWith("9")) { "SH" } else { "SZ" }
        return "$market$input"
    }
    if ($input -match '^HK[:._-]?(\d{1,5})$') {
        return "HK$($Matches[1].PadLeft(5, '0'))"
    }
    if ($input -match '^US[:_-]?([A-Z][A-Z0-9.-]{0,19})$') {
        return "US$($Matches[1])"
    }
    throw "股票代码格式无效；支持 SZ000034、SH600519、HK00700 和 USAAPL 等统一代码"
}

function Get-MarketFromCode {
    param([string]$Code)
    return $Code.Substring(0, 2)
}

function Get-CurrencyForMarket {
    param([string]$Market)
    switch ($Market) {
        "HK" { return "HKD" }
        "US" { return "USD" }
        default { return "CNY" }
    }
}

function Get-ProviderName {
    param([string]$Source)
    switch ($Source.ToLowerInvariant()) {
        "tencent" { return "腾讯行情" }
        "sina" { return "新浪财经" }
        "eastmoney" { return "东方财富" }
        default { return $Source }
    }
}

function Resolve-RefreshInterval {
    param([AllowNull()][object]$Value)

    $parsed = 60
    if (-not [int]::TryParse([string]$Value, [ref]$parsed)) { $parsed = 60 }
    if ($parsed -notin $script:AllowedIntervals) { $parsed = 60 }
    return $parsed
}

function Get-McpToolContent {
    param(
        [object]$Request,
        [string]$CallId
    )

    $frameworkData = Get-ObjectPropertyValue -Value $Request -Name "frameworkData"
    $mcp = Get-ObjectPropertyValue -Value $frameworkData -Name "mcp"
    $results = Get-ObjectPropertyValue -Value $mcp -Name "results"
    $execution = Get-ObjectPropertyValue -Value $results -Name $CallId
    $result = Get-ObjectPropertyValue -Value $execution -Name "result"
    if ($null -eq $result) {
        throw "stock-api MCP 调用结果缺失：$CallId"
    }
    $structured = Get-ObjectPropertyValue -Value $result -Name "structuredContent"
    if ([bool](Get-ObjectPropertyValue -Value $result -Name "isError" -DefaultValue $false)) {
        $response = Get-ObjectPropertyValue -Value $structured -Name "response"
        $message = [string](Get-ObjectPropertyValue -Value $response -Name "message" -DefaultValue "未知错误")
        throw "stock-api MCP 调用失败（$CallId）：$message"
    }
    if ($null -eq $structured) {
        throw "stock-api MCP 返回的结构化结果缺失：$CallId"
    }
    return $structured
}

function ConvertTo-HistoryRows {
    param([AllowNull()][object]$Values)

    $rows = @()
    foreach ($value in @($Values | Select-Object -Last 120)) {
        $date = ([string](Get-ObjectPropertyValue -Value $value -Name "date")).Trim()
        $open = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "open")
        $close = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "close")
        $high = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "high")
        $low = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "low")
        if ([string]::IsNullOrWhiteSpace($date) -or $null -eq $open -or $null -eq $close -or $null -eq $high -or $null -eq $low) {
            continue
        }
        if ($open -le 0 -or $close -le 0 -or $high -le 0 -or $low -le 0 -or $high -lt $low) {
            continue
        }
        $source = ([string](Get-ObjectPropertyValue -Value $value -Name "source")).Trim().ToLowerInvariant()
        $rows += [ordered]@{
            date = $date
            open = $open
            close = $close
            high = $high
            low = $low
            volume = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "volume") -Digits 0
            source = $source
        }
    }
    return @($rows)
}

function Get-StockSnapshot {
    param([object]$Request)

    $quoteContent = Get-McpToolContent -Request $Request -CallId "quote"
    $historyContent = Get-McpToolContent -Request $Request -CallId "history"
    $quoteResponse = Get-ObjectPropertyValue -Value $quoteContent -Name "response"
    $historyResponse = Get-ObjectPropertyValue -Value $historyContent -Name "response"
    $stock = Get-ObjectPropertyValue -Value $quoteResponse -Name "stock"
    if ($null -eq $stock) { throw "stock-api 未返回股票报价" }

    $code = Resolve-StockCode (Get-ObjectPropertyValue -Value $stock -Name "code")
    $name = ([string](Get-ObjectPropertyValue -Value $stock -Name "name")).Trim()
    $price = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "now")
    if ([string]::IsNullOrWhiteSpace($name) -or $name -eq "---" -or $null -eq $price -or $price -le 0) {
        throw "stock-api 未找到该代码的有效报价"
    }

    $history = @(ConvertTo-HistoryRows (Get-ObjectPropertyValue -Value $historyResponse -Name "klines" -DefaultValue @()))
    if ($history.Count -eq 0) { throw "stock-api 未返回可视化所需的日 K 线" }
    $source = ([string](Get-ObjectPropertyValue -Value $stock -Name "source")).Trim().ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($source)) {
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
    $latest = $history[-1]
    $observedAt = [DateTimeOffset]::UtcNow.ToString("o")
    $quote = [ordered]@{
        provider = "stock-api"
        providerVersion = "2.7.3"
        source = $source
        sourceName = Get-ProviderName -Source $source
        code = $code
        market = $market
        name = $name
        currency = Get-CurrencyForMarket -Market $market
        price = $price
        change = $change
        changePercent = $changePercent
        open = $latest.open
        high = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "high")
        low = Convert-NullableNumber (Get-ObjectPropertyValue -Value $stock -Name "low")
        previousClose = $previousClose
        observedAt = $observedAt
    }
    return [ordered]@{ quote = $quote; history = $history; latestKline = $latest }
}

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

function Write-RuntimeError {
    param([string]$Message)
    [Console]::Out.Write(([ordered]@{
        status = "error"
        error = [ordered]@{ code = "stock_monitor_failed"; message = $Message }
    } | ConvertTo-Json -Depth 20 -Compress))
}

function Write-SurfaceErrorState {
    param(
        [object]$Action,
        [string]$Message
    )

    if ($Message.Length -gt 400) { $Message = $Message.Substring(0, 400) }
    $rawCode = if ([string](Get-ObjectPropertyValue -Value $Action -Name "actionId") -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $Action -Name "value" -DefaultValue "SZ000034"
    }
    else {
        Get-ActionStateValue -Action $Action -Name "code" -DefaultValue "SZ000034"
    }
    try { $code = Resolve-StockCode $rawCode } catch { $code = ([string]$rawCode).Trim() }
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $Action -Name "intervalSeconds" -DefaultValue 60)
    $statePatch = [ordered]@{
        schemaVersion = 2
        provider = [ordered]@{ id = "stock-api"; name = "stock-api" }
        marketScope = "A 股 / 港股 / 美股"
        code = $code
        intervalSeconds = $interval
        status = "error"
        statusText = "行情获取失败"
        error = $Message
        disclaimer = $script:Disclaimer
    }
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = [object[]]@(
            (New-SetOperation -NodeId "status" -Path "/props/text" -Value "行情获取失败"),
            (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value $Message),
            (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $code),
            (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval))
        )
        statePatch = $statePatch
    }))
}

function New-FormalQuote {
    param([object]$Snapshot)
    $quote = $Snapshot.quote
    return [ordered]@{
        provider = "stock-api"
        providerVersion = "2.7.3"
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
        metrics = [ordered]@{
            open = $quote.open
            high = $quote.high
            low = $quote.low
            previousClose = $quote.previousClose
        }
        history = [ordered]@{
            period = "day"
            adjust = "none"
            rows = @($Snapshot.history)
        }
        disclaimer = $script:Disclaimer
    }
}

try {
    $requestText = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($requestText)) { throw "Stock Monitor request is required" }
    $request = $requestText | ConvertFrom-Json
    $script:SurfaceAction = Find-SurfaceAction -Value $request

    if ($null -ne $script:SurfaceAction) {
        $actionId = [string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId")
        if ($actionId -notin @("stock_refresh", "stock_symbol_commit", "stock_interval_commit")) {
            throw "action is not declared by the stock monitor: $actionId"
        }
        if ($actionId -eq "stock_interval_commit") {
            $interval = Resolve-RefreshInterval (Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue 60)
            $statePatch = [ordered]@{
                intervalSeconds = $interval
                statusText = "每 $interval 秒刷新"
                error = $null
            }
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
    $formalQuote = New-FormalQuote -Snapshot $snapshot
    if ($null -eq $script:SurfaceAction) {
        Write-RuntimeSuccess -FormalQuote $formalQuote
        exit 0
    }

    $requestedCode = if ([string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $quote.code
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "code" -DefaultValue $quote.code
    }
    if ((Resolve-StockCode $requestedCode) -ne [string]$quote.code) {
        throw "stock-api 返回的股票代码与请求不一致"
    }
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $script:SurfaceAction -Name "intervalSeconds" -DefaultValue 60)
    $statusText = "stock-api / $($quote.source) · 行情已更新"
    $statePatch = [ordered]@{
        schemaVersion = 2
        provider = [ordered]@{ id = "stock-api"; name = "stock-api"; source = [string]$quote.source }
        marketScope = "A 股 / 港股 / 美股"
        code = [string]$quote.code
        market = [string]$quote.market
        intervalSeconds = $interval
        status = "ready"
        statusText = $statusText
        quote = $quote
        history = $history
        lastUpdatedAt = [string]$quote.observedAt
        error = $null
        disclaimer = $script:Disclaimer
    }
    $metricsText = "开 $(Format-Price $quote.open)  高 $(Format-Price $quote.high)  低 $(Format-Price $quote.low)  昨收 $(Format-Price $quote.previousClose)"
    $operations = [object[]]@(
        (New-SetOperation -NodeId "status" -Path "/props/text" -Value $statusText),
        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value ([string]$quote.code)),
        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
        (New-SetOperation -NodeId "quote_name" -Path "/props/text" -Value "$($quote.name) $($quote.code) · $($quote.market)"),
        (New-SetOperation -NodeId "quote_price" -Path "/props/text" -Value (Format-Price $quote.price)),
        (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value "$(Format-SignedNumber $quote.change)  $(Format-SignedNumber $quote.changePercent '%')"),
        (New-SetOperation -NodeId "quote_metrics" -Path "/props/text" -Value $metricsText)
    )
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = $operations
        statePatch = $statePatch
    })) -Result ([ordered]@{
        outputs = [ordered]@{
            quote = [ordered]@{ kind = "value"; value = $formalQuote }
        }
        statePatch = $statePatch
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
