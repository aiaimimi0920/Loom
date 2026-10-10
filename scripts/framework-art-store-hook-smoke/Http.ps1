# Authenticated loopback HTTP clients and bounded readiness polling.
function Assert-SmokeLoopbackHttpUri {
    param([string]$Uri)

    $parsed = $null
    if (-not [Uri]::TryCreate($Uri, [UriKind]::Absolute, [ref]$parsed)) {
        throw "Smoke HTTP URI must be absolute: $Uri"
    }
    if (
        $parsed.Scheme -ne [Uri]::UriSchemeHttp -or
        -not [string]::IsNullOrEmpty($parsed.UserInfo)
    ) {
        throw "Smoke HTTP URI must use unauthenticated loopback HTTP: $Uri"
    }

    $address = $null
    $isLoopback = [System.Net.IPAddress]::TryParse($parsed.DnsSafeHost, [ref]$address) -and
        [System.Net.IPAddress]::IsLoopback($address)
    if (-not $isLoopback -and $parsed.DnsSafeHost -ne "localhost") {
        throw "Smoke HTTP URI must target loopback: $Uri"
    }
    return $parsed
}

function Invoke-JsonGet {
    param(
        [string]$Uri,
        [int]$TimeoutSeconds = 15
    )

    $validatedUri = Assert-SmokeLoopbackHttpUri -Uri $Uri
    return Invoke-RestMethod -Uri $validatedUri -Method Get -Headers $script:DaemonRequestHeaders -TimeoutSec $TimeoutSeconds
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body
    )

    $validatedUri = Assert-SmokeLoopbackHttpUri -Uri $Uri
    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $validatedUri -Method Post -Headers $script:DaemonRequestHeaders -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonPut {
    param(
        [string]$Uri,
        [object]$Body
    )

    $validatedUri = Assert-SmokeLoopbackHttpUri -Uri $Uri
    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $validatedUri -Method Put -Headers $script:DaemonRequestHeaders -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonDelete {
    param([string]$Uri)

    $validatedUri = Assert-SmokeLoopbackHttpUri -Uri $Uri
    return Invoke-RestMethod -Uri $validatedUri -Method Delete -Headers $script:DaemonRequestHeaders -TimeoutSec 20
}

function Wait-HttpJson {
    param(
        [string]$Uri,
        [string]$Message,
        [int]$TimeoutSeconds = 20
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $lastError = $null
    do {
        try {
            $remainingSeconds = [Math]::Max(1, [Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds))
            $attemptTimeout = [Math]::Min(2, [int]$remainingSeconds)
            return Invoke-JsonGet -Uri $Uri -TimeoutSeconds $attemptTimeout
        } catch {
            $lastError = $_.Exception.Message
            Start-Sleep -Milliseconds 150
        }
    } while ([DateTime]::UtcNow -lt $deadline)

    $safeError = ConvertTo-SafeSmokeErrorText -Text $lastError
    throw "$Message ($Uri). Last error: $safeError"
}

function Wait-TcpPort {
    param(
        [string]$HostName,
        [int]$Port,
        [string]$Message,
        [ValidateRange(1, 300)][int]$TimeoutSeconds = 10,
        [System.Diagnostics.Process]$Process,
        [string]$StderrPath = "",
        [string[]]$Secrets = @()
    )

    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $budget = [long]$TimeoutSeconds * 1000
    $lastError = "TCP connection attempt timed out"
    do {
        # The caller retains ownership and cleanup; a dead fixture cannot become ready.
        if ($null -ne $Process -and $Process.WaitForExit(0)) {
            $detail = ""
            if (-not [string]::IsNullOrWhiteSpace($StderrPath)) {
                try {
                    $safePath = Resolve-SmokeRealFile -Path $StderrPath -Label "fixture stderr"
                    $stream = [System.IO.File]::Open($safePath, 'Open', 'Read', 'ReadWrite')
                    try {
                        # Read a bounded tail, not an unbounded redirected log or command line.
                        $offset = [Math]::Max(0L, $stream.Length - 4096L)
                        [void]$stream.Seek($offset, 'Begin')
                        $bytes = New-Object byte[] 4096
                        $count = $stream.Read($bytes, 0, $bytes.Length)
                        $tail = [Text.Encoding]::UTF8.GetString($bytes, 0, $count)
                        # Discard cut lines so truncation cannot expose only part of a secret.
                        if ($offset -gt 0) {
                            $newline = $tail.IndexOf("`n")
                            $tail = if ($newline -ge 0) { $tail.Substring($newline + 1) } else { "<stderr line exceeds limit>" }
                        }
                        if ($stream.Position -lt $stream.Length) {
                            $newline = $tail.LastIndexOf("`n")
                            $tail = if ($newline -ge 0) { $tail.Substring(0, $newline) } else { "<stderr changed during read>" }
                        }
                        $detail = ConvertTo-SafeSmokeErrorText -Text $tail -Secrets $Secrets
                    } finally { $stream.Dispose() }
                } catch { $detail = "Fixture stderr unavailable" }
            }
            throw "$Message ($($HostName):$Port). Fixture exited with code $($Process.ExitCode). $detail"
        }
        $client = $null
        $waitHandle = $null
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $async = $client.BeginConnect($HostName, $Port, $null, $null)
            $waitHandle = $async.AsyncWaitHandle
            $remaining = [int][Math]::Max(0L, $budget - $clock.ElapsedMilliseconds)
            if ($waitHandle.WaitOne([Math]::Min(250, $remaining))) {
                $client.EndConnect($async)
                if ($client.Connected) {
                    if ($null -ne $Process -and $Process.WaitForExit(0)) { continue }
                    return
                }
            }
        } catch {
            $lastError = $_.Exception.Message
        } finally {
            if ($null -ne $waitHandle) {
                $waitHandle.Dispose()
            }
            if ($null -ne $client) {
                $client.Dispose()
            }
        }
        $remaining = [int][Math]::Max(0L, $budget - $clock.ElapsedMilliseconds)
        if ($remaining -gt 0) { Start-Sleep -Milliseconds ([Math]::Min(100, $remaining)) }
    } while ($clock.ElapsedMilliseconds -lt $budget)

    $safeError = ConvertTo-SafeSmokeErrorText -Text $lastError -Secrets $Secrets
    throw "$Message ($($HostName):$Port). Last error: $safeError"
}
