$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:SurfaceAction = $null
$script:SurfaceActionBudgets = $null
$script:AllowedIntervals = @(1, 3, 5, 15, 30, 60, 120, 300)
$script:AllowedPeriods = @("minute", "five-day", "day", "week", "month", "quarter", "year", "minute-120", "minute-60", "minute-30", "minute-15", "minute-5", "minute-1")
$script:MaxHistoryRows = 2000
$script:MaxOrderBookLevels = 10
$script:MaxLiveAgeSeconds = 90
$script:MaxOrderBookAgeSeconds = 120
$script:ProviderVersion = "2.9.0"
$script:UpstreamVersion = "2.7.3"
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

function Resolve-SurfaceAction {
    # 只在三个固定位置找 surfaceAction：请求根、params、inputs。宿主
    # (framework-packages/runtime-host/src/mcp.rs 的 find_surface_action) 只认 params 与
    # inputs，本地测试夹具把它放在请求根，所以这里接受三者的并集，但绝不递归。
    # 递归搜索会把 frameworkData.mcp.results 里的上游数据也当成调用来源：任何能让 MCP 服务
    # 回一个含 surfaceAction 键的对象的人，就能凭空造出一次动作调用。位置固定后这条路被封死，
    # 也不再需要深度上限。
    param([AllowNull()][object]$Value)

    if ($null -eq $Value) { return $null }
    $found = $null
    $foundText = $null
    foreach ($container in @($Value, (Get-ObjectPropertyValue -Value $Value -Name "params"), (Get-ObjectPropertyValue -Value $Value -Name "inputs"))) {
        if ($null -eq $container) { continue }
        $candidate = Get-ObjectPropertyValue -Value $container -Name "surfaceAction"
        if ($null -eq $candidate) { continue }
        if (-not (($candidate -is [System.Collections.IDictionary]) -or ($candidate -is [pscustomobject]))) {
            throw "surfaceAction must be a JSON object"
        }
        # PSCustomObject 的 -ne 是引用比较，同一份 JSON 反序列化两次也会"不等"，所以按序列化
        # 后的文本判断是否真的冲突。
        $candidateText = $candidate | ConvertTo-Json -Depth 20 -Compress
        if ($null -eq $found) {
            $found = $candidate
            $foundText = $candidateText
            continue
        }
        if ($candidateText -ne $foundText) {
            throw "conflicting surfaceAction invocations were provided"
        }
    }
    return $found
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

function Get-ActionRequestId {
    param([AllowNull()][object]$Action)

    $value = Get-ActionPayloadValue -Action $Action -Name "requestId"
    if ($null -eq $value) { return $null }
    $textValue = [string]$value
    $textValue = $textValue.Trim()
    if ($textValue.Length -eq 0) { return $null }
    if ($textValue.Length -gt 64) { return $textValue.Substring(0, 64) }
    return $textValue
}

function Get-SurfaceActionBudgets {
    # Surface 没有可读的动作预算通道，宿主超时也不会回补丁，所以运行时把 manifest 声明的
    # 每动作 timeoutMs（按 art.runtime.json 的 limits.timeoutMs 上限收敛）回写进状态，
    # 客户端才能把兜底计时器排在宿主放弃之后。读取失败时返回空表，客户端用镜像常量。
    if ($null -ne $script:SurfaceActionBudgets) { return $script:SurfaceActionBudgets }
    $budgets = [ordered]@{}
    try {
        $packageRoot = Split-Path -Parent $PSScriptRoot
        $manifestPath = Join-Path $packageRoot "manifest.json"
        $runtimePath = Join-Path $packageRoot "art.runtime.json"
        $ceiling = 0
        if (Test-Path -LiteralPath $runtimePath) {
            $runtimeConfig = Get-Content -LiteralPath $runtimePath -Raw -Encoding UTF8 | ConvertFrom-Json
            $limits = Get-ObjectPropertyValue -Value $runtimeConfig -Name "limits"
            $limitValue = Get-ObjectPropertyValue -Value $limits -Name "timeoutMs"
            if ($null -ne $limitValue) { $ceiling = [int]$limitValue }
        }
        if (Test-Path -LiteralPath $manifestPath) {
            $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
            $metadata = Get-ObjectPropertyValue -Value $manifest -Name "metadata"
            $capabilities = Get-ObjectPropertyValue -Value $metadata -Name "capabilities"
            $surface = Get-ObjectPropertyValue -Value $capabilities -Name "surface"
            $actions = Get-ObjectPropertyValue -Value $surface -Name "actions"
            if ($null -ne $actions) {
                foreach ($action in @($actions)) {
                    $actionId = Get-ObjectPropertyValue -Value $action -Name "id"
                    $timeoutValue = Get-ObjectPropertyValue -Value $action -Name "timeoutMs"
                    if ([string]::IsNullOrWhiteSpace([string]$actionId)) { continue }
                    if ($null -eq $timeoutValue) { continue }
                    $timeout = [int]$timeoutValue
                    if ($timeout -le 0) { continue }
                    if ($ceiling -gt 0 -and $timeout -gt $ceiling) { $timeout = $ceiling }
                    $budgets[[string]$actionId] = $timeout
                }
            }
        }
    }
    catch {
        $budgets = [ordered]@{}
    }
    $script:SurfaceActionBudgets = $budgets
    return $script:SurfaceActionBudgets
}

function Add-ActionEcho {
    # 每个状态补丁都必须带上回声字段。statePatch 是合并语义，留下上一次动作的
    # lastRequestId 会让客户端的关联检查永远匹配不上，pending 只能等兜底计时器解锁。
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$StatePatch,
        [AllowNull()][object]$Action
    )

    $actionId = Get-ObjectPropertyValue -Value $Action -Name "actionId"
    $StatePatch["lastActionId"] = if ($null -eq $actionId) { $null } else { [string]$actionId }
    $StatePatch["lastRequestId"] = Get-ActionRequestId -Action $Action
    $StatePatch["actionBudgetsMillis"] = Get-SurfaceActionBudgets
    return $StatePatch
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
        throw "请输入股票代码，例如 SZ000034、SH600519、BJ430047、HK00700 或 USAAPL"
    }
    if ($input -match '^(SH|SZ|BJ)[:._-]?(\d{6})$') {
        return "$($Matches[1])$($Matches[2])"
    }
    if ($input -match '^(\d{6})[:._-]?(SH|SZ|BJ)$') {
        return "$($Matches[2])$($Matches[1])"
    }
    if ($input -match '^(\d{6})$') {
        $market = if ($input.StartsWith("4") -or $input.StartsWith("8")) {
            "BJ"
        }
        elseif ($input.StartsWith("5") -or $input.StartsWith("6") -or $input.StartsWith("9")) {
            "SH"
        }
        else { "SZ" }
        return "$market$input"
    }
    if ($input -match '^HK[:._-]?(\d{1,5})$') {
        return "HK$($Matches[1].PadLeft(5, '0'))"
    }
    if ($input -match '^US[:_-]?([A-Z][A-Z0-9.-]{0,19})$') {
        return "US$($Matches[1])"
    }
    throw "股票代码格式无效；支持 SZ000034、SH600519、BJ430047、HK00700 和 USAAPL 等统一代码"
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
        "xueqiu" { return "雪球" }
        "pysnowball" { return "pysnowball / 雪球" }
        "mixed" { return "pysnowball + 雪球" }
        default { return $Source }
    }
}

