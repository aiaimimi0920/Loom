$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:SurfaceAction = $null
$script:AllowedIntervals = @(5, 15, 30, 60)
$script:QuoteFields = "f57,f58,f43,f44,f45,f46,f47,f48,f49,f50,f51,f52,f60,f116,f117,f162,f167,f168,f169,f170,f171"
$script:TrendFields1 = "f1,f2,f3,f4,f5,f6,f7,f8"
$script:TrendFields2 = "f51,f52,f53,f54,f55,f56,f57,f58"

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

function Convert-ScaledNumber {
    param(
        [AllowNull()][object]$Value,
        [double]$Scale = 1.0,
        [int]$Digits = 4
    )

    if ($null -eq $Value) { return $null }
    $text = ([string]$Value).Trim()
    if ([string]::IsNullOrWhiteSpace($text) -or $text -eq "-") { return $null }
    $number = 0.0
    $style = [System.Globalization.NumberStyles]::Float
    $culture = [System.Globalization.CultureInfo]::InvariantCulture
    if (-not [double]::TryParse($text, $style, $culture, [ref]$number)) { return $null }
    return [Math]::Round($number / $Scale, $Digits)
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

function Resolve-StockSymbol {
    param([AllowNull()][object]$Value)

    $input = ([string]$Value).Trim().ToUpperInvariant().Replace(" ", "")
    if ([string]::IsNullOrWhiteSpace($input)) { $input = "000034" }

    $market = ""
    $code = ""
    if ($input -match '^(SH|SZ)[:.-]?(\d{6})$') {
        $market = $Matches[1]
        $code = $Matches[2]
    }
    elseif ($input -match '^(\d{6})[:.-]?(SH|SZ)$') {
        $code = $Matches[1]
        $market = $Matches[2]
    }
    elseif ($input -match '^(\d{6})$') {
        $code = $Matches[1]
        if ($code.StartsWith("6")) { $market = "SH" } else { $market = "SZ" }
    }
    else {
        throw "请输入 6 位沪深 A 股代码，例如 000034、SZ000034 或 600519.SH"
    }

    if ($market -eq "SH" -and -not $code.StartsWith("6")) {
        throw "当前仅支持沪市 6 开头和深市 0/3 开头的 A 股代码"
    }
    if ($market -eq "SZ" -and -not ($code.StartsWith("0") -or $code.StartsWith("3"))) {
        throw "当前仅支持沪市 6 开头和深市 0/3 开头的 A 股代码"
    }

    $marketId = if ($market -eq "SH") { 1 } else { 0 }
    return [ordered]@{
        code = $code
        market = $market
        secid = "$marketId.$code"
    }
}

function Resolve-RefreshInterval {
    param([AllowNull()][object]$Value)

    $parsed = 15
    if (-not [int]::TryParse([string]$Value, [ref]$parsed)) { $parsed = 15 }
    if ($parsed -notin $script:AllowedIntervals) { $parsed = 15 }
    return $parsed
}

function Resolve-ApiEndpoints {
    param([AllowNull()][object]$ActionOverride)

    $override = [string]$ActionOverride
    if ([string]::IsNullOrWhiteSpace($override)) {
        $override = [Environment]::GetEnvironmentVariable("LOOM_STOCK_API_BASE_URL", "Process")
    }
    if ([string]::IsNullOrWhiteSpace($override)) {
        return [ordered]@{
            quote = [Uri]"https://push2.eastmoney.com/api/qt/stock/get"
            trend = [Uri]"https://push2his.eastmoney.com/api/qt/stock/trends2/get"
        }
    }

    [Uri]$baseUri = $null
    if (-not [Uri]::TryCreate($override.Trim(), [UriKind]::Absolute, [ref]$baseUri)) {
        throw "股票行情测试 API 根地址必须是有效的回环 HTTP 地址"
    }
    if ($baseUri.Scheme -notin @("http", "https") -or -not $baseUri.IsLoopback) {
        throw "股票行情测试 API 根地址仅允许用于回环测试服务"
    }
    return [ordered]@{
        quote = [Uri]::new($baseUri, "/api/qt/stock/get")
        trend = [Uri]::new($baseUri, "/api/qt/stock/trends2/get")
    }
}

function New-RequestUri {
    param(
        [Uri]$BaseUri,
        [string]$Query
    )
    $builder = [UriBuilder]::new($BaseUri)
    $builder.Query = $Query
    return $builder.Uri
}

function Invoke-EastmoneyRequest {
    param([Uri]$Uri)

    return Invoke-RestMethod -Uri $Uri.AbsoluteUri -Method Get -TimeoutSec 12 -Headers @{
        "Accept" = "application/json"
        "Referer" = "https://quote.eastmoney.com/"
        "User-Agent" = "Neuro-Loom-Stock-Monitor/1.0"
    }
}

function Get-ShanghaiNow {
    try {
        $zone = [TimeZoneInfo]::FindSystemTimeZoneById("China Standard Time")
        return [TimeZoneInfo]::ConvertTimeFromUtc([DateTime]::UtcNow, $zone)
    }
    catch {
        return [DateTime]::UtcNow.AddHours(8)
    }
}

function Get-MarketState {
    param([AllowNull()][string]$ProviderTimestamp)

    [DateTime]$providerTime = [DateTime]::MinValue
    $culture = [System.Globalization.CultureInfo]::InvariantCulture
    $parsed = [DateTime]::TryParseExact(
        $ProviderTimestamp,
        "yyyy-MM-dd HH:mm",
        $culture,
        [System.Globalization.DateTimeStyles]::None,
        [ref]$providerTime
    )
    $now = Get-ShanghaiNow
    $time = $now.TimeOfDay
    $morning = $time -ge [TimeSpan]::FromHours(9.5) -and $time -le [TimeSpan]::FromHours(11.5)
    $afternoon = $time -ge [TimeSpan]::FromHours(13) -and $time -le [TimeSpan]::FromHours(15)
    $weekday = $now.DayOfWeek -notin @([DayOfWeek]::Saturday, [DayOfWeek]::Sunday)
    if ($parsed -and $providerTime.Date -eq $now.Date -and $weekday -and ($morning -or $afternoon)) {
        return [ordered]@{ id = "open"; label = "交易中" }
    }
    return [ordered]@{ id = "closed"; label = "已收盘" }
}

function Convert-EastmoneyTrend {
    param([AllowNull()][object]$RawTrends)

    $result = @()
    foreach ($line in @($RawTrends | Select-Object -First 512)) {
        $parts = ([string]$line).Split(',')
        if ($parts.Count -lt 8) { continue }
        $price = Convert-ScaledNumber -Value $parts[1] -Scale 1 -Digits 3
        $average = Convert-ScaledNumber -Value $parts[7] -Scale 1 -Digits 3
        if ($null -eq $price) { continue }
        $timestamp = $parts[0].Trim()
        $result += [ordered]@{
            timestamp = $timestamp
            time = if ($timestamp.Length -ge 16) { $timestamp.Substring(11, 5) } else { $timestamp }
            price = $price
            average = $average
            volumeLots = Convert-ScaledNumber -Value $parts[5] -Scale 1 -Digits 0
            amount = Convert-ScaledNumber -Value $parts[6] -Scale 1 -Digits 2
        }
    }
    return @($result)
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
    $response = [ordered]@{
        status = "success"
        output = [ordered]@{ surfaceAction = $surfaceAction }
    }
    [Console]::Out.Write(($response | ConvertTo-Json -Depth 100 -Compress))
}

function Write-RuntimeError {
    param([string]$Message)
    $response = [ordered]@{
        status = "error"
        error = [ordered]@{
            code = "stock_monitor_failed"
            message = $Message
        }
    }
    [Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
}

function Write-SurfaceErrorState {
    param(
        [object]$Action,
        [string]$Message
    )

    $symbol = [string](Get-ActionStateValue -Action $Action -Name "symbol" -DefaultValue "000034")
    $market = [string](Get-ActionStateValue -Action $Action -Name "market" -DefaultValue "SZ")
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $Action -Name "intervalSeconds" -DefaultValue 15)
    $statePatch = [ordered]@{
        schemaVersion = 1
        provider = [ordered]@{ id = "eastmoney"; name = "东方财富" }
        marketScope = "沪深 A 股"
        symbol = $symbol
        market = $market
        intervalSeconds = $interval
        status = "error"
        statusText = "行情获取失败"
        error = $Message
        disclaimer = "仅用于行情观察，不构成交易指令"
    }
    $operations = [object[]]@(
        (New-SetOperation -NodeId "status" -Path "/props/text" -Value "行情获取失败"),
        (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value $Message),
        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $symbol),
        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval))
    )
    Write-SurfaceResponse -Patches ([object[]]@([ordered]@{
        operations = $operations
        statePatch = $statePatch
    }))
}

