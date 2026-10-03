# Closed-schema diagnostics: no server payloads, exception messages, paths, or logs.
function Reset-LoomPluginBoundaryDiagnostic {
    $script:PluginBoundaryDiagnostic = @{
        phase = "outside-websocket"; requestId = ""; budgetMs = 10000
        elapsed = [Diagnostics.Stopwatch]::StartNew(); fragments = 0L
        messages = 0L; webSocketState = "None"; operationCompleted = $false
    }
}

function Set-LoomPluginBoundaryPhase {
    param(
        [ValidateSet("connect", "subscribe-send", "subscribe-receive", "instantiation-receive", "execute-send", "execute-receive")]
        [string]$Phase
    )
    Reset-LoomPluginBoundaryDiagnostic
    $script:PluginBoundaryDiagnostic.phase = $Phase
    $script:PluginBoundaryDiagnostic.requestId = switch -Wildcard ($Phase) {
        "subscribe-*" { "subscribe:third-party-plugin" }
        "execute-*" { "execute:third-party-plugin" }
        default { "" }
    }
}

function Save-LoomPluginBoundaryDiagnostic {
    param([string]$EvidencePath, [System.Management.Automation.ErrorRecord]$Failure)
    # Diagnostic failure must never replace the original smoke exception.
    try {
        $state = $script:PluginBoundaryDiagnostic
        $types = @()
        $exception = $Failure.Exception
        for ($index = 0; $null -ne $exception -and $index -lt 4; $index++) {
            $name = $exception.GetType().FullName
            if ($name -match '^[A-Za-z0-9_.+`]{1,160}$') { $types += $name }
            $exception = $exception.InnerException
        }
        $source = [IO.Path]::GetFileName($Failure.InvocationInfo.ScriptName)
        if ($source -notin @("Invoke-LoomPluginBoundarySmoke.ps1", "WebSocket.ps1")) { $source = "unknown" }
        $record = [ordered]@{
            schemaVersion = 1
            phase = $state.phase
            requestId = $state.requestId
            budgetMs = $state.budgetMs
            budgetScope = "per-websocket-operation-or-fragment"
            phaseElapsedMs = $state.elapsed.ElapsedMilliseconds
            webSocketState = $state.webSocketState
            fragments = $state.fragments
            messages = $state.messages
            operationCompleted = $state.operationCompleted
            exceptionTypes = $types
            sourceFile = $source
            sourceLine = $Failure.InvocationInfo.ScriptLineNumber
        }
        $json = $record | ConvertTo-Json -Depth 4 -Compress
        if ([Text.Encoding]::UTF8.GetByteCount($json) -gt 4096) { throw "Diagnostic size limit exceeded." }
        [IO.File]::WriteAllText((Join-Path $EvidencePath "plugin-boundary-diagnostic.json"), $json, [Text.UTF8Encoding]::new($false))
    } catch {
        Write-Warning "Plugin boundary diagnostic unavailable."
    }
}