function Resolve-UtcTimestamp {
    # 上游时间戳必须自带时区偏移（Z 或 ±HH[:]MM）。之前用 AssumeUniversal 解析，等于把无偏移
    # 的本地时间当成 UTC：东八区的 "2026-08-21 15:00:00" 会被当作 UTC 15:00，凭空多出 8 小时
    # 的"新鲜度"，一个已经过期的报价因此显示为最新。没有偏移就当作不可判定，返回 $null，让
    # 调用方按过期处理。
    param(
        [AllowNull()][object]$Value,
        [AllowNull()][object]$FallbackValue = $null
    )

    foreach ($candidate in @($Value, $FallbackValue)) {
        $text = ([string]$candidate).Trim()
        if ([string]::IsNullOrWhiteSpace($text)) { continue }
        if ($text -notmatch '(?:[Zz]|[+-]\d{2}:?\d{2})$') { continue }
        try {
            return [DateTimeOffset]::Parse(
                $text,
                [System.Globalization.CultureInfo]::InvariantCulture,
                [System.Globalization.DateTimeStyles]::None
            ).ToUniversalTime().ToString("o")
        }
        catch {}
    }
    return $null
}

function Get-ObservationAgeSeconds {
    # 时间戳不可判定时返回 $null，不返回 [double]::PositiveInfinity：ConvertTo-Json 会把它写成
    # 裸 Infinity，那不是合法 JSON，宿主解析整份响应都会失败。调用方必须显式处理 $null，
    # 因为 PowerShell 里 $null -gt 90 是 $false，直接比较会把未知年龄判成"新鲜"。
    param([AllowNull()][object]$Value)

    $timestamp = Resolve-UtcTimestamp -Value $Value
    if ($null -eq $timestamp) { return $null }
    $age = ([DateTimeOffset]::UtcNow - [DateTimeOffset]::Parse($timestamp)).TotalSeconds
    return [Math]::Round([Math]::Max(0, $age), 3)
}

