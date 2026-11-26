# Owns fixed-position Surface action parsing, correlation echoes, and action budgets.

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

function Read-BoundedStandardInput {
    # Read raw UTF-8 so the limit is measured on the protocol bytes, not UTF-16 characters. The
    # standard-input handle stays host-owned; only the bounded accumulator is disposed here.
    param([int]$MaxBytes = $script:MaxRequestBytes)

    $inputStream = [Console]::OpenStandardInput()
    $memory = [System.IO.MemoryStream]::new()
    $buffer = [byte[]]::new(8192)
    $tooLarge = $false
    try {
        while (($read = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            if ($tooLarge) { continue }
            if (($memory.Length + $read) -gt $MaxBytes) {
                $tooLarge = $true
                $memory.SetLength(0)
                continue
            }
            $memory.Write($buffer, 0, $read)
        }
        if ($tooLarge) { throw "Stock Monitor request exceeds $MaxBytes bytes" }
        $utf8 = [System.Text.UTF8Encoding]::new($false, $true)
        $bytes = $memory.GetBuffer()
        $offset = if ($memory.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { 3 } else { 0 }
        return $utf8.GetString($bytes, $offset, [int]$memory.Length - $offset)
    }
    finally {
        $memory.Dispose()
    }
}

function Assert-JsonTextDepth {
    # Reject hostile nesting before Windows PowerShell's JSON parser sees it. The decoded-object check
    # below remains authoritative for node count and for scalar leaves at the maximum container depth.
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [int]$MaxDepth = $script:MaxJsonDepth
    )

    $depth = 0
    $inString = $false
    $escaped = $false
    foreach ($character in $Value.ToCharArray()) {
        if ($inString) {
            if ($escaped) {
                $escaped = $false
                continue
            }
            if ($character -eq [char]92) {
                $escaped = $true
                continue
            }
            if ($character -eq [char]34) { $inString = $false }
            continue
        }
        if ($character -eq [char]34) {
            $inString = $true
            continue
        }
        if ($character -eq [char]123 -or $character -eq [char]91) {
            $depth += 1
            if ($depth -gt $MaxDepth) { throw "Stock Monitor request exceeds JSON depth $MaxDepth" }
        }
        elseif ($character -eq [char]125 -or $character -eq [char]93) {
            $depth -= 1
        }
    }
}

function Assert-RequestObjectGraph {
    # Windows PowerShell 5.1 has no ConvertFrom-Json -Depth switch. Validate the decoded graph
    # iteratively so nesting and total node count remain explicit and recursion cannot overflow.
    param([AllowNull()][object]$Value)

    $stack = [System.Collections.Generic.Stack[object]]::new()
    $stack.Push([Tuple[object, int]]::new([object]$Value, 1))
    $nodes = 0
    while ($stack.Count -gt 0) {
        $entry = [Tuple[object, int]]$stack.Pop()
        $current = $entry.Item1
        $depth = $entry.Item2
        $nodes += 1
        if ($nodes -gt $script:MaxRequestNodes) { throw "Stock Monitor request has too many JSON values" }
        if ($depth -gt $script:MaxJsonDepth) { throw "Stock Monitor request exceeds JSON depth $($script:MaxJsonDepth)" }
        if ($null -eq $current -or $current -is [string] -or $current.GetType().IsPrimitive) { continue }
        if ($current -is [System.Collections.IDictionary]) {
            foreach ($child in $current.Values) { $stack.Push([Tuple[object, int]]::new([object]$child, $depth + 1)) }
        }
        elseif ($current -is [System.Collections.IList]) {
            foreach ($child in $current) { $stack.Push([Tuple[object, int]]::new([object]$child, $depth + 1)) }
        }
        elseif ($current -is [pscustomobject]) {
            foreach ($property in $current.PSObject.Properties) {
                $stack.Push([Tuple[object, int]]::new([object]$property.Value, $depth + 1))
            }
        }
    }
}

function Resolve-SurfaceAction {
    # Resolve surfaceAction only from three fixed positions: request root, params, and inputs.
    # The host (framework-packages/runtime-host/src/mcp.rs find_surface_action) accepts params and
    # inputs while local test fixtures place it at the root; never search recursively, otherwise an
    # upstream MCP payload under frameworkData.mcp.results containing a surfaceAction key could
    # fabricate an action invocation.
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
        # PSCustomObject -ne checks reference equality, so compare normalized JSON text to detect
        # genuine conflicts across duplicate placements.
        $candidateText = $candidate | ConvertTo-Json -Depth $script:MaxJsonDepth -Compress
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
    if ($value -isnot [string]) { return $null }
    $textValue = [string]$value
    $textValue = $textValue.Trim()
    if ($textValue.Length -eq 0) { return $null }
    if ($textValue -match '[\x00-\x1f\x7f]') { return $null }
    try {
        $null = [System.Text.UTF8Encoding]::new($false, $true).GetByteCount($textValue)
    }
    catch {
        return $null
    }
    if ($textValue.Length -gt 64) {
        $textValue = $textValue.Substring(0, 64)
        if ([char]::IsHighSurrogate($textValue[$textValue.Length - 1])) {
            $textValue = $textValue.Substring(0, $textValue.Length - 1)
        }
    }
    return $textValue
}

function Get-SurfaceActionBudgets {
    # Surface has no direct action-budget channel and host timeouts do not emit patches, so publish
    # each manifest-declared timeoutMs (clamped by art.runtime.json limits.timeoutMs) in state so
    # the client can schedule its fallback deadline after the host gives up.
    if ($null -ne $script:SurfaceActionBudgets) { return $script:SurfaceActionBudgets }
    $budgets = [ordered]@{}
    try {
        $packageRoot = Split-Path -Parent $script:StockMonitorRuntimeRoot
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
    # Every statePatch must carry correlation echo fields: statePatch merges into authoritativeState,
    # so leaving a stale lastRequestId from an earlier action would prevent client correlation from
    # matching and leave pending controls locked until the fallback timer expires.
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
