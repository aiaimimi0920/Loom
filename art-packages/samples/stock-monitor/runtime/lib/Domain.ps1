# Owns stock, number, period, timestamp, and market-session normalization.

function ConvertTo-StrictBoolean {
    param(
        [AllowNull()][object]$Value,
        [bool]$DefaultValue = $false
    )

    if ($Value -is [bool]) { return $Value }
    return $DefaultValue
}

function ConvertTo-BoundedText {
    param(
        [AllowNull()][object]$Value,
        [int]$MaxLength,
        [AllowNull()][string]$DefaultValue = $null
    )

    if ($Value -isnot [string]) { return $DefaultValue }
    $text = ([string]$Value).Trim()
    if ([string]::IsNullOrWhiteSpace($text) -or $text.Length -gt $MaxLength -or $text -match '[\x00-\x1f\x7f]') {
        return $DefaultValue
    }
    return $text
}

function Convert-NullableNumber {
    param(
        [AllowNull()][object]$Value,
        [int]$Digits = 4
    )

    if ($null -eq $Value) { return $null }
    $typeCode = [Type]::GetTypeCode($Value.GetType())
    if ($typeCode -notin @(
        [TypeCode]::Byte, [TypeCode]::SByte, [TypeCode]::Int16, [TypeCode]::UInt16,
        [TypeCode]::Int32, [TypeCode]::UInt32, [TypeCode]::Int64, [TypeCode]::UInt64,
        [TypeCode]::Single, [TypeCode]::Double, [TypeCode]::Decimal, [TypeCode]::String
    )) { return $null }
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

    if ($Value -is [System.Collections.IDictionary] -or $Value -is [System.Collections.IList] -or
        $Value -is [pscustomobject] -or $Value -is [bool]) {
        throw "股票代码格式无效；支持 SZ000034、SH600519、BJ430047、HK00700 和 USAAPL 等统一代码"
    }
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

    if ($Value -isnot [string]) { return "day" }
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