function Resolve-RefreshInterval {
    param([AllowNull()][object]$Value)

    $parsed = 5
    if (-not [int]::TryParse([string]$Value, [ref]$parsed)) { $parsed = 5 }
    if ($parsed -notin $script:AllowedIntervals) { $parsed = 5 }
    return $parsed
}

function Resolve-MarketPeriod {
    param([AllowNull()][object]$Value)

    $period = ([string]$Value).Trim().ToLowerInvariant()
    if ($period -notin $script:AllowedPeriods) { return "day" }
    return $period
}

function Get-MarketPeriodLabel {
    param([string]$Period)

    switch ($Period) {
        "minute" { return "分时" }
        "five-day" { return "五日" }
        "day" { return "日 K" }
        "week" { return "周 K" }
        "month" { return "月 K" }
        "quarter" { return "季 K" }
        "year" { return "年 K" }
        "minute-120" { return "120 分钟" }
        "minute-60" { return "60 分钟" }
        "minute-30" { return "30 分钟" }
        "minute-15" { return "15 分钟" }
        "minute-5" { return "5 分钟" }
        "minute-1" { return "1 分钟" }
        default { return "日 K" }
    }
}

function Resolve-TradingDate {
    param([AllowNull()][object]$Value)

    $text = ([string]$Value).Trim()
    if ($text -notmatch '^(\d{4}-\d{2}-\d{2})') { return $null }
    $candidate = $Matches[1]
    try {
        $null = [DateTime]::ParseExact(
            $candidate,
            "yyyy-MM-dd",
            [System.Globalization.CultureInfo]::InvariantCulture
        )
        return $candidate
    }
    catch {
        return $null
    }
}

function Get-MarketSessionState {
    param(
        [string]$Market,
        [string]$LastTradingDate
    )

    $zoneId = if ($Market -eq "US") { "Eastern Standard Time" } else { "China Standard Time" }
    try {
        $localNow = [System.TimeZoneInfo]::ConvertTimeBySystemTimeZoneId([DateTimeOffset]::UtcNow, $zoneId)
    }
    catch {
        $localNow = [DateTimeOffset]::UtcNow
    }
    $isTradingDay = $localNow.DayOfWeek -notin @([DayOfWeek]::Saturday, [DayOfWeek]::Sunday)
    $isLatestDay = $LastTradingDate -eq $localNow.ToString("yyyy-MM-dd")
    $minuteOfDay = ($localNow.Hour * 60) + $localNow.Minute
    $insideSession = switch ($Market) {
        "US" { $minuteOfDay -ge 570 -and $minuteOfDay -lt 960 }
        "HK" { ($minuteOfDay -ge 570 -and $minuteOfDay -lt 720) -or ($minuteOfDay -ge 780 -and $minuteOfDay -lt 960) }
        default { ($minuteOfDay -ge 570 -and $minuteOfDay -lt 690) -or ($minuteOfDay -ge 780 -and $minuteOfDay -lt 900) }
    }
    if ($isTradingDay -and $isLatestDay -and $insideSession) {
        return "open"
    }
    return "closed"
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

function Try-Get-McpToolContent {
    param(
        [object]$Request,
        [string]$CallId
    )

    try {
        return [ordered]@{
            content = Get-McpToolContent -Request $Request -CallId $CallId
            error = $null
        }
    }
    catch {
        return [ordered]@{
            content = $null
            error = $_.Exception.Message
        }
    }
}

function Get-StockFromActionState {
    param([object]$Action)

    $quote = Get-ActionStateValue -Action $Action -Name "quote"
    if ($null -eq $quote) { return $null }
    try {
        $code = Resolve-StockCode (Get-ObjectPropertyValue -Value $quote -Name "code")
    }
    catch {
        return $null
    }
    $name = ([string](Get-ObjectPropertyValue -Value $quote -Name "name")).Trim()
    $now = Convert-NullableNumber (Get-ObjectPropertyValue -Value $quote -Name "rawPrice" -DefaultValue (Get-ObjectPropertyValue -Value $quote -Name "price"))
    if ([string]::IsNullOrWhiteSpace($name) -or $null -eq $now -or $now -le 0) { return $null }
    $changePercent = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $quote -Name "changePercent") -Digits 8
    return [ordered]@{
        code = $code
        name = $name
        now = $now
        low = Convert-NullableNumber (Get-ObjectPropertyValue -Value $quote -Name "low")
        high = Convert-NullableNumber (Get-ObjectPropertyValue -Value $quote -Name "high")
        yesterday = Convert-NullableNumber (Get-ObjectPropertyValue -Value $quote -Name "previousClose")
        percent = if ($null -eq $changePercent) { $null } else { $changePercent / 100.0 }
        source = ([string](Get-ObjectPropertyValue -Value $quote -Name "source" -DefaultValue "eastmoney")).Trim().ToLowerInvariant()
        observedAt = [string](Get-ObjectPropertyValue -Value $quote -Name "observedAt")
        fetchedAt = [string](Get-ObjectPropertyValue -Value $quote -Name "fetchedAt")
        stale = [bool](Get-ObjectPropertyValue -Value $quote -Name "stale" -DefaultValue $false)
    }
}

