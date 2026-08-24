<# Owns the bounded loopback Brave-compatible image-search API fixture process. #>

function Start-ImageSearchApiFixture {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$ReadyPath,
        [Parameter(Mandatory = $true)][string]$RequestPath,
        [Parameter(Mandatory = $true)][string]$ImageUrl,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )

    $fixturePath = Join-Path $WorkRoot "image-search-api-fixture.ps1"
    $fixtureSource = @'
param(
    [int]$Port,
    [string]$ReadyPath,
    [string]$RequestPath,
    [string]$ImageUrl
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
$listener.Start()
try {
    [System.IO.File]::WriteAllText($ReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
    $captured = @()
    for ($requestIndex = 0; $requestIndex -lt 1; $requestIndex++) {
        $client = $listener.AcceptTcpClient()
        try {
            $client.ReceiveTimeout = 5000
            $client.SendTimeout = 5000
            $stream = $client.GetStream()
            $stream.ReadTimeout = 5000
            $stream.WriteTimeout = 5000
            $buffer = New-Object byte[] (64KB)
            $length = 0
            $complete = $false
            while ($length -lt $buffer.Length) {
                $value = $stream.ReadByte()
                if ($value -eq -1) { break }
                $buffer[$length] = [byte]$value
                $length++
                if ($length -ge 4 -and $buffer[$length - 4] -eq 13 -and $buffer[$length - 3] -eq 10 -and $buffer[$length - 2] -eq 13 -and $buffer[$length - 1] -eq 10) {
                    $complete = $true
                    break
                }
            }
            if (-not $complete) {
                throw "Image-search fixture HTTP headers exceeded 64 KiB or ended early."
            }
            $headerText = [System.Text.Encoding]::ASCII.GetString($buffer, 0, $length - 4)
            $lines = @($headerText -split "`r`n")
            $captured += $lines
            $captured += ""
            $requestLine = [string]$lines[0]
            if ($requestLine -like "GET /res/v1/images/search?*") {
                $body = @{
                    results = @(@{
                        title = "Installed package fixture"
                        url = $ImageUrl
                        source = "https://example.test/source"
                        thumbnail = @{ src = $ImageUrl }
                        properties = @{ url = $ImageUrl; width = 1; height = 1 }
                    })
                } | ConvertTo-Json -Depth 10 -Compress
                $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
                $contentType = "application/json; charset=utf-8"
                $status = "200 OK"
            }
            else {
                $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes("not found")
                $contentType = "text/plain"
                $status = "404 Not Found"
            }
            $header = "HTTP/1.1 $status`r`nContent-Type: $contentType`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $stream.Write($headerBytes, 0, $headerBytes.Length)
            $stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $stream.Flush()
        }
        finally {
            $client.Dispose()
        }
    }
    [System.IO.File]::WriteAllLines($RequestPath, $captured, [System.Text.UTF8Encoding]::new($false))
}
finally {
    $listener.Stop()
}
'@
    Write-Utf8NoBomFile -Path $fixturePath -Content ($fixtureSource + "`n")
    return Start-PowerShellFixtureProcess `
        -ScriptPath $fixturePath `
        -Parameters @{ Port = $Port; ReadyPath = $ReadyPath; RequestPath = $RequestPath; ImageUrl = $ImageUrl } `
        -StdoutPath $StdoutPath `
        -StderrPath $StderrPath
}
