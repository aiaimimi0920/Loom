[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
. (Join-Path $repoRoot "scripts\plugin-boundary-smoke\Diagnostics.ps1")
. (Join-Path $repoRoot "scripts\plugin-boundary-smoke\WebSocket.ps1")
$script:LoomAuthorizationHeader = 'Bearer fixture'
$baseUrl = 'http://127.0.0.1:19819'
$script:BrokerMode = 'normal'
# Only the HTTP broker receives the administrator header. Both smoke helpers
# must obtain the scoped token instead of reusing that header on the socket.
function Invoke-LoomJson {
    param($Method, $Url, $Body)
    Assert-Diagnostic ($Method -eq 'Post' -and $Url -eq "$baseUrl/v1/hook-bridge/credentials") 'Wrong credential broker request.'
    if ($script:BrokerMode -eq 'unavailable') { throw 'Hook broker unavailable' }
    if ($script:BrokerMode -eq 'mismatch') { return @{ url = 'ws://127.0.0.1:1'; token = 'hook-v1.invalid' } }
    return @{ url = "ws://127.0.0.1:$($server.Port)"; token = 'hook-v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA' }
}
function Invoke-JsonPost {
    param($Uri, $Body)
    return Invoke-LoomJson -Method Post -Url $Uri -Body $Body
}
Add-Type -Path (Join-Path $PSScriptRoot "plugin-boundary\WebSocketFixture.cs")
$root = Join-Path ([IO.Path]::GetTempPath()) "loom-plugin-diagnostic-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $root | Out-Null
$diagnosticPath = Join-Path $root "plugin-boundary-diagnostic.json"
function Assert-Diagnostic([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Test-CredentialBrokerFailure {
    foreach ($mode in @('unavailable', 'mismatch')) {
        $script:BrokerMode = $mode
        $caught = $null
        try { $null = New-LoomHookBridgeWebSocket -Port 19820 -DaemonBaseUrl $baseUrl } catch { $caught = $_ }
        Assert-Diagnostic ($null -ne $caught -and $caught.Exception.Message -match 'broker|daemon-owned') 'Credential broker failure must reject before connecting.'
    }
    $script:BrokerMode = 'normal'
}

function Test-SocketFailure([string]$Scenario, [string]$Phase, [int]$MinimumFragments = 0) {
    $server = [PluginBoundarySocketFixture]::new($Scenario)
    $client = $null
    $caught = $null
    try {
        Set-LoomPluginBoundaryPhase -Phase "connect"
        if ($Scenario -eq "connect-timeout") {
            $client = New-LoomHookBridgeWebSocket -Port $server.Port -Authorization 'Bearer fixture' -BudgetMs 100
        } else {
            $client = New-LoomHookBridgeWebSocket -Port $server.Port -Authorization 'Bearer fixture'
            Set-LoomPluginBoundaryPhase -Phase $Phase
            if ($Scenario -eq "send-timeout") {
                Send-LoomHookBridgeWebSocketJson -Client $client -Json ('x' * 16777216) -BudgetMs 100
            } else {
                $null = Receive-LoomHookBridgeWebSocketJson -Client $client -BudgetMs 100
            }
        }
    } catch {
        $caught = $_
        Save-LoomPluginBoundaryDiagnostic -EvidencePath $root -Failure $_
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
        $server.Dispose()
    }
    Assert-Diagnostic ($null -ne $caught) "$Scenario did not fail."
    $text = [IO.File]::ReadAllText($diagnosticPath)
    $record = $text | ConvertFrom-Json
    Assert-Diagnostic ($record.phase -eq $Phase) "$Scenario lost its exact phase."
    $expectedRequestId = if ($Phase -like 'subscribe-*') { 'subscribe:third-party-plugin' }
        elseif ($Phase -like 'execute-*') { 'execute:third-party-plugin' } else { '' }
    Assert-Diagnostic ($record.requestId -eq $expectedRequestId) "Fixed request identity missing."
    Assert-Diagnostic ($record.budgetMs -eq 100 -and $record.phaseElapsedMs -ge 0) "Budget or phase timing missing."
    Assert-Diagnostic ($record.fragments -ge $MinimumFragments) "Fragment count missing."
    Assert-Diagnostic (-not $record.operationCompleted) "Failure was reported as completed."
    Assert-Diagnostic (@($record.exceptionTypes).Count -gt 0 -and $record.sourceLine -gt 0) "Exception type or source line missing."
    Assert-Diagnostic ($record.webSocketState -in @('Open', 'CloseReceived', 'Closed', 'Aborted')) "Socket state missing."
    Assert-Diagnostic ([Text.Encoding]::UTF8.GetByteCount($text) -le 4096) "Diagnostic exceeded its size cap."
    Assert-Diagnostic (-not $text.Contains('private-payload') -and -not $text.Contains($root) -and -not $text.Contains('secret')) "Diagnostic leaked payload or path."
    if ($Scenario -like '*timeout') {
        # .NET Framework may wrap a canceled connect as WebSocketException.
        Assert-Diagnostic (($record.exceptionTypes -join ',') -match 'OperationCanceledException|TaskCanceledException|WebSocketException') "Cancellation type lost."
        Assert-Diagnostic ($record.phaseElapsedMs -ge 80 -and $record.phaseElapsedMs -lt 3000) "Cancellation did not follow the short test budget."
    }
    Write-Output "PASS $Scenario / $Phase"
}

try {
    Test-CredentialBrokerFailure
    Test-SocketFailure "connect-timeout" "connect"
    Test-SocketFailure "send-timeout" "subscribe-send"
    Test-SocketFailure "send-timeout" "execute-send"
    foreach ($phase in @('subscribe-receive', 'instantiation-receive', 'execute-receive')) {
        Test-SocketFailure "receive-timeout" $phase
    }
    Test-SocketFailure "fragment-timeout" "execute-receive" 1
    Test-SocketFailure "close" "execute-receive" 1
    Test-SocketFailure "bad-json" "subscribe-receive" 1

    foreach ($scenario in @('normal', 'unrelated')) {
        $server = [PluginBoundarySocketFixture]::new($scenario)
        $client = $null
        try {
            Set-LoomPluginBoundaryPhase -Phase "connect"
            $client = New-LoomHookBridgeWebSocket -Port $server.Port
            Assert-Diagnostic $server.ReceivedAuthorization 'Plugin smoke omitted its configured bearer credential.'
            Assert-Diagnostic ($script:PluginBoundaryDiagnostic.budgetMs -eq 10000) "Production connect budget changed."
            if ($scenario -eq 'normal') {
                Set-LoomPluginBoundaryPhase -Phase "subscribe-send"
                Send-LoomHookBridgeWebSocketJson -Client $client -Json '{"requestId":"test"}'
                Assert-Diagnostic ($script:PluginBoundaryDiagnostic.budgetMs -eq 10000 -and $script:PluginBoundaryDiagnostic.operationCompleted) "Production send budget or completion changed."
                Set-LoomPluginBoundaryPhase -Phase "subscribe-receive"
                $result = Receive-LoomHookBridgeWebSocketJson -Client $client
                Assert-Diagnostic ($server.ReceivedMessages -eq 1) "Send was retried or omitted."
                Assert-Diagnostic ($script:PluginBoundaryDiagnostic.fragments -ge 2) "Fragmented JSON not reassembled."
            } else {
                Set-LoomPluginBoundaryPhase -Phase "execute-receive"
                $result = Receive-LoomHookBridgeExecutionResult -Client $client
                Assert-Diagnostic ($result.requestId -eq "execute:third-party-plugin") "Unrelated event was accepted as execution result."
                Assert-Diagnostic ($script:PluginBoundaryDiagnostic.messages -eq 3) "Unrelated events were not consumed."
            }
            Assert-Diagnostic ($result.status -eq "succeeded") "Successful response changed."
            Assert-Diagnostic ($script:PluginBoundaryDiagnostic.budgetMs -eq 10000) "Production receive budget changed."
            Assert-Diagnostic ($script:PluginBoundaryDiagnostic.operationCompleted) "Successful receive not recorded."
        } finally {
            Close-LoomHookBridgeWebSocket -Client $client
            $server.Dispose()
        }
        Write-Output "PASS $scenario"
    }
    . (Join-Path $repoRoot 'scripts\framework-art-store-hook-smoke\HookBridge.ps1')
    Test-CredentialBrokerFailure
    $script:DaemonRequestHeaders = @{ Authorization = 'Bearer fixture' }
    $server = [PluginBoundarySocketFixture]::new('normal')
    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $server.Port
        Assert-Diagnostic $server.ReceivedAuthorization 'Framework smoke omitted its configured bearer credential.'
    } finally {
        if ($null -ne $client) { $client.Dispose() }
        $server.Dispose()
    }
    Reset-LoomPluginBoundaryDiagnostic
    try { throw ('private-payload' * 10000) } catch {
        Save-LoomPluginBoundaryDiagnostic -EvidencePath $root -Failure $_
    }
    $largeError = [IO.File]::ReadAllText($diagnosticPath)
    Assert-Diagnostic ([Text.Encoding]::UTF8.GetByteCount($largeError) -le 4096 -and -not $largeError.Contains('private-payload')) "Large exception message escaped the bounded schema."
    Assert-Diagnostic (($largeError | ConvertFrom-Json).phase -eq 'outside-websocket') "Non-WebSocket error inherited the last completed phase."
    $original = $null
    try {
        try { throw "private-original-failure" } catch {
            $original = $_
            Save-LoomPluginBoundaryDiagnostic -EvidencePath (Join-Path $root 'missing') -Failure $_ -WarningAction SilentlyContinue
            throw
        }
    } catch {
        Assert-Diagnostic ([object]::ReferenceEquals($original.Exception, $_.Exception)) "Diagnostic write failure replaced original exception."
    }
} finally {
    $resolvedRoot = [IO.Path]::GetFullPath($root)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolvedRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe test cleanup path." }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
Write-Output "Plugin boundary diagnostics: 13 loopback/privacy/error cases and both authenticated smoke clients passed."
