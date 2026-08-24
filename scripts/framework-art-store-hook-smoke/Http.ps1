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
        [int]$TimeoutSeconds = 10
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $lastError = $null
    do {
        $client = $null
        $waitHandle = $null
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $async = $client.BeginConnect($HostName, $Port, $null, $null)
            $waitHandle = $async.AsyncWaitHandle
            if ($waitHandle.WaitOne(250)) {
                $client.EndConnect($async)
                if ($client.Connected) {
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
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    $safeError = ConvertTo-SafeSmokeErrorText -Text $lastError
    throw "$Message ($($HostName):$Port). Last error: $safeError"
}
