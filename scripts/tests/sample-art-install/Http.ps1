<# Owns bounded authenticated JSON calls and Surface action polling for the install smoke. #>

function Read-BoundedHttpStream {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [int64]$DeclaredLength = -1,
        [int64]$MaximumBytes = (256MB)
    )

    if ($DeclaredLength -gt $MaximumBytes) {
        throw "Loom HTTP response exceeds the $MaximumBytes byte limit."
    }
    $buffer = New-Object byte[] (64KB)
    $output = [System.IO.MemoryStream]::new()
    try {
        while (($read = $Stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            if ($output.Length + $read -gt $MaximumBytes) {
                throw "Loom HTTP response exceeded the $MaximumBytes byte limit while streaming."
            }
            $output.Write($buffer, 0, $read)
        }
        return ,$output.ToArray()
    }
    finally {
        $output.Dispose()
    }
}

function ConvertFrom-LoomHttpResponse {
    param([Parameter(Mandatory = $true)][System.Net.HttpWebResponse]$Response)

    $stream = $Response.GetResponseStream()
    try {
        $bytes = Read-BoundedHttpStream -Stream $stream -DeclaredLength $Response.ContentLength
        if ($bytes.Length -eq 0) { return $null }
        $encoding = [System.Text.UTF8Encoding]::new($false, $true)
        $text = $encoding.GetString($bytes)
        return $text | ConvertFrom-Json
    }
    finally {
        if ($null -ne $stream) { $stream.Dispose() }
    }
}

function Invoke-LoomJson {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [AllowNull()][object]$Body
    )

    $request = [System.Net.WebRequest]::CreateHttp($Url)
    $request.Method = $Method.ToUpperInvariant()
    $request.Accept = "application/json"
    $request.AutomaticDecompression = [System.Net.DecompressionMethods]::GZip -bor [System.Net.DecompressionMethods]::Deflate
    $request.Timeout = if ($null -eq $Body) { 30000 } else { 120000 }
    $request.ReadWriteTimeout = $request.Timeout
    foreach ($name in $script:DaemonRequestHeaders.Keys) {
        $request.Headers[[string]$name] = [string]$script:DaemonRequestHeaders[$name]
    }
    if ($null -ne $Body) {
        $json = $Body | ConvertTo-Json -Depth 40 -Compress
        $requestBytes = [System.Text.Encoding]::UTF8.GetBytes($json)
        if ($requestBytes.Length -gt 192MB) {
            throw "Loom HTTP request exceeds the 201326592 byte limit."
        }
        $request.ContentType = "application/json"
        $request.ContentLength = $requestBytes.Length
        $requestStream = $request.GetRequestStream()
        try {
            $requestStream.Write($requestBytes, 0, $requestBytes.Length)
        }
        finally {
            $requestStream.Dispose()
        }
    }

    $response = $null
    try {
        $response = [System.Net.HttpWebResponse]$request.GetResponse()
        return ConvertFrom-LoomHttpResponse -Response $response
    }
    catch [System.Net.WebException] {
        $errorResponse = $_.Exception.Response
        if ($null -ne $errorResponse) {
            try {
                $bodyText = ""
                $errorStream = $errorResponse.GetResponseStream()
                try {
                    if ($null -ne $errorStream) {
                        $errorBytes = Read-BoundedHttpStream -Stream $errorStream -DeclaredLength $errorResponse.ContentLength -MaximumBytes (1MB)
                        $bodyText = [System.Text.Encoding]::UTF8.GetString($errorBytes)
                    }
                }
                finally {
                    if ($null -ne $errorStream) { $errorStream.Dispose() }
                }
                throw "Loom HTTP $Method $Url failed with status $([int]$errorResponse.StatusCode): $(Redact-SensitiveText $bodyText)"
            }
            finally {
                $errorResponse.Dispose()
            }
        }
        throw
    }
    finally {
        if ($null -ne $response) { $response.Dispose() }
    }
}

function Wait-StockSurfaceAction {
    param(
        [string]$BaseUrl,
        [string]$InstanceId,
        [string]$EventId,
        [int]$TimeoutSeconds = 20
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $record = Invoke-LoomJson -Method Get -Url "$BaseUrl/v1/surfaces/instances/$InstanceId" -Body $null
        $ack = Get-PropertyValue (Get-PropertyValue $record "eventAcks") $EventId
        if ($null -ne $ack -and [string]$ack.status -in @("succeeded", "failed", "cancelled")) {
            return [pscustomobject]@{ record = $record; ack = $ack }
        }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for Stock Monitor Surface action: $EventId"
}
