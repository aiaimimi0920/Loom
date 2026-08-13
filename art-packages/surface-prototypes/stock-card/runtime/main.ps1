$ErrorActionPreference = "Stop"

function Find-SurfaceAction {
    param([object]$Value)
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
        if ($null -ne $Value.PSObject.Properties["surfaceAction"]) { return $Value.surfaceAction }
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

function Get-PayloadValue {
    param([object]$Action, [string]$Name, [object]$Default)
    if ($null -ne $Action.payload -and $null -ne $Action.payload.PSObject.Properties[$Name]) {
        return $Action.payload.$Name
    }
    return $Default
}

function Get-StateValue {
    param([object]$Action, [string]$Name, [object]$Default)
    if ($null -ne $Action.authoritativeState -and $null -ne $Action.authoritativeState.PSObject.Properties[$Name]) {
        return $Action.authoritativeState.$Name
    }
    return $Default
}

function New-SetOperation {
    param([string]$NodeId, [string]$Path, [object]$Value)
    return [ordered]@{ op = "set"; nodeId = $NodeId; path = $Path; value = $Value }
}

function Write-SurfaceSuccess {
    param([hashtable]$SurfaceAction)
    $response = [ordered]@{ status = "success"; output = [ordered]@{ surfaceAction = $SurfaceAction } }
    [Console]::Out.Write(($response | ConvertTo-Json -Depth 80 -Compress))
}

function Write-SurfaceError {
    param([string]$Code, [string]$Message)
    $response = [ordered]@{
        status = "error"
        error = [ordered]@{ code = $Code; message = $Message }
    }
    [Console]::Out.Write(($response | ConvertTo-Json -Depth 20 -Compress))
}

try {
    $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
    $action = Find-SurfaceAction -Value $request
    if ($null -eq $action) { throw "surfaceAction invocation is required" }
    $actionId = [string]$action.actionId
    $allowedActions = @(
        "stock_symbol_input",
        "stock_symbol_commit",
        "stock_interval_preview",
        "stock_interval_commit",
        "stock_refresh"
    )
    if ($actionId -notin $allowedActions) { throw "action is not declared by the stock card prototype: $actionId" }

    switch ($actionId) {
        "stock_symbol_input" {
            $symbol = [string](Get-PayloadValue -Action $action -Name "value" -Default "MSFT")
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $symbol),
                        (New-SetOperation -NodeId "symbol_hint" -Path "/props/text" -Value "正在输入 $symbol")
                    )
                    statePatch = [ordered]@{ draftSymbol = $symbol }
                })
            })
        }
        "stock_symbol_commit" {
            $symbol = [string](Get-PayloadValue -Action $action -Name "value" -Default "MSFT")
            $symbol = $symbol.Trim().ToUpperInvariant()
            if ([string]::IsNullOrWhiteSpace($symbol)) { $symbol = "MSFT" }
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "symbol" -Path "/props/value" -Value $symbol),
                        (New-SetOperation -NodeId "symbol_hint" -Path "/props/text" -Value "已选择 $symbol")
                    )
                    statePatch = [ordered]@{ symbol = $symbol; draftSymbol = $symbol }
                })
            })
        }
        "stock_interval_preview" {
            $interval = [int](Get-PayloadValue -Action $action -Name "value" -Default 3)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value $interval),
                        (New-SetOperation -NodeId "interval_label" -Path "/props/text" -Value "预览：$interval 秒")
                    )
                    statePatch = [ordered]@{ draftInterval = $interval }
                })
            })
        }
        "stock_interval_commit" {
            $interval = [int](Get-PayloadValue -Action $action -Name "value" -Default 3)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "interval" -Path "/props/value" -Value $interval),
                        (New-SetOperation -NodeId "interval_label" -Path "/props/text" -Value "每 $interval 秒刷新")
                    )
                    statePatch = [ordered]@{ interval = $interval; draftInterval = $interval }
                })
            })
        }
        "stock_refresh" {
            $symbol = [string](Get-StateValue -Action $action -Name "symbol" -Default "MSFT")
            $oldPrice = [double](Get-StateValue -Action $action -Name "price" -Default 101.20)
            $first = [Math]::Round($oldPrice + 0.11, 2)
            $second = [Math]::Round($oldPrice + 0.24, 2)
            $final = [Math]::Round($oldPrice + 0.37, 2)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@(
                    [ordered]@{
                        operations = [object[]]@((New-SetOperation -NodeId "price" -Path "/props/text" -Value ("¥{0:N2}" -f $first)))
                        statePatch = [ordered]@{}
                    },
                    [ordered]@{
                        operations = [object[]]@((New-SetOperation -NodeId "price" -Path "/props/text" -Value ("¥{0:N2}" -f $second)))
                        statePatch = [ordered]@{}
                    },
                    [ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "price" -Path "/props/text" -Value ("¥{0:N2}" -f $final)),
                            (New-SetOperation -NodeId "status" -Path "/props/text" -Value "行情已更新")
                        )
                        statePatch = [ordered]@{ price = $final }
                    }
                )
                result = [ordered]@{
                    outputs = [ordered]@{
                        quote = [ordered]@{ kind = "value"; value = [ordered]@{ symbol = $symbol; price = $final; currency = "CNY" } }
                    }
                    statePatch = [ordered]@{ price = $final }
                }
            })
        }
        default { throw "unknown Surface prototype action: $actionId" }
    }
}
catch {
    Write-SurfaceError -Code "surface_prototype_failed" -Message $_.Exception.Message
}
