<# Owns daemon polling, authenticated JSON requests, and background job decoding. #>

function Wait-ForPath {
    param(
        [string]$Path,
        [int]$TimeoutSeconds,
        [AllowNull()]$Job
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            return
        }
        if ($null -ne $Job -and $Job.State -eq "Failed") {
            $jobError = (Receive-Job -Job $Job -Keep -ErrorAction SilentlyContinue | Out-String).Trim()
            throw "Fixture job failed: $jobError"
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for fixture path: $Path"
}

function Test-ExactProcessAlive {
    param(
        [int]$ProcessId,
        [string]$ExpectedExecutablePath,
        [AllowNull()][object]$ExpectedStartTimeUtcTicks
    )

    $snapshot = Get-ProcessSnapshotById -ProcessId $ProcessId
    if ($null -eq $snapshot -or -not (Test-SamePath -Left $snapshot.ExecutablePath -Right $ExpectedExecutablePath)) {
        return $false
    }
    return ($null -eq $ExpectedStartTimeUtcTicks -or $snapshot.startTimeUtcTicks -eq [long]$ExpectedStartTimeUtcTicks)
}

function Invoke-JsonGet {
    param(
        [string]$Uri,
        [int]$TimeoutSeconds = 15
    )

    return Invoke-RestMethod -Uri $Uri -Method Get -Headers $script:DaemonAuthHeaders -TimeoutSec $TimeoutSeconds
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body,
        [int]$TimeoutSeconds = 30
    )

    $json = $Body | ConvertTo-Json -Depth 40 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -Headers $script:DaemonAuthHeaders -ContentType "application/json" -Body $json -TimeoutSec $TimeoutSeconds
}

function Receive-JsonJob {
    param([System.Management.Automation.Job]$Job)

    $lines = @(Receive-Job -Job $Job -ErrorAction Stop | ForEach-Object { $_.ToString() })
    $text = ($lines -join [Environment]::NewLine).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "Background invoke job returned no JSON."
    }
    return $text | ConvertFrom-Json
}

function Wait-ForDaemonStatus {
    param(
        [string]$BaseUrl,
        [System.Diagnostics.Process]$Process,
        [string]$ExpectedExecutablePath,
        [AllowNull()][object]$ExpectedStartTimeUtcTicks,
        [int]$TimeoutSeconds = 45
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "loom-daemon exited before status became ready with code $($Process.ExitCode)."
        }
        if (-not (Test-ExactProcessAlive `
            -ProcessId $Process.Id `
            -ExpectedExecutablePath $ExpectedExecutablePath `
            -ExpectedStartTimeUtcTicks $ExpectedStartTimeUtcTicks)) {
            throw "loom-daemon process identity changed or process exited."
        }
        try {
            $health = Invoke-JsonGet -Uri "$BaseUrl/health" -TimeoutSeconds 2
            $status = Invoke-JsonGet -Uri "$BaseUrl/status" -TimeoutSeconds 2
            if ([string]$health.status -eq "ok" -and [string]$status.status -eq "ready") {
                return $status
            }
        }
        catch {
        }
        Start-Sleep -Milliseconds 200
    }
    throw "Timed out waiting for Loom daemon at $BaseUrl"
}

function Restore-EnvironmentValue {
    param(
        [string]$Name,
        [AllowNull()][string]$Value
    )

    [Environment]::SetEnvironmentVariable($Name, $Value, "Process")
}
