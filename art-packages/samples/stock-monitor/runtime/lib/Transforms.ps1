# Owns bounded favorite, history, order-book, and live-tape projections.

function Select-FirstBoundedValue {
    param(
        [AllowNull()][object]$Values,
        [int]$Limit
    )

    if ($null -eq $Values -or $Limit -le 0) { return @() }
    $selected = [System.Collections.Generic.List[object]]::new()
    foreach ($value in $Values) {
        $selected.Add($value)
        if ($selected.Count -ge $Limit) { break }
    }
    return $selected.ToArray()
}

function Select-LastBoundedValue {
    param(
        [AllowNull()][object]$Values,
        [int]$Limit
    )

    if ($null -eq $Values -or $Limit -le 0) { return @() }
    if ($Values -is [System.Collections.IList]) {
        $selected = [System.Collections.Generic.List[object]]::new([Math]::Min($Values.Count, $Limit))
        for ($index = [Math]::Max(0, $Values.Count - $Limit); $index -lt $Values.Count; $index += 1) {
            $selected.Add($Values[$index])
        }
        return $selected.ToArray()
    }
    $queue = [System.Collections.Generic.Queue[object]]::new($Limit)
    foreach ($value in $Values) {
        if ($queue.Count -ge $Limit) { [void]$queue.Dequeue() }
        $queue.Enqueue($value)
    }
    return $queue.ToArray()
}

function ConvertTo-FavoriteQuotes {
    param([AllowNull()][object]$Values)

    $quotes = [System.Collections.Generic.List[object]]::new()
    foreach ($value in @(Select-FirstBoundedValue -Values $Values -Limit 12)) {
        try { $code = Resolve-StockCode (Get-ObjectPropertyValue -Value $value -Name "code") }
        catch { continue }
        $name = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $value -Name "name") -MaxLength 128
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
            source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $value -Name "source") -MaxLength 32 -DefaultValue "eastmoney").ToLowerInvariant()
            observedAt = [string](Get-ObjectPropertyValue -Value $value -Name "observedAt")
            fetchedAt = [string](Get-ObjectPropertyValue -Value $value -Name "fetchedAt")
            stale = ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $value -Name "stale" -DefaultValue $false)
        })
    }
    return @($quotes.ToArray())
}

function ConvertTo-HistoryRows {
    param([AllowNull()][object]$Values)

    $rows = [System.Collections.Generic.List[object]]::new()
    foreach ($value in @(Select-LastBoundedValue -Values $Values -Limit $script:MaxHistoryRows)) {
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
        $source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $value -Name "source") -MaxLength 32 -DefaultValue "unknown").ToLowerInvariant()
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
    foreach ($value in @(Select-FirstBoundedValue -Values $Values -Limit $script:MaxOrderBookLevels)) {
        $price = Convert-NullableNumber (Get-ObjectPropertyValue -Value $value -Name "price")
        if ($null -eq $price -or $price -le 0) { continue }
        $levelValue = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $value -Name "level") -Digits 0
        $level = if ($null -eq $levelValue -or $levelValue -lt 1 -or $levelValue -gt $script:MaxOrderBookLevels) {
            $levels.Count + 1
        }
        else {
            [int]$levelValue
        }
        $levels.Add([ordered]@{
            level = $level
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
        source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $Value -Name "source") -MaxLength 32 -DefaultValue "xueqiu").ToLowerInvariant()
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
        isTrade = ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $Value -Name "isTrade" -DefaultValue $false)
        tradeSession = Convert-NullableNumber -Value (Get-ObjectPropertyValue -Value $Value -Name "tradeSession") -Digits 0
        observedAt = $observedAt
        fetchedAt = $normalizedFetchedAt
        ageSeconds = $ageSeconds
        maxAgeSeconds = $script:MaxLiveAgeSeconds
        stale = ($null -eq $ageSeconds) -or ($ageSeconds -gt $script:MaxLiveAgeSeconds)
        source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $Value -Name "source") -MaxLength 32 -DefaultValue "xueqiu").ToLowerInvariant()
    }
}
