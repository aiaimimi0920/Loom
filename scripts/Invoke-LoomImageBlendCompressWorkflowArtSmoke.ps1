<#
.SYNOPSIS
    Execute the installed image-blend-and-compress workflow Art through AHRP.
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [int]$BridgePort = 19820,
    [string]$ArtId = "custom-image-blend-compress-workflow",
    [int]$MixRatio = 25,
    [int]$Quality = 90,
    [int]$Size = 64,
    [string]$OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($MixRatio -lt 0 -or $MixRatio -gt 100) {
    throw "MixRatio must be between 0 and 100."
}
if ($Quality -lt 60 -or $Quality -gt 100) {
    throw "Quality must be between 60 and 100."
}
if ($Size -le 0) {
    throw "Size must be greater than zero."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $OutputDir -or $OutputDir.Trim().Length -eq 0) {
    $OutputDir = Join-Path $repoRoot "output\smoke\image-blend-compress-workflow"
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function New-SolidPngDataUrl {
    param(
        [int]$Width,
        [int]$Height,
        [System.Drawing.Color]$Color
    )

    $bitmap = [System.Drawing.Bitmap]::new(
        $Width,
        $Height,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear($Color)
        }
        finally {
            $graphics.Dispose()
        }

        $stream = [System.IO.MemoryStream]::new()
        try {
            $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
            return "data:image/png;base64," + [Convert]::ToBase64String($stream.ToArray())
        }
        finally {
            $stream.Dispose()
        }
    }
    finally {
        $bitmap.Dispose()
    }
}

function Send-WebSocketText {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$Text
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $cts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        $null = $Client.SendAsync(
            [ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text,
            $true,
            $cts.Token
        ).GetAwaiter().GetResult()
    }
    finally {
        $cts.Dispose()
    }
}

function Receive-WebSocketText {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [int]$TimeoutSeconds
    )

    $buffer = New-Object byte[] 65536
    $builder = [System.Text.StringBuilder]::new()
    do {
        $cts = [System.Threading.CancellationTokenSource]::new(
            [TimeSpan]::FromSeconds($TimeoutSeconds)
        )
        try {
            $result = $Client.ReceiveAsync(
                [ArraySegment[byte]]::new($buffer),
                $cts.Token
            ).GetAwaiter().GetResult()
        }
        finally {
            $cts.Dispose()
        }
        if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
            throw "Hook Bridge closed before returning the requested result."
        }
        $null = $builder.Append(
            [System.Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count)
        )
    } while (-not $result.EndOfMessage)

    return $builder.ToString()
}

function Get-ResponseError {
    param([object]$Response)

    foreach ($path in @(
        "error.message",
        "error",
        "data.error.message",
        "data.error",
        "data.message",
        "message"
    )) {
        $candidate = $Response
        foreach ($segment in $path.Split(".")) {
            if ($null -eq $candidate) {
                break
            }
            $property = $candidate.PSObject.Properties[$segment]
            if ($null -eq $property) {
                $candidate = $null
                break
            }
            $candidate = $property.Value
        }
        if ($null -ne $candidate -and -not [string]::IsNullOrWhiteSpace([string]$candidate)) {
            return [string]$candidate
        }
    }
    return "Workflow Art execution failed."
}

Add-Type -AssemblyName System.Drawing

$status = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd('/') + "/status") -Method Get -TimeoutSec 10
if ([string]$status.status -ne "ready") {
    throw "Loom daemon is not ready at $BaseUrl."
}
$bridgeStatus = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd('/') + "/v1/hook-bridge/status") -Method Get -TimeoutSec 10
if (-not [bool]$bridgeStatus.running) {
    throw "Loom Hook Bridge is not running."
}
if ($BridgePort -le 0) {
    $BridgePort = [int]$bridgeStatus.port
}

