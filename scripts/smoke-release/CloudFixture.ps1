<# Owns the loopback cloud API fixture job and its bounded request/response protocol. #>

function Start-LoomCloudApiFixtureJob {
    param(
        [int]$Port,
        [string]$OutputDir,
        [int]$MaxRequests = 3
    )

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $readyPath = Join-Path $OutputDir "ready.txt"

    return Start-Job -ArgumentList $Port, $OutputDir, $readyPath, $MaxRequests -ScriptBlock {
        param(
            [int]$FixturePort,
            [string]$FixtureOutputDir,
            [string]$FixtureReadyPath,
            [int]$FixtureMaxRequests
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"

        function Read-LoomCloudRequest {
            param([System.Net.Sockets.NetworkStream]$Stream)

            $maxRequestBytes = 16 * 1024 * 1024
            $buffer = New-Object byte[] 4096
            $captured = New-Object System.IO.MemoryStream
            $headerEnd = -1
            $expectedLength = -1L
            $headers = ""
            try {
                while ($true) {
                    $read = $Stream.Read($buffer, 0, $buffer.Length)
                    if ($read -le 0) {
                        break
                    }

                    $previousLength = [int]$captured.Length
                    if (($captured.Length + $read) -gt $maxRequestBytes) {
                        throw "Cloud fixture request exceeded 16 MiB."
                    }
                    $captured.Write($buffer, 0, $read)

                    if ($headerEnd -lt 0) {
                        $capturedBuffer = $captured.GetBuffer()
                        $scanStart = [System.Math]::Max(0, $previousLength - 3)
                        $scanEnd = [int]$captured.Length - 4
                        for ($index = $scanStart; $index -le $scanEnd; $index++) {
                            if ($capturedBuffer[$index] -eq 13 -and
                                $capturedBuffer[$index + 1] -eq 10 -and
                                $capturedBuffer[$index + 2] -eq 13 -and
                                $capturedBuffer[$index + 3] -eq 10) {
                                $headerEnd = $index
                                break
                            }
                        }

                        if ($headerEnd -ge 0) {
                            $headers = [System.Text.Encoding]::UTF8.GetString($capturedBuffer, 0, $headerEnd)
                            $contentLength = 0
                            foreach ($line in ($headers -split "`r`n")) {
                                $parts = $line.Split(":", 2)
                                if ($parts.Count -eq 2 -and $parts[0].Trim().Equals("content-length", [System.StringComparison]::OrdinalIgnoreCase)) {
                                    if (-not [int]::TryParse($parts[1].Trim(), [ref]$contentLength) -or $contentLength -lt 0) {
                                        throw "Cloud fixture request had an invalid Content-Length header."
                                    }
                                }
                            }
                            $expectedLength = [long]$headerEnd + 4 + $contentLength
                            if ($expectedLength -gt $maxRequestBytes) {
                                throw "Cloud fixture request declared more than 16 MiB."
                            }
                        }
                    }

                    if ($headerEnd -ge 0 -and $captured.Length -ge $expectedLength) {
                        $requestBytes = $captured.ToArray()
                        $raw = [System.Text.Encoding]::UTF8.GetString($requestBytes, 0, [int]$expectedLength)
                        $body = [System.Text.Encoding]::UTF8.GetString($requestBytes, $headerEnd + 4, $contentLength)
                        $requestLine = (($headers -split "`r`n") | Select-Object -First 1)
                        $requestParts = $requestLine.Split(" ")
                        return [ordered]@{
                            raw = $raw
                            path = [string]$requestParts[1]
                            body = [string]$body
                        }
                    }
                }
            } finally {
                $captured.Dispose()
            }

            return [ordered]@{
                raw = ""
                path = "/"
                body = ""
            }
        }

        function Get-LoomJsonProperty {
            param(
                [object]$Object,
                [string]$Name
            )

            if ($null -eq $Object) {
                return $null
            }
            $property = $Object.PSObject.Properties[$Name]
            if ($null -eq $property) {
                return $null
            }
            return $property.Value
        }

        function Write-LoomCloudJsonResponse {
            param(
                [System.Net.Sockets.NetworkStream]$Stream,
                [string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $Stream.Write($headerBytes, 0, $headerBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), $FixturePort)
        $listener.Start()
        try {
            [System.IO.File]::WriteAllText($FixtureReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
            for ($index = 0; $index -lt $FixtureMaxRequests; $index++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $stream = $client.GetStream()
                    $stream.ReadTimeout = 30000
                    $request = Read-LoomCloudRequest -Stream $stream
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$index.txt"),
                        [string]$request["raw"],
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$index.json"),
                        [string]$request["body"],
                        [System.Text.UTF8Encoding]::new($false)
                    )

                    $payload = $null
                    $bodyText = [string]$request["body"]
                    $bodyTrimmed = $bodyText.TrimStart()
                    if (
                        -not [string]::IsNullOrWhiteSpace($bodyText) -and
                        ($bodyTrimmed.StartsWith("{") -or $bodyTrimmed.StartsWith("["))
                    ) {
                        $payload = $bodyText | ConvertFrom-Json
                    }

                    if ([string]$request["path"] -like "/multipart/*") {
                        $raw = [string]$request["raw"]
                        $rawLower = $raw.ToLowerInvariant()
                        $evidence = [ordered]@{
                            path = [string]$request["path"]
                            multipartSeen = $rawLower.Contains("content-type: multipart/form-data; boundary=")
                            fileFieldSeen = $raw.Contains('name="file"')
                            # The host names the uploaded part `loom-cloud-input.<ext>` when the
                            # input arrives as a data URL, and `loom-cloud-input-<pid>-<stamp>.png`
                            # when it arrives as a staged temp file. Match the shared prefix.
                            tempFilenameSeen = $raw.Contains('filename="loom-cloud-input')
                            promptSeen = $raw.Contains("release cloud multipart")
                            traceSeen = $rawLower.Contains("x-trace: release-trace")
                            unresolvedTemplateSeen = $raw.Contains("{{")
                        }
                        [System.IO.File]::WriteAllText(
                            (Join-Path $FixtureOutputDir "multipart-request.json"),
                            ($evidence | ConvertTo-Json -Depth 20 -Compress),
                            [System.Text.UTF8Encoding]::new($false)
                        )
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "image"
                                    data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
                                    mimeType = "image/png"
                                }
                            )
                        }
                    } elseif ([string]$request["path"] -eq "/text") {
                        $prompt = [string](Get-LoomJsonProperty -Object $payload -Name "prompt")
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "text"
                                    text = "cloud saw $prompt"
                                }
                            )
                        }
                    } else {
                        $imageData = Get-LoomJsonProperty -Object $payload -Name "input_base64"
                        $inputValue = Get-LoomJsonProperty -Object $payload -Name "input"
                        if ([string]::IsNullOrWhiteSpace([string]$imageData) -and $null -ne $inputValue -and -not ($inputValue -is [string])) {
                            $imageData = Get-LoomJsonProperty -Object $inputValue -Name "data"
                        }
                        if ([string]::IsNullOrWhiteSpace([string]$imageData)) {
                            $imageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
                        }
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "image"
                                    data = [string]$imageData
                                    mimeType = "image/png"
                                }
                            )
                        }
                    }

                    Write-LoomCloudJsonResponse -Stream $stream -Body ($response | ConvertTo-Json -Depth 20 -Compress)
                } finally {
                    $client.Close()
                }
            }
        } finally {
            $listener.Stop()
        }
    }
}
