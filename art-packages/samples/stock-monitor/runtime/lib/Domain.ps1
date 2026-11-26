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
        throw "Invalid stock code format; unified codes such as SZ000034, SH600519, BJ430047, HK00700, and USAAPL are supported"
    }
    $input = ([string]$Value).Trim().ToUpperInvariant().Replace(" ", "")
    if ([string]::IsNullOrWhiteSpace($input)) {
        throw "Enter a stock code, such as SZ000034, SH600519, BJ430047, HK00700, or USAAPL"
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
    throw "Invalid stock code format; unified codes such as SZ000034, SH600519, BJ430047, HK00700, and USAAPL are supported"
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
        "tencent" { return "Tencent Quotes" }
        "sina" { return "Sina Finance" }
        "eastmoney" { return "Eastmoney" }
        "xueqiu" { return "Xueqiu" }
        "pysnowball" { return "pysnowball / Xueqiu" }
        "mixed" { return "pysnowball + Xueqiu" }
        default { return $Source }
    }
}

function Resolve-UtcTimestamp {
    # Upstream timestamps must carry an explicit UTC offset (Z or +/-HH[:]MM). Parsing naive local
    # strings with AssumeUniversal would misread a UTC+8 timestamp as UTC and understate record age
    # by 8 hours, making stale quotes appear fresh. Return $null when offset is missing so callers
    # fail closed as stale.
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
    # Return $null rather than [double]::PositiveInfinity when age is unknown: ConvertTo-Json emits
    # bare Infinity, which is invalid JSON and breaks host parsing. Callers must check $null
    # explicitly because ($null -gt 90) evaluates to $false in PowerShell.
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
        "minute" { return "Intraday" }
        "five-day" { return "5D" }
        "day" { return "1D" }
        "week" { return "1W" }
        "month" { return "1M" }
        "quarter" { return "1Q" }
        "year" { return "1Y" }
        "minute-120" { return "120m" }
        "minute-60" { return "60m" }
        "minute-30" { return "30m" }
        "minute-15" { return "15m" }
        "minute-5" { return "5m" }
        "minute-1" { return "1m" }
        default { return "1D" }
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
