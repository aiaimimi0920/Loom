# Loom Hook Bridge WebSocket request, response and cleanup helpers.
function New-LoomHookBridgeWebSocket {
    param([int]$Port)

    if ($Port -lt 1 -or $Port -gt 65535) {
        throw "Hook Bridge port is out of range: $Port"
    }
    $client = [System.Net.WebSockets.ClientWebSocket]::new()
    $uri = [Uri]::new("ws://127.0.0.1:$Port")
    $connectCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$client.ConnectAsync($uri, $connectCts.Token).GetAwaiter().GetResult()
    } catch {
        $client.Dispose()
        throw
    } finally {
        $connectCts.Dispose()
    }
    return $client
}

function Send-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$Json
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
    if ($bytes.Length -gt 1MB) {
        throw "Hook Bridge request exceeds the 1 MiB smoke limit."
    }
    $sendCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$Client.SendAsync(
            [ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text,
            $true,
            $sendCts.Token
        ).GetAwaiter().GetResult()
    } finally {
        $sendCts.Dispose()
    }
}

function Receive-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [int]$TimeoutSeconds = 30,
        [int]$MaxMessageBytes = 1MB,
        [string]$Operation = "message"
    )

    if ($TimeoutSeconds -lt 1 -or $MaxMessageBytes -lt 1) {
        throw "Hook Bridge receive bounds must be positive."
    }
    $buffer = New-Object byte[] 8192
    $stream = [System.IO.MemoryStream]::new()
    $receiveCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds($TimeoutSeconds))
    try {
        do {
            try {
                $result = $Client.ReceiveAsync(
                    [ArraySegment[byte]]::new($buffer),
                    $receiveCts.Token
                ).GetAwaiter().GetResult()
            } catch {
                if ($_.Exception.ToString() -match "OperationCanceledException|TaskCanceledException|operation was canceled") {
                    throw "Hook Bridge $Operation receive timed out after $TimeoutSeconds seconds."
                }
                throw
            }

            if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
                throw "Hook Bridge WebSocket closed before sending a JSON response."
            }
            if ($result.MessageType -ne [System.Net.WebSockets.WebSocketMessageType]::Text) {
                throw "Hook Bridge WebSocket returned a non-text response."
            }
            if (($stream.Length + $result.Count) -gt $MaxMessageBytes) {
                throw "Hook Bridge response exceeds the $MaxMessageBytes-byte smoke limit."
            }
            $stream.Write($buffer, 0, $result.Count)
        } while (-not $result.EndOfMessage)

        $strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
        return $strictUtf8.GetString($stream.ToArray()) | ConvertFrom-Json
    } finally {
        $receiveCts.Dispose()
        $stream.Dispose()
    }
}

function Receive-LoomHookResponse {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$RequestId
    )

    # The daemon gives Hook Art requests a 120-second execution budget. Keep a little transport
    # headroom so a slow hosted runner reports the daemon's terminal response instead of cancelling
    # the WebSocket first.
    $deadline = [DateTime]::UtcNow.AddSeconds(150)
    for ($attempt = 0; $attempt -lt 64 -and [DateTime]::UtcNow -lt $deadline; $attempt++) {
        $remainingSeconds = [Math]::Max(1, [Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds))
        $message = Receive-LoomHookBridgeWebSocketJson `
            -Client $Client `
            -TimeoutSeconds ([int]$remainingSeconds) `
            -Operation "art response $RequestId"
        if ([string]$message.protocolVersion -ne "loom.hook.v1") {
            continue
        }
        $requestIdProperty = $message.PSObject.Properties["requestId"]
        $statusProperty = $message.PSObject.Properties["status"]
        if (
            $null -ne $requestIdProperty -and
            $null -ne $statusProperty -and
            [string]$requestIdProperty.Value -eq $RequestId -and
            -not [string]::IsNullOrWhiteSpace([string]$statusProperty.Value)
        ) {
            return $message
        }
    }
    throw "Loom Hook did not return a terminal response for requestId $RequestId."
}

function Invoke-LoomHookArtExecution {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$RequestId,
        [string]$NodeId,
        [string]$ArtId,
        [hashtable]$Inputs,
        [hashtable]$Parameters
    )

    Send-LoomHookBridgeWebSocketJson -Client $Client -Json (@{
        method = "loom.hook.art.execute"
        params = @{
            protocolVersion = "loom.hook.v1"
            requestId = $RequestId
            nodeId = $NodeId
            artId = $ArtId
            generation = 1
            deviceId = "device:release-smoke"
            outputTransports = @("shared_memory", "websocket")
            inputs = $Inputs
            parameters = $Parameters
            disabledParameters = @()
        }
    } | ConvertTo-Json -Depth 30 -Compress)
    $response = Receive-LoomHookResponse -Client $Client -RequestId $RequestId
    if ([string]$response.status -ne "succeeded") {
        throw "Loom Hook Art execution failed for $ArtId (requestId=$RequestId, status=$([string]$response.status))."
    }
    return $response
}

function Close-LoomHookBridgeWebSocket {
    param([System.Net.WebSockets.ClientWebSocket]$Client)

    if ($null -eq $Client) {
        return
    }

    try {
        if ($Client.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
            $closeCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
            try {
                try {
                    [void]$Client.CloseAsync(
                        [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
                        "done",
                        $closeCts.Token
                    ).GetAwaiter().GetResult()
                } catch {
                    Write-Warning "Hook Bridge WebSocket close failed: $($_.Exception.Message)"
                }
            } finally {
                $closeCts.Dispose()
            }
        }
    } finally {
        $Client.Dispose()
    }
}
