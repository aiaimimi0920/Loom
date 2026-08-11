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
    $allowedActions = @("dashboard_refresh", "dashboard_toggle")
    if ($actionId -notin $allowedActions) { throw "action is not declared by the dashboard prototype: $actionId" }

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
        "dashboard_refresh" {
            $png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mP4DwQACfsD/Wj6HMwAAAAASUVORK5CYII="
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                resourceUploads = [object[]]@([ordered]@{
                    id = "dashboard-chart"
                    kind = "image"
                    mime = "image/png"
                    dataBase64 = $png
                    width = 1
                    height = 1
                    leaseMillis = 900000
                })
                patches = [object[]]@(
                    [ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "sync_progress" -Path "/props/value" -Value 0.35),
                            (New-SetOperation -NodeId "sync_status" -Path "/props/text" -Value "正在同步设备")
                        )
                        statePatch = [ordered]@{}
                    },
                    [ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "sync_progress" -Path "/props/value" -Value 0.75),
                            (New-SetOperation -NodeId "device_2" -Path "/props/text" -Value "平板 · 在线")
                        )
                        statePatch = [ordered]@{}
                    },
                    [ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "sync_progress" -Path "/props/value" -Value 1.0),
                            (New-SetOperation -NodeId "sync_status" -Path "/props/text" -Value "全部同步完成"),
                            (New-SetOperation -NodeId "chart" -Path "/props/resourceId" -Value "surface-upload:dashboard-chart")
                        )
                        statePatch = [ordered]@{ refreshCount = 1; status = "ready" }
                    }
                )
                result = [ordered]@{
                    outputs = [ordered]@{
                        dashboard = [ordered]@{ kind = "value"; value = [ordered]@{ online = 3; status = "ready" } }
                    }
                    statePatch = [ordered]@{ status = "ready" }
                }
            })
        }
        "dashboard_toggle" {
            $enabled = [bool](Get-PayloadValue -Action $action -Name "value" -Default $true)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@((New-SetOperation -NodeId "auto_sync" -Path "/props/value" -Value $enabled))
                    statePatch = [ordered]@{ autoSync = $enabled }
                })
            })
        }
        "form_validate" {
            $name = [string](Get-PayloadValue -Action $action -Name "value" -Default "")
            $valid = -not [string]::IsNullOrWhiteSpace($name)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "project_name" -Path "/props/value" -Value $name),
                        (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value (-not $valid)),
                        (New-SetOperation -NodeId "name_error" -Path "/props/text" -Value $(if ($valid) { "" } else { "项目名称为必填项" }))
                    )
                    statePatch = [ordered]@{ projectName = $name; valid = $valid }
                })
            })
        }
        "form_submit" {
            $name = [string](Get-StateValue -Action $action -Name "projectName" -Default "")
            if ([string]::IsNullOrWhiteSpace($name)) {
                Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                    protocolVersion = "loom.surface.v1"
                    patches = [object[]]@([ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value $true),
                            (New-SetOperation -NodeId "name_error" -Path "/props/text" -Value "项目名称为必填项")
                        )
                        statePatch = [ordered]@{ valid = $false }
                    })
                })
            } else {
                Start-Sleep -Milliseconds 450
                Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                    protocolVersion = "loom.surface.v1"
                    patches = [object[]]@([ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "form_status" -Path "/props/text" -Value "提交成功"),
                            (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value $false)
                        )
                        statePatch = [ordered]@{ submitted = $true; valid = $true }
                    })
                    result = [ordered]@{
                        outputs = [ordered]@{
                            submission = [ordered]@{ kind = "value"; value = [ordered]@{ projectName = $name; submitted = $true } }
                        }
                        statePatch = [ordered]@{ submitted = $true }
                    }
                })
            }
        }
        "form_cancel" {
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "project_name" -Path "/props/value" -Value ""),
                        (New-SetOperation -NodeId "notes" -Path "/props/value" -Value ""),
                        (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value $false),
                        (New-SetOperation -NodeId "form_status" -Path "/props/text" -Value "已取消")
                    )
                    statePatch = [ordered]@{ projectName = ""; notes = ""; submitted = $false; valid = $false }
                })
            })
        }
        default { throw "unknown Surface prototype action: $actionId" }
    }
}
catch {
    Write-SurfaceError -Code "surface_prototype_failed" -Message $_.Exception.Message
}
