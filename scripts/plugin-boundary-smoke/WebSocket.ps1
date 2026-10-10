function New-LoomHookBridgeWebSocket {
    param([int]$Port, [ValidateRange(1, 10000)][int]$BudgetMs = 10000,
        [string]$Authorization = $script:LoomAuthorizationHeader)

    if ([string]::IsNullOrWhiteSpace($Authorization)) { throw 'Hook authentication is required' }

    $client = [System.Net.WebSockets.ClientWebSocket]::new()
    $uri = [Uri]::new("ws://127.0.0.1:$Port")
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $connectCts = [System.Threading.CancellationTokenSource]::new($BudgetMs)
    $connected = $false
    try {
        $client.Options.SetRequestHeader('Authorization', $Authorization)
        [void]$client.ConnectAsync($uri, $connectCts.Token).GetAwaiter().GetResult()
        $connected = $true
        $script:PluginBoundaryDiagnostic.operationCompleted = $true
    }
    finally {
        $script:PluginBoundaryDiagnostic.webSocketState = [string]$client.State
        $connectCts.Dispose()
        if (-not $connected) { $client.Dispose() }
    }
    return $client
}

function Send-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$Json,
        [ValidateRange(1, 10000)][int]$BudgetMs = 10000
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $script:PluginBoundaryDiagnostic.operationCompleted = $false
    $sendCts = [System.Threading.CancellationTokenSource]::new($BudgetMs)
    try {
        [void]$Client.SendAsync(
            [ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text,
            $true,
            $sendCts.Token
        ).GetAwaiter().GetResult()
        $script:PluginBoundaryDiagnostic.operationCompleted = $true
    }
    finally {
        $script:PluginBoundaryDiagnostic.webSocketState = [string]$Client.State
        $sendCts.Dispose()
    }
}

function Receive-LoomHookBridgeWebSocketJson {
    param([System.Net.WebSockets.ClientWebSocket]$Client, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)

    $buffer = New-Object byte[] 4096
    $builder = [System.Text.StringBuilder]::new()
    $script:PluginBoundaryDiagnostic.budgetMs = $BudgetMs
    $script:PluginBoundaryDiagnostic.operationCompleted = $false
    do {
        $receiveCts = [System.Threading.CancellationTokenSource]::new($BudgetMs)
        try {
            $result = $Client.ReceiveAsync(
                [ArraySegment[byte]]::new($buffer),
                $receiveCts.Token
            ).GetAwaiter().GetResult()
        }
        finally {
            $script:PluginBoundaryDiagnostic.webSocketState = [string]$Client.State
            $receiveCts.Dispose()
        }
        $script:PluginBoundaryDiagnostic.fragments++
        if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
            throw "Hook Bridge WebSocket closed before sending a JSON response."
        }
        [void]$builder.Append([System.Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count))
    } while (-not $result.EndOfMessage)
    $value = $builder.ToString() | ConvertFrom-Json
    $script:PluginBoundaryDiagnostic.messages++
    $script:PluginBoundaryDiagnostic.operationCompleted = $true
    return $value
}

function Close-LoomHookBridgeWebSocket {
    param([AllowNull()][System.Net.WebSockets.ClientWebSocket]$Client)

    if ($null -eq $Client) {
        return
    }
    try {
        $Client.Dispose()
    }
    catch {
    }
}


function Receive-LoomHookBridgeExecutionResult {
    param([System.Net.WebSockets.ClientWebSocket]$Client, [ValidateRange(1, 10000)][int]$BudgetMs = 10000)
    do {
        $hookExecution = Receive-LoomHookBridgeWebSocketJson -Client $Client -BudgetMs $BudgetMs
        $protocolVersionProperty = $hookExecution.PSObject.Properties["protocolVersion"]
        $requestIdProperty = $hookExecution.PSObject.Properties["requestId"]
        $statusProperty = $hookExecution.PSObject.Properties["status"]
    } while (
        $null -eq $protocolVersionProperty -or
        $null -eq $requestIdProperty -or
        $null -eq $statusProperty -or
        [string]$protocolVersionProperty.Value -ne "loom.hook.v1" -or
        [string]$requestIdProperty.Value -ne "execute:third-party-plugin" -or
        [string]::IsNullOrWhiteSpace([string]$statusProperty.Value)
    )
    return $hookExecution
}