function ConvertTo-FavoriteQuotes {
    param([AllowNull()][object]$Values)

    $quotes = [System.Collections.Generic.List[object]]::new()
    foreach ($value in @($Values | Select-Object -First 12)) {
        try { $code = Resolve-StockCode (Get-ObjectPropertyValue -Value $value -Name "code") }
        catch { continue }
        $name = ([string](Get-ObjectPropertyValue -Value $value -Name "name")).Trim()
        $price = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "now" -DefaultValue (Get-ObjectPropertyValue -Value $value -Name "price"))
        if ([string]::IsNullOrWhiteSpace($name) -or $name -eq "---" -or $null -eq $price -or $price -le 0) { continue }
        $previousClose = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "yesterday" -DefaultValue (Get-ObjectPropertyValue -Value $value -Name "previousClose"))
        $percentFraction = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "percent") -Digits 8
        $changePercent = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "changePercent") -Digits 4
        if ($null -eq $changePercent -and $null -ne $percentFraction) {
            $changePercent = [Math]::Round($percentFraction * 100, 4)
        }
        if ($null -eq $changePercent -and $null -ne $previousClose -and $previousClose -gt 0) {
            $changePercent = [Math]::Round((($price - $previousClose) / $previousClose) * 100, 4)
        }
        $market = Get-MarketFromCode -Code $code
        $quotes.Add([ordered]@{
            code = $code
            market = $market
            name = $name
            currency = Get-CurrencyForMarket -Market $market
            price = $price
            change = if ($null -ne $previousClose) { [Math]::Round($price - $previousClose, 4) } else { $null }
            changePercent = $changePercent
            previousClose = $previousClose
            source = ([string](Get-ObjectPropertyValue -Value $value -Name "source" -DefaultValue "eastmoney")).Trim().ToLowerInvariant()
            observedAt = [string](Get-ObjectPropertyValue -Value $value -Name "observedAt")
            fetchedAt = [string](Get-ObjectPropertyValue -Value $value -Name "fetchedAt")
            stale = [bool](Get-ObjectPropertyValue -Value $value -Name "stale" -DefaultValue $false)
        })
    }
    return @($quotes.ToArray())
}

