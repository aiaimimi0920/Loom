$ErrorActionPreference = "Stop"

function New-UnicodeText {
    param([int[]]$CodePoints)
    return -join ($CodePoints | ForEach-Object { [char]$_ })
}

$script:RequiredNameLabel = New-UnicodeText @(0x9879, 0x76EE, 0x540D, 0x79F0, 0x4E3A, 0x5FC5, 0x586B, 0x9879)
$script:SubmittedLabel = New-UnicodeText @(0x63D0, 0x4EA4, 0x6210, 0x529F)
$script:CancelledLabel = New-UnicodeText @(0x5DF2, 0x53D6, 0x6D88)

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
    $allowedActions = @("form_validate", "form_notes_input", "form_submit", "form_cancel")
    if ($actionId -notin $allowedActions) { throw "action is not declared by the form prototype: $actionId" }

    switch ($actionId) {
        "form_validate" {
            $name = [string](Get-PayloadValue -Action $action -Name "value" -Default "")
            $valid = -not [string]::IsNullOrWhiteSpace($name)
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@(
                        (New-SetOperation -NodeId "project_name" -Path "/props/value" -Value $name),
                        (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value (-not $valid)),
                        (New-SetOperation -NodeId "name_error" -Path "/props/text" -Value $(if ($valid) { "" } else { $script:RequiredNameLabel }))
                    )
                    statePatch = [ordered]@{ projectName = $name; valid = $valid }
                })
            })
        }
        "form_notes_input" {
            $notes = [string](Get-PayloadValue -Action $action -Name "value" -Default "")
            Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                protocolVersion = "loom.surface.v1"
                patches = [object[]]@([ordered]@{
                    operations = [object[]]@((New-SetOperation -NodeId "notes" -Path "/props/value" -Value $notes))
                    statePatch = [ordered]@{ notes = $notes }
                })
            })
        }
        "form_submit" {
            $name = [string](Get-StateValue -Action $action -Name "projectName" -Default "")
            $notes = [string](Get-StateValue -Action $action -Name "notes" -Default "")
            if ([string]::IsNullOrWhiteSpace($name)) {
                Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                    protocolVersion = "loom.surface.v1"
                    patches = [object[]]@([ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value $true),
                            (New-SetOperation -NodeId "name_error" -Path "/props/text" -Value $script:RequiredNameLabel)
                        )
                        statePatch = [ordered]@{ valid = $false }
                    })
                })
            } else {
                Start-Sleep -Milliseconds 1500
                Write-SurfaceSuccess -SurfaceAction ([ordered]@{
                    protocolVersion = "loom.surface.v1"
                    patches = [object[]]@([ordered]@{
                        operations = [object[]]@(
                            (New-SetOperation -NodeId "form_status" -Path "/props/text" -Value $script:SubmittedLabel),
                            (New-SetOperation -NodeId "name_error" -Path "/props/visible" -Value $false)
                        )
                        statePatch = [ordered]@{ submitted = $true; valid = $true }
                    })
                    result = [ordered]@{
                        outputs = [ordered]@{
                            submission = [ordered]@{ kind = "value"; value = [ordered]@{ projectName = $name; notes = $notes; submitted = $true } }
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
                        (New-SetOperation -NodeId "form_status" -Path "/props/text" -Value $script:CancelledLabel)
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