function Get-StockSnapshot {
    param(
        [object]$ResolvedSymbol,
        [AllowNull()][object]$ApiBaseUrl
    )

    $endpoints = Resolve-ApiEndpoints -ActionOverride $ApiBaseUrl
    $secid = [Uri]::EscapeDataString([string]$ResolvedSymbol.secid)
    $quoteQuery = "secid=$secid&fields=$script:QuoteFields"
    $trendQuery = "secid=$secid&fields1=$script:TrendFields1&fields2=$script:TrendFields2&ndays=1&iscr=0&iscca=0"
    $quoteResponse = Invoke-EastmoneyRequest -Uri (New-RequestUri -BaseUri $endpoints.quote -Query $quoteQuery)
    $trendResponse = Invoke-EastmoneyRequest -Uri (New-RequestUri -BaseUri $endpoints.trend -Query $trendQuery)

    if ([int](Get-ObjectPropertyValue -Value $quoteResponse -Name "rc" -DefaultValue -1) -ne 0) {
        throw "东方财富报价接口返回失败"
    }
    if ([int](Get-ObjectPropertyValue -Value $trendResponse -Name "rc" -DefaultValue -1) -ne 0) {
        throw "东方财富分时接口返回失败"
    }
    $quoteData = Get-ObjectPropertyValue -Value $quoteResponse -Name "data"
    $trendData = Get-ObjectPropertyValue -Value $trendResponse -Name "data"
    if ($null -eq $quoteData -or $null -eq $trendData) {
        throw "未找到该股票代码的沪深 A 股行情"
    }

    $price = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f43") -Scale 100 -Digits 2
    $name = ([string](Get-ObjectPropertyValue $quoteData "f58")).Trim()
    if ($null -eq $price -or [string]::IsNullOrWhiteSpace($name)) {
        throw "行情数据缺少股票名称或最新价格"
    }

    $trend = Convert-EastmoneyTrend -RawTrends (Get-ObjectPropertyValue $trendData "trends" @())
    $providerTimestamp = if ($trend.Count -gt 0) { [string]$trend[-1].timestamp } else { $null }
    $marketState = Get-MarketState -ProviderTimestamp $providerTimestamp
    $symbol = ([string](Get-ObjectPropertyValue $quoteData "f57" $ResolvedSymbol.code)).Trim()
    $quote = [ordered]@{
        provider = "eastmoney"
        providerName = "东方财富"
        market = [string]$ResolvedSymbol.market
        symbol = $symbol
        name = $name
        currency = "CNY"
        price = $price
        change = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f169") -Scale 100 -Digits 2
        changePercent = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f170") -Scale 100 -Digits 2
        open = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f46") -Scale 100 -Digits 2
        high = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f44") -Scale 100 -Digits 2
        low = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f45") -Scale 100 -Digits 2
        previousClose = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f60") -Scale 100 -Digits 2
        upperLimit = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f51") -Scale 100 -Digits 2
        lowerLimit = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f52") -Scale 100 -Digits 2
        volumeLots = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f47") -Scale 1 -Digits 0
        amount = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f48") -Scale 1 -Digits 2
        volumeRatio = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f50") -Scale 100 -Digits 2
        totalMarketCap = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f116") -Scale 1 -Digits 2
        floatMarketCap = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f117") -Scale 1 -Digits 2
        peDynamic = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f162") -Scale 100 -Digits 2
        pb = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f167") -Scale 100 -Digits 2
        turnoverRate = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f168") -Scale 100 -Digits 2
        amplitude = Convert-ScaledNumber -Value (Get-ObjectPropertyValue $quoteData "f171") -Scale 100 -Digits 2
        timestamp = $providerTimestamp
        marketState = $marketState.id
        marketStateLabel = $marketState.label
    }
    return [ordered]@{ quote = $quote; trend = $trend }
}