function ConvertTo-HistoryRows {
    param([AllowNull()][object]$Values)

    $rows = [System.Collections.Generic.List[object]]::new()
    foreach ($value in @($Values | Select-Object -Last $script:MaxHistoryRows)) {
        $date = ([string](Get-ObjectPropertyValue -Value $value -Name "date")).Trim()
        $normalizedDate = Resolve-TradingDate -Value $date
        $open = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "open")
        $close = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "close")
        $high = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "high")
        $low = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "low")
        if ($null -eq $normalizedDate -or $null -eq $open -or $null -eq $close -or $null -eq $high -or $null -eq $low) {
            continue
        }
        if ($open -le 0 -or $close -le 0 -or $high -le 0 -or $low -le 0 -or $high -lt $low) {
            continue
        }
        $source = ([string](Get-ObjectPropertyValue -Value $value -Name "source")).Trim().ToLowerInvariant()
        $rows.Add([ordered]@{
            date = $date
            open = $open
            close = $close
            high = $high
            low = $low
            volume = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "volume") -Digits 0
            source = $source
        })
    }
    return @($rows.ToArray())
}

function ConvertTo-OrderBookLevels {
    param([AllowNull()][object]$Values)

    $levels = [System.Collections.Generic.List[object]]::new()
    foreach ($value in @($Values | Select-Object -First $script:MaxOrderBookLevels)) {
        $price = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "price")
        if ($null -eq $price -or $price -le 0) { continue }
        $levels.Add([ordered]@{
            level = [int](Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "level" -DefaultValue ($levels.Count + 1)) -Digits 0)
            price = $price
            volume = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "volume") -Digits 0
            orders = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "orders") -Digits 0
        })
    }
    return @($levels.ToArray())
}

function ConvertTo-OrderBook {
    param(
        [AllowNull()][object]$Value,
        [string]$Code,
        [AllowNull()][object]$FetchedAt = $null
    )

    if ($null -eq $Value) { return $null }
    $bids = @(ConvertTo-OrderBookLevels (Get-ObjectPropertyValue -Value $Value -Name "bids" -DefaultValue @()))
    $asks = @(ConvertTo-OrderBookLevels (Get-ObjectPropertyValue -Value $Value -Name "asks" -DefaultValue @()))
    if ($bids.Count -eq 0 -and $asks.Count -eq 0) { return $null }
    $normalizedFetchedAt = Resolve-UtcTimestamp -Value (Get-ObjectPropertyValue -Value $Value -Name "fetchedAt") -FallbackValue $FetchedAt
    # 不再用 [DateTimeOffset]::UtcNow 顶替缺失的上游时间戳：那等于宣布"刚刚取到"，年龄算出来
    # 是 0，任何过期检查都会通过。上游没给可判定的时间戳，就让年龄为 $null 并直接判过期。
    $observedAt = Resolve-UtcTimestamp -Value (Get-ObjectPropertyValue -Value $Value -Name "observedAt") -FallbackValue $normalizedFetchedAt
    $ageSeconds = Get-ObservationAgeSeconds -Value $observedAt
    return [ordered]@{
        code = $Code
        bids = $bids
        asks = $asks
        buyPercent = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "buyPercent")
        sellPercent = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "sellPercent")
        netVolume = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "netVolume") -Digits 0
        ratio = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "ratio")
        levels = [Math]::Max($bids.Count, $asks.Count)
        observedAt = $observedAt
        fetchedAt = $normalizedFetchedAt
        ageSeconds = $ageSeconds
        maxAgeSeconds = $script:MaxOrderBookAgeSeconds
        stale = ($null -eq $ageSeconds) -or ($ageSeconds -gt $script:MaxOrderBookAgeSeconds)
        source = ([string](Get-ObjectPropertyValue -Value $Value -Name "source" -DefaultValue "xueqiu")).Trim().ToLowerInvariant()
    }
}

