[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$moduleRoot = Join-Path $scriptRoot "daemon-concurrency-smoke"
@("Common.ps1", "Process.ps1", "Http.ps1", "GatewayFixture.ps1") | ForEach-Object {
    . (Join-Path $moduleRoot $_)
}
. (Join-Path $scriptRoot "LoomSmokePorts.ps1")
$script:DaemonAuthHeaders = @{}

function Join-ByteArrays {
    param([byte[][]]$Arrays)

    $length = 0
    foreach ($array in $Arrays) { $length += $array.Length }
    $result = New-Object byte[] $length
    $offset = 0
    foreach ($array in $Arrays) {
        [System.Buffer]::BlockCopy($array, 0, $result, $offset, $array.Length)
        $offset += $array.Length
    }
    return $result
}

function Invoke-GatewayFixtureProbe {
    param(
        [byte[]]$RequestBytes,
        [bool]$ExpectEntered
    )

    $probeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-concurrency-helper-{0}" -f [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $probeRoot | Out-Null
    $readyPath = Join-Path $probeRoot "ready"
    $capturePath = Join-Path $probeRoot "capture.json"
    $enteredName = "LoomConcurrencyHelperEntered$([Guid]::NewGuid().ToString('N'))"
    $releaseName = "LoomConcurrencyHelperRelease$([Guid]::NewGuid().ToString('N'))"
    $entered = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $enteredName)
    $release = [System.Threading.EventWaitHandle]::new($false, [System.Threading.EventResetMode]::ManualReset, $releaseName)
    $job = $null
    $client = $null
    $stream = $null
    try {
        $port = Get-LoomSmokePort
        $job = Start-GatewayFixtureJob `
            -Port $port `
            -ReadyPath $readyPath `
            -CapturePath $capturePath `
            -EnteredEventName $enteredName `
            -ReleaseEventName $releaseName
        Wait-ForPath -Path $readyPath -TimeoutSeconds 10 -Job $job

        $client = [System.Net.Sockets.TcpClient]::new()
        $client.ReceiveTimeout = 5000
        $client.SendTimeout = 5000
        $client.Connect("127.0.0.1", $port)
        $stream = $client.GetStream()
        $stream.Write($RequestBytes, 0, $RequestBytes.Length)
        $stream.Flush()

        if ($ExpectEntered) {
            Assert-True ($entered.WaitOne(5000)) "Valid fixture request did not enter the release gate."
            [void]$release.Set()
            $response = [System.IO.MemoryStream]::new()
            try {
                $buffer = New-Object byte[] 4096
                while ($response.Length -lt (1024 * 1024)) {
                    $count = $stream.Read($buffer, 0, $buffer.Length)
                    if ($count -eq 0) { break }
                    $response.Write($buffer, 0, $count)
                }
                Assert-True ($response.Length -gt 0) "Valid fixture request returned no response."
            }
            finally {
                $response.Dispose()
            }
        }

        $completed = Wait-Job -Job $job -Timeout 10
        Assert-True ($null -ne $completed) "Gateway fixture probe did not terminate within its budget."
        $jobState = [string]$job.State
        $jobText = (Receive-Job -Job $job -Keep -ErrorAction SilentlyContinue 2>&1 | Out-String).Trim()
        $capture = if (Test-Path -LiteralPath $capturePath -PathType Leaf) {
            Get-Content -Raw -Encoding UTF8 -LiteralPath $capturePath | ConvertFrom-Json
        }
        else {
            $null
        }
        return [pscustomobject][ordered]@{
            state = $jobState
            output = $jobText
            capture = $capture
        }
    }
    finally {
        [void]$release.Set()
        if ($null -ne $stream) { $stream.Dispose() }
        if ($null -ne $client) { $client.Dispose() }
        if ($null -ne $job) {
            if ($job.State -eq "Running" -or $job.State -eq "NotStarted") {
                Stop-Job -Job $job -ErrorAction SilentlyContinue
            }
            Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
        }
        $entered.Dispose()
        $release.Dispose()
        if ((Split-Path -Leaf $probeRoot).StartsWith("loom-concurrency-helper-", [System.StringComparison]::Ordinal)) {
            Remove-Item -Recurse -Force -LiteralPath $probeRoot -ErrorAction SilentlyContinue
        }
    }
}

$scratchRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-concurrency-common-{0}" -f [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $scratchRoot | Out-Null
try {
    $largeLog = Join-Path $scratchRoot "large.log"
    [System.IO.File]::WriteAllBytes($largeLog, (New-Object byte[] 4096))
    $boundedLog = Read-BoundedUtf8Text -Path $largeLog -MaximumBytes 1024
    Assert-True ($boundedLog.Contains("<loom-evidence-truncated omittedBytes=3072>")) "Bounded log read did not mark truncation."

    $secretText = 'apiKey=alpha access_token=beta password: gamma https://example.test/?token=delta&ok=1 {"authToken":"echo"} Cookie: zeta'
    $redactedText = Redact-Text $secretText
    @("alpha", "beta", "gamma", "delta", "echo", "zeta") | ForEach-Object {
        Assert-True (-not $redactedText.Contains($_)) "Redaction retained secret value: $_"
    }
}
finally {
    Remove-Item -Recurse -Force -LiteralPath $scratchRoot -ErrorAction SilentlyContinue
}

$selfSnapshot = Get-ProcessSnapshotById -ProcessId $PID
Assert-True ($null -ne $selfSnapshot) "Could not snapshot the helper test process."
Assert-True ($null -ne $selfSnapshot.startTimeUtcTicks) "Process snapshot omitted creation identity."
Assert-True `
    (Test-ExactProcessAlive -ProcessId $PID -ExpectedExecutablePath $selfSnapshot.ExecutablePath -ExpectedStartTimeUtcTicks $selfSnapshot.startTimeUtcTicks) `
    "Exact process identity rejected the current process."
Assert-True `
    (-not (Test-ExactProcessAlive -ProcessId $PID -ExpectedExecutablePath $selfSnapshot.ExecutablePath -ExpectedStartTimeUtcTicks ($selfSnapshot.startTimeUtcTicks + 1))) `
    "Exact process identity accepted a mismatched creation time."
$refusedStop = Stop-ExactProcessById `
    -ProcessId $PID `
    -ExpectedExecutablePath $selfSnapshot.ExecutablePath `
    -ExpectedStartTimeUtcTicks ($selfSnapshot.startTimeUtcTicks + 1)
Assert-True (-not $refusedStop) "Exact process cleanup accepted a reused PID identity."

$partialRequest = [System.Text.Encoding]::ASCII.GetBytes("POST /v1/chat/completions HTTP/1.1`r`nHost: 127.0.0.1`r`n")
$stalled = Invoke-GatewayFixtureProbe -RequestBytes $partialRequest -ExpectEntered $false
Assert-Equal "Failed" $stalled.state "Stalled fixture request must fail."
Assert-True (([string]$stalled.capture.error).Contains("timed out while reading")) "Stalled fixture request did not report its read timeout."

$oversizedHeader = [System.Text.Encoding]::ASCII.GetBytes(
    "POST /v1/chat/completions HTTP/1.1`r`nContent-Type: application/json`r`nContent-Length: 1048577`r`n`r`n"
)
$oversized = Invoke-GatewayFixtureProbe -RequestBytes $oversizedHeader -ExpectEntered $false
Assert-Equal "Failed" $oversized.state "Oversized fixture request must fail."
Assert-True (([string]$oversized.capture.error).Contains("body exceeded")) "Oversized fixture request did not report its body limit."

$missingLengthHeader = [System.Text.Encoding]::ASCII.GetBytes(
    "POST /v1/chat/completions HTTP/1.1`r`nContent-Type: application/json`r`n`r`n"
)
$missingLength = Invoke-GatewayFixtureProbe -RequestBytes $missingLengthHeader -ExpectEntered $false
Assert-Equal "Failed" $missingLength.state "Fixture request without Content-Length must fail."
Assert-True (([string]$missingLength.capture.error).Contains("requires a Content-Length")) "Missing Content-Length was not diagnosed."

$invalidPrefix = [System.Text.Encoding]::ASCII.GetBytes(
    '{"model":"concurrency-smoke","messages":[{"role":"system","content":"plan"},{"role":"user","content":"'
)
$invalidSuffix = [System.Text.Encoding]::ASCII.GetBytes('"}]}')
$invalidBody = Join-ByteArrays -Arrays @($invalidPrefix, ([byte[]]@(0xff)), $invalidSuffix)
$invalidHeader = [System.Text.Encoding]::ASCII.GetBytes(
    "POST /v1/chat/completions HTTP/1.1`r`nAuthorization: Bearer loom-concurrency-smoke-token`r`nContent-Type: application/json`r`nContent-Length: $($invalidBody.Length)`r`n`r`n"
)
$invalidUtf8 = Invoke-GatewayFixtureProbe `
    -RequestBytes (Join-ByteArrays -Arrays @($invalidHeader, $invalidBody)) `
    -ExpectEntered $false
Assert-Equal "Failed" $invalidUtf8.state "Fixture request with invalid UTF-8 must fail."
Assert-True (-not [string]::IsNullOrWhiteSpace([string]$invalidUtf8.capture.error)) "Invalid UTF-8 was not diagnosed."

$unicodeContent = "hello " + [char]0x4e2d + [char]0x6587
$body = [ordered]@{
    model = "concurrency-smoke"
    messages = @(
        [ordered]@{ role = "system"; content = "plan" },
        [ordered]@{ role = "user"; content = $unicodeContent }
    )
} | ConvertTo-Json -Depth 10 -Compress
$bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
$headerBytes = [System.Text.Encoding]::ASCII.GetBytes(
    "POST /v1/chat/completions HTTP/1.1`r`nAuthorization: Bearer loom-concurrency-smoke-token`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`n`r`n"
)
$validRequest = Join-ByteArrays -Arrays @($headerBytes, $bodyBytes)
$valid = Invoke-GatewayFixtureProbe -RequestBytes $validRequest -ExpectEntered $true
Assert-Equal "Completed" $valid.state "Valid fixture request must complete."
Assert-True ([bool]$valid.capture.valid) "Valid fixture request failed contract validation."
Assert-Equal $unicodeContent ([string]$valid.capture.userContent) "Fixture decoded Content-Length as characters instead of UTF-8 bytes."

Write-Host "Loom daemon concurrency smoke helper tests passed."
