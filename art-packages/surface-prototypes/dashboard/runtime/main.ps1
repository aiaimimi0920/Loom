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

function Write-Utf8JsonLine {
    param([object]$Value, [int]$Depth = 80)
    $json = ($Value | ConvertTo-Json -Depth $Depth -Compress) + [Environment]::NewLine
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
    $stdout = [Console]::OpenStandardOutput()
    $stdout.Write($bytes, 0, $bytes.Length)
    $stdout.Flush()
}

function Write-SurfaceSuccess {
    param([hashtable]$SurfaceAction)
    $response = [ordered]@{ status = "success"; output = [ordered]@{ surfaceAction = $SurfaceAction } }
    Write-Utf8JsonLine -Value $response -Depth 80
}

function Write-SurfaceError {
    param([string]$Code, [string]$Message)
    $response = [ordered]@{
        status = "error"
        error = [ordered]@{ code = $Code; message = $Message }
    }
    Write-Utf8JsonLine -Value $response -Depth 20
}

try {
    $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
    $action = Find-SurfaceAction -Value $request
    if ($null -eq $action) { throw "surfaceAction invocation is required" }
    $actionId = [string]$action.actionId
    $allowedActions = @("dashboard_refresh", "dashboard_toggle")
    if ($actionId -notin $allowedActions) { throw "action is not declared by the dashboard prototype: $actionId" }

    switch ($actionId) {
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
        default { throw "unknown Surface prototype action: $actionId" }
    }
}
catch {
    Write-SurfaceError -Code "surface_prototype_failed" -Message $_.Exception.Message
}
