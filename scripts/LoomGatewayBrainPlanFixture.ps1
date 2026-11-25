# Owns the single-request loopback Gateway fixture and its background-job lifetime.
function Start-LoomGatewayBrainPlanFixture {
    param(
        [int]$Port,
        [string]$ReadyPath,
        [string]$CapturePath
    )

    return Start-Job -ArgumentList @($Port, $ReadyPath, $CapturePath) -ScriptBlock {
        param(
            [int]$Port,
            [string]$ReadyPath,
            [string]$CapturePath
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        $listener = $null
        $client = $null
        $stream = $null
        $memory = $null
        $encoding = [System.Text.UTF8Encoding]::new($false)
        try {
            $listener = [System.Net.Sockets.TcpListener]::new(
                [System.Net.IPAddress]::Parse("127.0.0.1"),
                $Port
            )
            $listener.Start()
            [System.IO.File]::WriteAllText($ReadyPath, "ready", $encoding)

            $client = $listener.AcceptTcpClient()
            $stream = $client.GetStream()
            $memory = [System.IO.MemoryStream]::new()
            $buffer = New-Object byte[] 8192
            $headerEnd = -1
            $contentLength = 0
            $requestText = ""
            while ($true) {
                $count = $stream.Read($buffer, 0, $buffer.Length)
                if ($count -eq 0) {
                    break
                }
                $memory.Write($buffer, 0, $count)
                $requestText = [System.Text.Encoding]::UTF8.GetString($memory.ToArray())
                $headerEnd = $requestText.IndexOf("`r`n`r`n", [System.StringComparison]::Ordinal)
                if ($headerEnd -lt 0) {
                    continue
                }
                $headerText = $requestText.Substring(0, $headerEnd)
                foreach ($line in ($headerText -split "`r`n")) {
                    $separator = $line.IndexOf(":", [System.StringComparison]::Ordinal)
                    if ($separator -gt 0 -and $line.Substring(0, $separator).Trim().ToLowerInvariant() -eq "content-length") {
                        $contentLength = [int]$line.Substring($separator + 1).Trim()
                    }
                }
                if ($memory.Length -ge ($headerEnd + 4 + $contentLength)) {
                    break
                }
            }

            if ($headerEnd -lt 0) {
                throw "Gateway fixture received an incomplete HTTP request"
            }
            $headerText = $requestText.Substring(0, $headerEnd)
            $bodyStart = $headerEnd + 4
            # Content-Length is measured in bytes, not decoded UTF-16 characters.
            $bodyText = if ($memory.Length -ge ($bodyStart + $contentLength)) {
                [System.Text.Encoding]::UTF8.GetString($memory.ToArray(), $bodyStart, $contentLength)
            } else {
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
            $payload = $null
            try {
                if (-not [string]::IsNullOrWhiteSpace($bodyText)) {
                    $payload = $bodyText | ConvertFrom-Json
                }
            }
            catch {
                $payload = $null
            }
            $model = if ($null -ne $payload) { [string]$payload.model } else { "" }
            $messages = @()
            if ($null -ne $payload -and $null -ne $payload.messages) {
                $messages = @($payload.messages | ForEach-Object {
                    [ordered]@{
                        role = [string]$_.role
                        content = [string]$_.content
                    }
                })
            }
            $valid = (
                $method -eq "POST" -and
                $path -eq "/v1/chat/completions" -and
                $authorization -eq "Bearer smoke-token" -and
                $model -eq "smoke-planner" -and
                $messages.Count -ge 2
            )
            $userContent = if ($messages.Count -ge 2) { [string]$messages[1].content } else { "" }
            $capture = [ordered]@{
                valid = [bool]$valid
                method = $method
                path = $path
                authReceived = ($authorization -eq "Bearer smoke-token")
                model = $model
                messageRoles = @($messages | ForEach-Object { [string]$_.role })
                userContent = $userContent
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($capture | ConvertTo-Json -Depth 20),
                $encoding
            )

            if ($valid) {
                $assistantContent = '{"summary":"Packaged Gateway plan","steps":["inspect packaged request","execute packaged plan"]}'
                $responseObject = [ordered]@{
                    model = "smoke-resolved-model"
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
                        code = "invalid_smoke_request"
                        message = "Gateway smoke request did not match the expected contract"
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
            $errorCapture = [ordered]@{
                valid = $false
                error = $_.Exception.Message
            }
            [System.IO.File]::WriteAllText(
                $CapturePath,
                ($errorCapture | ConvertTo-Json -Depth 10),
                $encoding
            )
            throw
        }
        finally {
            if ($null -ne $memory) { $memory.Dispose() }
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
            if ($null -ne $listener) { $listener.Stop() }
        }
    }
}

function Stop-LoomGatewayBrainPlanFixture {
    param(
        [AllowNull()]$Job,
        [int]$Port
    )

    if ($null -eq $Job) { return }
    if ($Job.State -eq "Running" -or $Job.State -eq "NotStarted") {
        # Wake AcceptTcpClient before cancellation so the fixture releases its sockets.
        $wakeClient = $null
        try {
            $wakeClient = [System.Net.Sockets.TcpClient]::new()
            $wakeClient.Connect("127.0.0.1", $Port)
        }
        catch {
        }
        finally {
            if ($null -ne $wakeClient) { $wakeClient.Dispose() }
        }
        [void](Wait-Job -Job $Job -Timeout 3 -ErrorAction SilentlyContinue)
        if ($Job.State -eq "Running" -or $Job.State -eq "NotStarted") {
            Stop-Job -Job $Job -ErrorAction SilentlyContinue
        }
    }
    Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
}