try {
    $requestText = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($requestText)) { throw "Surface action request is required" }
    $request = $requestText | ConvertFrom-Json
    $script:SurfaceAction = Find-SurfaceAction -Value $request
    if ($null -eq $script:SurfaceAction) { throw "surfaceAction invocation is required" }

    $actionId = [string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId")
    if ($actionId -notin @("stock_refresh", "stock_symbol_commit", "stock_interval_commit")) {
        throw "action is not declared by the stock monitor: $actionId"
    }

    if ($actionId -eq "stock_interval_commit") {
        $interval = Resolve-RefreshInterval (Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue 15)
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

    $symbolValue = if ($actionId -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue "000034"
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "symbol" -DefaultValue "000034"
    }
    $resolvedSymbol = Resolve-StockSymbol -Value $symbolValue
    $interval = Resolve-RefreshInterval (Get-ActionStateValue -Action $script:SurfaceAction -Name "intervalSeconds" -DefaultValue 15)
    $testApiBaseUrl = Get-ActionPayloadValue -Action $script:SurfaceAction -Name "testApiBaseUrl" -DefaultValue $null
    $snapshot = Get-StockSnapshot -ResolvedSymbol $resolvedSymbol -ApiBaseUrl $testApiBaseUrl
    $quote = $snapshot.quote
    $trend = @($snapshot.trend)
    $statusText = "$($quote.marketStateLabel) · 行情已更新"
    $statePatch = [ordered]@{
        schemaVersion = 1
        provider = [ordered]@{ id = "eastmoney"; name = "东方财富" }
        marketScope = "沪深 A 股"
        symbol = [string]$quote.symbol
        market = [string]$quote.market
        intervalSeconds = $interval
        status = "ready"
        statusText = $statusText
        quote = $quote
        trend = $trend
        lastUpdatedAt = [string]$quote.timestamp
        error = $null
        disclaimer = "仅用于行情观察，不构成交易指令"
    }
    $metricsText = "今开 $(Format-Price $quote.open)  最高 $(Format-Price $quote.high)  最低 $(Format-Price $quote.low)  昨收 $(Format-Price $quote.previousClose)"
    $operations = [object[]]@(
        (New-SetOperation -NodeId "status" -Path "/props/text" -Value $statusText),
        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value ([string]$quote.symbol)),
        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value ([string]$interval)),
        (New-SetOperation -NodeId "quote_name" -Path "/props/text" -Value "$($quote.name) $($quote.symbol) · $($quote.market)"),
        (New-SetOperation -NodeId "quote_price" -Path "/props/text" -Value (Format-Price $quote.price)),
        (New-SetOperation -NodeId "quote_change" -Path "/props/text" -Value "$(Format-SignedNumber $quote.change)  $(Format-SignedNumber $quote.changePercent '%')"),
        (New-SetOperation -NodeId "quote_metrics" -Path "/props/text" -Value $metricsText)
    )
    $formalQuote = [ordered]@{
        provider = "eastmoney"
        providerName = "东方财富"
        market = [string]$quote.market
        symbol = [string]$quote.symbol
        name = [string]$quote.name
        currency = "CNY"
        price = $quote.price
        change = $quote.change
        changePercent = $quote.changePercent
        timestamp = [string]$quote.timestamp
        marketState = [string]$quote.marketState
        metrics = [ordered]@{
            open = $quote.open
            high = $quote.high
            low = $quote.low
            previousClose = $quote.previousClose
            upperLimit = $quote.upperLimit
            lowerLimit = $quote.lowerLimit
            volumeLots = $quote.volumeLots
            amount = $quote.amount
            volumeRatio = $quote.volumeRatio
            totalMarketCap = $quote.totalMarketCap
            floatMarketCap = $quote.floatMarketCap
            peDynamic = $quote.peDynamic
            pb = $quote.pb
            turnoverRate = $quote.turnoverRate
            amplitude = $quote.amplitude
        }
        trend = $trend
    }
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
