# Owns MCP result extraction and authoritative quote fallback normalization.

function Get-SafeMcpErrorMessage {
    param([AllowNull()][object]$Value)

    $message = ConvertTo-BoundedText -Value $Value -MaxLength 400 -DefaultValue "上游服务请求失败"
    if ($message -match '(?i)\b(?:authorization|cookie|token|secret|password|credential|signature|api[-_ ]?key|access[-_ ]?key|session[-_ ]?id)\b' -or
        $message -match '(?i)bearer\s+\S+' -or
        $message -match '(?:[A-Za-z]:\\|\\\\[^\\\s]+\\|%[A-Za-z_][A-Za-z0-9_]*%[\\/]|/(?:home|users|var|etc|tmp|opt|root|srv)/|\b(?:HKLM|HKCU|HKEY_[A-Z_]+)\\)') {
        return "上游服务请求失败"
    }
    return $message
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
    if (ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $result -Name "isError" -DefaultValue $false)) {
        $response = Get-ObjectPropertyValue -Value $structured -Name "response"
        $message = Get-SafeMcpErrorMessage (Get-ObjectPropertyValue -Value $response -Name "message" -DefaultValue "未知错误")
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
    $name = ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $quote -Name "name") -MaxLength 128
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
        source = (ConvertTo-BoundedText -Value (Get-ObjectPropertyValue -Value $quote -Name "source") -MaxLength 32 -DefaultValue "eastmoney").ToLowerInvariant()
        observedAt = [string](Get-ObjectPropertyValue -Value $quote -Name "observedAt")
        fetchedAt = [string](Get-ObjectPropertyValue -Value $quote -Name "fetchedAt")
        stale = ConvertTo-StrictBoolean (Get-ObjectPropertyValue -Value $quote -Name "stale" -DefaultValue $false)
    }
}
