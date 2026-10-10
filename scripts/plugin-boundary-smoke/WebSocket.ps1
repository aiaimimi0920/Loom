. (Join-Path $PSScriptRoot '..\NativeBridgeProbe.ps1')

function New-LoomHookBridgeWebSocket {
    param([int]$Port, [string]$ManifestPath, [string]$ProbeExecutable,
        [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    if ($Port -lt 1 -or $Port -gt 65535) { throw 'Hook Bridge port is out of range.' }
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $script:PluginBoundaryDiagnostic.operationCompleted = $false
    $client = Start-LoomNativeBridgeProbe -Executable $ProbeExecutable -ManifestPath $ManifestPath -BudgetMs $BudgetMs
    $script:PluginBoundaryDiagnostic.webSocketState = $client.State
    $script:PluginBoundaryDiagnostic.operationCompleted = $true
    return $client
}

function Send-LoomHookBridgeWebSocketJson {
    param([object]$Client, [string]$Json, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    if ([Text.Encoding]::UTF8.GetByteCount($Json) -gt 1MB) { throw 'Hook Bridge request exceeds the 1 MiB smoke limit.' }
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $script:PluginBoundaryDiagnostic.operationCompleted = $false
    try {
        [void](Invoke-LoomNativeBridgeProbe -Client $Client -BudgetMs $BudgetMs -Command @{
            op = 'send'; payload = $Json; timeoutMs = $BudgetMs
        })
        $script:PluginBoundaryDiagnostic.operationCompleted = $true
    } finally { $script:PluginBoundaryDiagnostic.webSocketState = $Client.State }
}

function Receive-LoomHookBridgeWebSocketJson {
    param([object]$Client, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $script:PluginBoundaryDiagnostic.operationCompleted = $false
    try {
        $response = Invoke-LoomNativeBridgeProbe -Client $Client -BudgetMs $BudgetMs -Command @{
            op = 'receive'; timeoutMs = $BudgetMs; maxBytes = 1MB
        }
        $script:PluginBoundaryDiagnostic.messages++
        $script:PluginBoundaryDiagnostic.operationCompleted = $true
        return $response.payload
    } finally { $script:PluginBoundaryDiagnostic.webSocketState = $Client.State }
}

function Close-LoomHookBridgeWebSocket {
    param([AllowNull()][object]$Client)
    Stop-LoomNativeBridgeProbe -Client $Client
}

function Receive-LoomHookBridgeExecutionResult {
    param([object]$Client, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $remaining = $BudgetMs - [int]$watch.ElapsedMilliseconds
        if ($remaining -lt 1) { break }
        $message = Receive-LoomHookBridgeWebSocketJson -Client $Client -BudgetMs $remaining
        $protocol = $message.PSObject.Properties['protocolVersion']
        $requestId = $message.PSObject.Properties['requestId']
        $status = $message.PSObject.Properties['status']
        if ($null -ne $protocol -and $null -ne $requestId -and $null -ne $status -and
            [string]$protocol.Value -eq 'loom.hook.v1' -and
            [string]$requestId.Value -eq 'execute:third-party-plugin' -and
            -not [string]::IsNullOrWhiteSpace([string]$status.Value)) { return $message }
    }
    throw 'Hook Bridge execution response exceeded its budget.'
}