function ConvertTo-LiveTape {
    param(
        [AllowNull()][object]$Value,
        [string]$Code,
        [AllowNull()][object]$FetchedAt = $null
    )

    if ($null -eq $Value) { return $null }
    $current = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "now" -DefaultValue (Get-ObjectPropertyValue -Value $Value -Name "price"))
    if ($null -eq $current -or $current -le 0) { return $null }
    $normalizedFetchedAt = Resolve-UtcTimestamp -Value (Get-ObjectPropertyValue -Value $Value -Name "fetchedAt") -FallbackValue $FetchedAt
    # 与盘口一致：缺失的时间戳不合成，未知年龄直接判过期。实时逐笔比盘口更敏感，一条被当成
    # 新鲜的旧 tick 会直接写进主报价的显示价格。
    $observedAt = Resolve-UtcTimestamp -Value (Get-ObjectPropertyValue -Value $Value -Name "observedAt") -FallbackValue $normalizedFetchedAt
    $ageSeconds = Get-ObservationAgeSeconds -Value $observedAt
    return [ordered]@{
        code = $Code
        price = $current
        open = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "open")
        high = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "high")
        low = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "low")
        previousClose = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "yesterday" -DefaultValue (Get-ObjectPropertyValue -Value $Value -Name "previousClose"))
        avgPrice = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "avgPrice")
        volume = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "volume") -Digits 0
        amount = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "amount") -Digits 0
        turnoverRate = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "turnoverRate")
        amplitude = Convert-NullableNumber (Get-ObjectPropertyValue -Value $Value -Name "amplitude")
        marketCapital = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "marketCapital") -Digits 0
        isTrade = [bool](Get-ObjectPropertyValue -Value $Value -Name "isTrade" -DefaultValue $false)
        tradeSession = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "tradeSession") -Digits 0
        observedAt = $observedAt
        fetchedAt = $normalizedFetchedAt
        ageSeconds = $ageSeconds
        maxAgeSeconds = $script:MaxLiveAgeSeconds
        stale = ($null -eq $ageSeconds) -or ($ageSeconds -gt $script:MaxLiveAgeSeconds)
        source = ([string](Get-ObjectPropertyValue -Value $Value -Name "source" -DefaultValue "xueqiu")).Trim().ToLowerInvariant()
    }
}

function Get-StockSnapshot {
    param([object]$Request)

    $actionId = [string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId")
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
    $name = ([string](Get-ObjectPropertyValue -Value $stock -Name "name")).Trim()
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
    $quoteProviderStale = [bool](Get-ObjectPropertyValue -Value $quoteResponse -Name "stale" -DefaultValue (Get-ObjectPropertyValue -Value $stock -Name "stale" -DefaultValue $false))

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
    $source = ([string](Get-ObjectPropertyValue -Value $stock -Name "source")).Trim().ToLowerInvariant()
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
            if ($null -ne $cachedOrderBook -and -not [bool](Get-ObjectPropertyValue -Value $cachedOrderBook -Name "stale" -DefaultValue $true)) {
                $orderBook = $cachedOrderBook
            }
        }
        if ($null -eq $liveTape -and [string](Get-ObjectPropertyValue -Value $cachedTape -Name "code") -eq $code) {
            $cachedLiveTape = ConvertTo-LiveTape -Value $cachedTape -Code $code
            if ($null -ne $cachedLiveTape -and -not [bool](Get-ObjectPropertyValue -Value $cachedLiveTape -Name "stale" -DefaultValue $true)) {
                $liveTape = $cachedLiveTape
            }
        }
    }
    $liveTapeFresh = $null -ne $liveTape -and -not [bool](Get-ObjectPropertyValue -Value $liveTape -Name "stale" -DefaultValue $true)
    $marketStatus = Get-MarketSessionState -Market $market -LastTradingDate $lastTradingDate
    if ($liveTapeFresh -and [bool](Get-ObjectPropertyValue -Value $liveTape -Name "isTrade" -DefaultValue $false)) {
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
    try { $code = Resolve-StockCode $rawCode } catch { $code = ([string]$rawCode).Trim() }
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
        stale = [bool]$quote.stale
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

try {
    $requestText = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($requestText)) { throw "Stock Monitor request is required" }
    $request = $requestText | ConvertFrom-Json
    $script:SurfaceAction = Resolve-SurfaceAction -Value $request

    if ($null -ne $script:SurfaceAction) {
        $actionId = [string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId")
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

    $requestedCode = if ([string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId") -eq "stock_symbol_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $quote.code
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "code" -DefaultValue $quote.code
    }
    if ((Resolve-StockCode $requestedCode) -ne [string]$quote.code) {
        throw "stock-api 返回的股票代码与请求不一致"
    }
    $actionId = [string](Get-ObjectPropertyValue -Value $script:SurfaceAction -Name "actionId")
    $requestedPeriodValue = if ($actionId -eq "stock_period_commit") {
        Get-ActionPayloadValue -Action $script:SurfaceAction -Name "value" -DefaultValue $period
    }
    else {
        Get-ActionStateValue -Action $script:SurfaceAction -Name "period" -DefaultValue $period
    }
    $requestedPeriodText = ([string]$requestedPeriodValue).Trim().ToLowerInvariant()
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
