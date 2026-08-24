<# Owns the loopback MCP Registry fixture job and deterministic registry responses. #>

function Start-LoomMcpRegistryFixtureJob {
    param(
        [int]$Port,
        [string]$OutputDir
    )

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $readyPath = Join-Path $OutputDir "ready.txt"

    return Start-Job -ArgumentList $Port, $OutputDir, $readyPath -ScriptBlock {
        param(
            [int]$FixturePort,
            [string]$FixtureOutputDir,
            [string]$FixtureReadyPath
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"

        function Read-LoomMcpRegistryRequest {
            param([System.Net.Sockets.NetworkStream]$Stream)

            $maxRequestBytes = 1MB
            $buffer = New-Object byte[] 2048
            $captured = New-Object System.IO.MemoryStream
            $delimiterState = 0
            try {
                while ($true) {
                    $read = $Stream.Read($buffer, 0, $buffer.Length)
                    if ($read -le 0) {
                        break
                    }
                    if (($captured.Length + $read) -gt $maxRequestBytes) {
                        throw "MCP registry fixture request exceeded 1 MiB."
                    }
                    $captured.Write($buffer, 0, $read)

                    $delimiterSeen = $false
                    for ($index = 0; $index -lt $read; $index++) {
                        $value = $buffer[$index]
                        switch ($delimiterState) {
                            0 {
                                if ($value -eq 13) { $delimiterState = 1 }
                            }
                            1 {
                                if ($value -eq 10) { $delimiterState = 2 }
                                elseif ($value -ne 13) { $delimiterState = 0 }
                            }
                            2 {
                                if ($value -eq 13) { $delimiterState = 3 }
                                else { $delimiterState = 0 }
                            }
                            3 {
                                if ($value -eq 10) { $delimiterSeen = $true }
                                elseif ($value -eq 13) { $delimiterState = 1 }
                                else { $delimiterState = 0 }
                            }
                        }
                    }
                    if ($delimiterSeen) {
                        return [System.Text.Encoding]::UTF8.GetString($captured.ToArray())
                    }
                }
            } finally {
                $captured.Dispose()
            }
            return ""
        }

        function Write-LoomMcpRegistryJsonResponse {
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
            for ($requestIndex = 1; $requestIndex -le 2; $requestIndex++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $stream = $client.GetStream()
                    $stream.ReadTimeout = 30000
                    $request = Read-LoomMcpRegistryRequest -Stream $stream
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request.txt"),
                        $request,
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$requestIndex.txt"),
                        $request,
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    $body = '{"servers":[{"server":{"name":"io.modelcontextprotocol/fixture","title":"Fixture MCP","description":"Fixture registry server","packages":[{"registryType":"npm","identifier":"@fixture/mcp","version":"1.0.0","transport":{"type":"stdio"},"runtimeArguments":[{"value":"-y"}],"environmentVariables":[{"name":"FIXTURE_API_KEY","isRequired":true}]}]},"_meta":{"io.modelcontextprotocol.registry/official":{"status":"active","isLatest":true,"updatedAt":"2026-06-12T00:00:00Z"}}}],"metadata":{"count":1}}'
                    Write-LoomMcpRegistryJsonResponse -Stream $stream -Body $body
                } finally {
                    $client.Close()
                }
            }
        } finally {
            $listener.Stop()
        }
    }
}
