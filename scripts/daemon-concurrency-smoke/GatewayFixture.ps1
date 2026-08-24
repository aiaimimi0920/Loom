<# Owns the bounded loopback Gateway fixture and its cross-job synchronization. #>

function Start-GatewayFixtureJob {
    param(
        [int]$Port,
        [string]$ReadyPath,
        [string]$CapturePath,
        [string]$EnteredEventName,
        [string]$ReleaseEventName
    )

    return Start-Job -ArgumentList @(
        $Port,
        $ReadyPath,
        $CapturePath,
        $EnteredEventName,
        $ReleaseEventName
    ) -ScriptBlock {
        param(
            [int]$Port,
            [string]$ReadyPath,
            [string]$CapturePath,
            [string]$EnteredEventName,
            [string]$ReleaseEventName
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $listener = $null
        $acceptResult = $null
        $client = $null
        $stream = $null
        $memory = $null
        $enteredEvent = $null
        $releaseEvent = $null
        $encoding = [System.Text.UTF8Encoding]::new($false)
        $strictEncoding = [System.Text.UTF8Encoding]::new($false, $true)
        $maximumHeaderBytes = 64 * 1024
        $maximumBodyBytes = 1024 * 1024
        $maximumRequestBytes = $maximumHeaderBytes + 4 + $maximumBodyBytes
        try {
            $enteredEvent = [System.Threading.EventWaitHandle]::OpenExisting($EnteredEventName)
            $releaseEvent = [System.Threading.EventWaitHandle]::OpenExisting($ReleaseEventName)
            $listener = [System.Net.Sockets.TcpListener]::new(
                [System.Net.IPAddress]::Parse("127.0.0.1"),
                $Port
            )
            $listener.Start()
            [System.IO.File]::WriteAllText($ReadyPath, "ready", $encoding)

            $acceptResult = $listener.BeginAcceptTcpClient($null, $null)
            if (-not $acceptResult.AsyncWaitHandle.WaitOne(30000)) {
                throw "Gateway fixture timed out waiting for a connection."
            }
            $client = $listener.EndAcceptTcpClient($acceptResult)
            $acceptResult.AsyncWaitHandle.Close()
            $acceptResult = $null
            $client.ReceiveTimeout = 5000
            $client.SendTimeout = 5000
            $stream = $client.GetStream()
            $stream.ReadTimeout = 5000
            $stream.WriteTimeout = 5000
            $memory = [System.IO.MemoryStream]::new()
            $buffer = New-Object byte[] 8192
            $headerEnd = -1
            $contentLength = 0
            $expectedBytes = $null
            $headerText = ""
            while ($true) {
                try {
                    $count = $stream.Read($buffer, 0, $buffer.Length)
                }
                catch [System.IO.IOException] {
                    throw "Gateway fixture timed out while reading the request."
                }
                if ($count -eq 0) {
                    break
                }
                if (($memory.Length + $count) -gt $maximumRequestBytes) {
                    throw "Gateway fixture request exceeded the byte limit."
                }
                $memory.Write($buffer, 0, $count)

                if ($headerEnd -lt 0) {
                    $headerProbe = [System.Text.Encoding]::ASCII.GetString($memory.ToArray())
                    $headerEnd = $headerProbe.IndexOf("`r`n`r`n", [System.StringComparison]::Ordinal)
                    if ($headerEnd -lt 0) {
                        if ($memory.Length -gt $maximumHeaderBytes) {
                            throw "Gateway fixture request headers exceeded the byte limit."
                        }
                        continue
                    }
                    if ($headerEnd -gt $maximumHeaderBytes) {
                        throw "Gateway fixture request headers exceeded the byte limit."
                    }
                    $headerText = $headerProbe.Substring(0, $headerEnd)
                    $contentLengthSeen = $false
                    $contentType = ""
                    foreach ($line in ($headerText -split "`r`n")) {
                        $separator = $line.IndexOf(":", [System.StringComparison]::Ordinal)
                        if ($separator -le 0) {
                            continue
                        }
                        $headerName = $line.Substring(0, $separator).Trim().ToLowerInvariant()
                        if ($headerName -eq "content-type") {
                            if (-not [string]::IsNullOrWhiteSpace($contentType)) {
                                throw "Gateway fixture received duplicate Content-Type headers."
                            }
                            $contentType = $line.Substring($separator + 1).Trim()
                            continue
                        }
                        if ($headerName -ne "content-length") {
                            continue
                        }
                        if ($contentLengthSeen) { throw "Gateway fixture received duplicate Content-Length headers." }
                        $contentLengthText = $line.Substring($separator + 1).Trim()
                        if ($contentLengthText -notmatch '^\d{1,7}$') {
                            throw "Gateway fixture received an invalid Content-Length header."
                        }
                        $contentLength = [int]$contentLengthText
                        $contentLengthSeen = $true
                    }
                    if (-not $contentLengthSeen) {
                        throw "Gateway fixture requires a Content-Length header."
                    }
                    $contentTypeMedia = (($contentType -split ';', 2)[0]).Trim().ToLowerInvariant()
                    if ($contentTypeMedia -ne "application/json") {
                        throw "Gateway fixture requires Content-Type application/json."
                    }
                    if ($contentLength -gt $maximumBodyBytes) {
                        throw "Gateway fixture request body exceeded the byte limit."
                    }
                    $expectedBytes = $headerEnd + 4 + $contentLength
                }
                if ($null -ne $expectedBytes -and $memory.Length -ge $expectedBytes) {
                    break
                }
            }

            if ($headerEnd -lt 0 -or $null -eq $expectedBytes -or $memory.Length -lt $expectedBytes) {
                throw "Gateway fixture received an incomplete HTTP request."
            }
            $requestBytes = $memory.ToArray()
            $bodyStart = $headerEnd + 4
            $bodyText = if ($contentLength -gt 0) {
                $strictEncoding.GetString($requestBytes, $bodyStart, $contentLength)
            }
            else {
                ""
            }
            $lines = $headerText -split "`r`n"
            $requestParts = $lines[0] -split " "
            $method = if ($requestParts.Count -gt 0) { $requestParts[0] } else { "" }
            $path = if ($requestParts.Count -gt 1) { $requestParts[1] } else { "" }
            $authorization = ""
            for ($index = 1; $index -lt $lines.Count; $index++) {
                $separator = $lines[$index].IndexOf(":", [System.StringComparison]::Ordinal)
                if ($separator -gt 0 -and $lines[$index].Substring(0, $separator).Trim().ToLowerInvariant() -eq "authorization") {
                    $authorization = $lines[$index].Substring($separator + 1).Trim()
                }
            }

            $payload = if ([string]::IsNullOrWhiteSpace($bodyText)) {
                $null
            }
            else {
                $bodyText | ConvertFrom-Json
            }
            $model = if ($null -ne $payload) { [string]$payload.model } else { "" }
            $messages = @()
            if ($null -ne $payload -and $null -ne $payload.messages) {
                $messages = @($payload.messages | ForEach-Object {
                    [ordered]@{
                        role = [string]$_.role
                        content = ([string]$_.content).Replace("loom-concurrency-smoke-token", "<redacted>")
                    }
                })
            }
            $valid = (
                $method -eq "POST" -and
                $path -eq "/v1/chat/completions" -and
                $authorization -eq "Bearer loom-concurrency-smoke-token" -and
                $model -eq "concurrency-smoke" -and
                $messages.Count -ge 2
            )
            $capture = [ordered]@{
                valid = [bool]$valid
                method = $method
                path = $path
                authReceived = ($authorization -eq "Bearer loom-concurrency-smoke-token")
                model = $model
                messageRoles = @($messages | ForEach-Object { [string]$_.role })
                userContent = if ($messages.Count -ge 2) { [string]$messages[1].content } else { "" }
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($capture | ConvertTo-Json -Depth 20),
                $encoding
            )
            [void]$enteredEvent.Set()

            if (-not $releaseEvent.WaitOne(30000)) {
                throw "Gateway fixture timed out waiting for release."
            }

            if ($valid) {
                $assistantContent = '{"summary":"Concurrent packaged Gateway plan","steps":["inspect concurrent request","complete concurrent plan"]}'
                $responseObject = [ordered]@{
                    model = "concurrency-smoke-resolved"
                    choices = @(
                        [ordered]@{
                            message = [ordered]@{
                                role = "assistant"
                                content = $assistantContent
                            }
                        }
                    )
                }
                $statusLine = "200 OK"
            }
            else {
                $responseObject = [ordered]@{
                    error = [ordered]@{
                        code = "invalid_concurrency_smoke_request"
                        message = "Gateway concurrency request did not match the expected contract."
                    }
                }
                $statusLine = "400 Bad Request"
            }
            $responseJson = $responseObject | ConvertTo-Json -Depth 20 -Compress
            $responseBytes = [System.Text.Encoding]::UTF8.GetBytes($responseJson)
            $responseHeader = "HTTP/1.1 $statusLine`r`nContent-Type: application/json`r`nContent-Length: $($responseBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($responseHeader)
            $stream.Write($headerBytes, 0, $headerBytes.Length)
            $stream.Write($responseBytes, 0, $responseBytes.Length)
            $stream.Flush()
        }
        catch {
            if (-not (Test-Path -LiteralPath $CapturePath -PathType Leaf)) {
                $errorCapture = [ordered]@{
                    valid = $false
                    error = $_.Exception.Message
                }
                [System.IO.File]::WriteAllText(
                    $CapturePath,
                    ($errorCapture | ConvertTo-Json -Depth 10),
                    $encoding
                )
            }
            throw
        }
        finally {
            if ($null -ne $memory) { $memory.Dispose() }
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
            if ($null -ne $acceptResult) { $acceptResult.AsyncWaitHandle.Close() }
            if ($null -ne $listener) { $listener.Stop() }
            if ($null -ne $enteredEvent) { $enteredEvent.Dispose() }
            if ($null -ne $releaseEvent) { $releaseEvent.Dispose() }
        }
    }
}
