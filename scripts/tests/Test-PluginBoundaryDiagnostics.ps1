[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $repoRoot 'scripts\plugin-boundary-smoke\Diagnostics.ps1')
. (Join-Path $repoRoot 'scripts\plugin-boundary-smoke\WebSocket.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) "loom-plugin-diagnostic-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $root | Out-Null
$diagnosticPath = Join-Path $root 'plugin-boundary-diagnostic.json'
function Assert-Diagnostic([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

# These are facade/diagnostic unit tests. Real TLS, fragmentation, timeout and
# process cleanup are exercised by bridge_probe Rust tests and the native smoke.
function Start-LoomNativeBridgeProbe {
    param($Executable, $ManifestPath, $BudgetMs)
    if ($script:scenario -eq 'connect-failed') { throw 'secret private-payload connect error' }
    return [pscustomobject]@{ State = 'Open' }
}
function Stop-LoomNativeBridgeProbe {
    param($Client)
    if ($null -ne $Client) { $Client.State = 'Closed' }
}
function Invoke-LoomNativeBridgeProbe {
    param($Client, $Command, $BudgetMs)
    if ($script:scenario -eq 'operation-failed') {
        $Client.State = 'Closed'
        throw 'secret private-payload operation error'
    }
    if ($Command.op -eq 'send') { $script:sends++; return @{ ok = $true } }
    if ($script:messages.Count -eq 0) { throw 'No fixture response remains.' }
    return @{ ok = $true; payload = $script:messages.Dequeue() }
}

try {
    foreach ($phase in @('connect', 'subscribe-send', 'execute-send', 'subscribe-receive', 'instantiation-receive', 'execute-receive')) {
        $script:scenario = if ($phase -eq 'connect') { 'connect-failed' } else { 'operation-failed' }
        Set-LoomPluginBoundaryPhase -Phase $phase
        $client = [pscustomobject]@{ State = 'Open' }
        $caught = $null
        try {
            if ($phase -eq 'connect') { $null = New-LoomHookBridgeWebSocket -Port 1234 -ManifestPath 'fixture' -ProbeExecutable 'fixture' -BudgetMs 100 }
            elseif ($phase -like '*-send') { Send-LoomHookBridgeWebSocketJson -Client $client -Json '{}' -BudgetMs 100 }
            else { $null = Receive-LoomHookBridgeWebSocketJson -Client $client -BudgetMs 100 }
        } catch { $caught = $_; Save-LoomPluginBoundaryDiagnostic -EvidencePath $root -Failure $_ }
        Assert-Diagnostic ($null -ne $caught) "$phase did not fail."
        $text = [IO.File]::ReadAllText($diagnosticPath)
        $record = $text | ConvertFrom-Json
        Assert-Diagnostic ($record.phase -eq $phase -and $record.budgetMs -eq 100) 'Phase or budget lost.'
        Assert-Diagnostic (-not $record.operationCompleted) 'Failure reported as completed.'
        Assert-Diagnostic (@($record.exceptionTypes).Count -gt 0) 'Exception type lost.'
        Assert-Diagnostic ($record.budgetScope -eq 'per-native-operation-whole-message') 'Whole-message budget not recorded.'
        Assert-Diagnostic ([Text.Encoding]::UTF8.GetByteCount($text) -le 4096) 'Diagnostic size limit exceeded.'
        Assert-Diagnostic (-not $text.Contains('secret') -and -not $text.Contains('private-payload') -and -not $text.Contains($root)) 'Diagnostic leaked private data.'
    }
    $script:scenario = 'normal'
    $script:sends = 0
    $script:messages = [Collections.Queue]::new()
    $script:messages.Enqueue([pscustomobject]@{ method = 'loom.hook.art.progress' })
    $script:messages.Enqueue([pscustomobject]@{ protocolVersion = 'loom.hook.v1'; requestId = 'other'; status = 'succeeded' })
    $script:messages.Enqueue([pscustomobject]@{ protocolVersion = 'loom.hook.v1'; requestId = 'execute:third-party-plugin'; status = 'succeeded' })
    $client = New-LoomHookBridgeWebSocket -Port 1234 -ManifestPath 'fixture' -ProbeExecutable 'fixture'
    Set-LoomPluginBoundaryPhase -Phase 'execute-send'
    Send-LoomHookBridgeWebSocketJson -Client $client -Json '{}'
    Assert-Diagnostic ($script:sends -eq 1) 'Send was duplicated or omitted.'
    Set-LoomPluginBoundaryPhase -Phase 'execute-receive'
    $result = Receive-LoomHookBridgeExecutionResult -Client $client
    Assert-Diagnostic ($result.requestId -eq 'execute:third-party-plugin' -and $script:PluginBoundaryDiagnostic.messages -eq 3) 'Unrelated event filtering changed.'
    Close-LoomHookBridgeWebSocket -Client $client
    Assert-Diagnostic ($client.State -eq 'Closed') 'Facade did not close transport.'
    Reset-LoomPluginBoundaryDiagnostic
    try { throw ('private-payload' * 10000) } catch { Save-LoomPluginBoundaryDiagnostic -EvidencePath $root -Failure $_ }
    $text = [IO.File]::ReadAllText($diagnosticPath)
    Assert-Diagnostic ([Text.Encoding]::UTF8.GetByteCount($text) -le 4096 -and -not $text.Contains('private-payload')) 'Large exception escaped bounded schema.'
    $original = $null
    try {
        try { throw 'private-original-failure' } catch {
            $original = $_
            Save-LoomPluginBoundaryDiagnostic -EvidencePath (Join-Path $root 'missing') -Failure $_ -WarningAction SilentlyContinue
            throw
        }
    } catch { Assert-Diagnostic ([object]::ReferenceEquals($original.Exception, $_.Exception)) 'Diagnostic failure replaced original exception.' }
} finally {
    $resolved = [IO.Path]::GetFullPath($root)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
Write-Output 'Plugin boundary native facade, bounded diagnostics, privacy, and error-preservation unit tests passed.'