$source = New-SolidPngDataUrl -Width $Size -Height $Size -Color (
    [System.Drawing.Color]::FromArgb(255, 240, 60, 0)
)
$reference = New-SolidPngDataUrl -Width $Size -Height $Size -Color (
    [System.Drawing.Color]::FromArgb(255, 40, 160, 200)
)
$requestId = "loom-workflow-smoke-" + [Guid]::NewGuid().ToString("N")
$request = [ordered]@{
    method = "art/process"
    params = [ordered]@{
        request_id = $requestId
        art_id = $ArtId
        input = [ordered]@{
            type = "base64"
            data = $source
            width = $Size
            height = $Size
            format = "rgba8"
        }
        params = [ordered]@{
            mix_ratio = $MixRatio
            quality_num = $Quality
        }
        input_images = [ordered]@{
            reference = $reference
        }
        disabled_params = @()
    }
}

$client = [System.Net.WebSockets.ClientWebSocket]::new()
$connectCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
try {
    $null = $client.ConnectAsync(
        [Uri]::new("ws://127.0.0.1:$BridgePort"),
        $connectCts.Token
    ).GetAwaiter().GetResult()
}
finally {
    $connectCts.Dispose()
}

try {
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    Send-WebSocketText -Client $client -Text ($request | ConvertTo-Json -Depth 20 -Compress)
    $rawResponse = $null
    $response = $null
    do {
        $rawResponse = Receive-WebSocketText -Client $client -TimeoutSeconds 60
        $response = $rawResponse | ConvertFrom-Json
    } while ([string]$response.request_id -ne $requestId)
    $stopwatch.Stop()

    if ([string]$response.status -ne "Success") {
        throw (Get-ResponseError -Response $response)
    }

    $outputData = [string]$response.data.output.data
    if ([string]::IsNullOrWhiteSpace($outputData)) {
        throw "Workflow Art returned no image data."
    }
    $base64 = ($outputData -split ",", 2)[-1]
    $outputBytes = [Convert]::FromBase64String($base64)
    $outputPath = Join-Path $OutputDir "output.png"
    [System.IO.File]::WriteAllBytes($outputPath, $outputBytes)

    $stream = [System.IO.MemoryStream]::new($outputBytes)
    try {
        $bitmap = [System.Drawing.Bitmap]::new($stream)
        try {
            if ($bitmap.Width -ne $Size -or $bitmap.Height -ne $Size) {
                throw "Workflow output size mismatch: $($bitmap.Width)x$($bitmap.Height)."
            }
            $pixel = $bitmap.GetPixel(0, 0)
        }
        finally {
            $bitmap.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }

    $expectedR = [int][Math]::Round(240 * (100 - $MixRatio) / 100 + 40 * $MixRatio / 100)
    $expectedG = [int][Math]::Round(60 * (100 - $MixRatio) / 100 + 160 * $MixRatio / 100)
    $expectedB = [int][Math]::Round(0 * (100 - $MixRatio) / 100 + 200 * $MixRatio / 100)
    $tolerance = 25
    if (
        [Math]::Abs([int]$pixel.R - $expectedR) -gt $tolerance -or
        [Math]::Abs([int]$pixel.G - $expectedG) -gt $tolerance -or
        [Math]::Abs([int]$pixel.B - $expectedB) -gt $tolerance
    ) {
        throw "Workflow output pixel is outside compression tolerance."
    }

    $summary = [ordered]@{
        requestId = $requestId
        artId = $ArtId
        status = [string]$response.status
        responseMs = $stopwatch.ElapsedMilliseconds
        bridgePort = $BridgePort
        mixRatio = $MixRatio
        quality = $Quality
        width = $Size
        height = $Size
        pixelR = [int]$pixel.R
        pixelG = [int]$pixel.G
        pixelB = [int]$pixel.B
        pixelA = [int]$pixel.A
        expectedR = $expectedR
        expectedG = $expectedG
        expectedB = $expectedB
        outputPngBytes = $outputBytes.Length
        responseChars = $rawResponse.Length
        outputPath = $outputPath
    }
    $summaryPath = Join-Path $OutputDir "summary.json"
    $summaryJson = $summary | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText(
        $summaryPath,
        ($summaryJson + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    $summaryJson
}
finally {
    if ($client.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
        $closeCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(3))
        try {
            $null = $client.CloseAsync(
                [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
                "done",
                $closeCts.Token
            ).GetAwaiter().GetResult()
        }
        catch {
        }
        finally {
            $closeCts.Dispose()
        }
    }
    $client.Dispose()
}
