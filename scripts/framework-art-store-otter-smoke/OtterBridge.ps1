# Business assertions stay in PowerShell; authenticated transport stays native.
. (Join-Path $PSScriptRoot '..\NativeBridgeProbe.ps1')

function New-LoomOtterBridgeWebSocket {
    param([int]$Port, [string]$ManifestPath, [string]$ProbeExecutable)
    if ($Port -lt 1 -or $Port -gt 65535) { throw 'Otter Bridge port is out of range.' }
    return Start-LoomNativeBridgeProbe -Executable $ProbeExecutable -ManifestPath $ManifestPath
}

function Send-LoomOtterBridgeWebSocketJson {
    param([object]$Client, [string]$Json)
    if ([Text.Encoding]::UTF8.GetByteCount($Json) -gt 1MB) { throw 'Otter Bridge request exceeds the 1 MiB smoke limit.' }
    [void](Invoke-LoomNativeBridgeProbe -Client $Client -BudgetMs 11000 -Command @{
        op = 'send'; payload = $Json; timeoutMs = 10000
    })
}

function Receive-LoomOtterBridgeWebSocketJson {
    param([object]$Client, [int]$TimeoutSeconds = 30,
        [int]$MaxMessageBytes = 1MB, [string]$Operation = 'message')
    if ($TimeoutSeconds -lt 1 -or $MaxMessageBytes -lt 1) { throw 'Otter Bridge receive bounds must be positive.' }
    if ($TimeoutSeconds -gt 150 -or $MaxMessageBytes -gt 1MB) { throw 'Otter Bridge receive bounds exceed the smoke limit.' }
    $envelope = Invoke-LoomNativeBridgeProbe -Client $Client -BudgetMs ($TimeoutSeconds * 1000 + 1000) -Command @{
        op = 'receive'; timeoutMs = $TimeoutSeconds * 1000; maxBytes = $MaxMessageBytes
    }
    return $envelope.payload
}

function Receive-LoomOtterResponse {
    param(
        [object]$Client,
        [string]$RequestId
    )

    # The daemon gives Otter Art requests a 120-second execution budget. Keep a little transport
    # headroom so a slow hosted runner reports the daemon's terminal response instead of cancelling
    # the WebSocket first.
    $deadline = [DateTime]::UtcNow.AddSeconds(150)
    for ($attempt = 0; $attempt -lt 64 -and [DateTime]::UtcNow -lt $deadline; $attempt++) {
        $remainingSeconds = [Math]::Max(1, [Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds))
        $message = Receive-LoomOtterBridgeWebSocketJson `
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
    throw "Loom Otter did not return a terminal response for requestId $RequestId."
}

function Invoke-LoomOtterArtExecution {
    param(
        [object]$Client,
        [string]$RequestId,
        [string]$NodeId,
        [string]$ArtId,
        [hashtable]$Inputs,
        [hashtable]$Parameters
    )

    Send-LoomOtterBridgeWebSocketJson -Client $Client -Json (@{
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
    $response = Receive-LoomOtterResponse -Client $Client -RequestId $RequestId
    if ([string]$response.status -ne "succeeded") {
        throw "Loom Otter Art execution failed for $ArtId (requestId=$RequestId, status=$([string]$response.status))."
    }
    return $response
}

function Close-LoomOtterBridgeWebSocket {
    param([AllowNull()][object]$Client)
    Stop-LoomNativeBridgeProbe -Client $Client
}
