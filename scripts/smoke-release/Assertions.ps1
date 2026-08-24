<# Owns smoke assertions, JSON property access, local temp creation, JSON file readiness, and JSON HTTP verbs. #>

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Contains {
    param(
        [string]$Needle,
        [string]$Haystack,
        [string]$Message
    )

    if (-not $Haystack.Contains($Needle)) {
        throw "$Message Missing=[$Needle]"
    }
}

function Assert-PathExists {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing release smoke path: $Path"
    }
}

function Get-JsonPropertyOrNull {
    param(
        [object]$Object,
        [string]$Name
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function New-SmokeTempRoot {
    param([string]$Prefix)

    if ([string]::IsNullOrWhiteSpace($Prefix) -or $Prefix -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Smoke temp prefix must be a safe path segment."
    }
    if ([string]::IsNullOrWhiteSpace($env:TEMP)) {
        throw "TEMP is not configured for smoke execution."
    }

    $tempRoot = [System.IO.Path]::GetFullPath($env:TEMP)
    if (-not (Test-Path -LiteralPath $tempRoot -PathType Container)) {
        throw "TEMP directory is missing: $tempRoot"
    }
    $suffix = [System.Guid]::NewGuid().ToString("N")
    $path = [System.IO.Path]::GetFullPath((Join-Path $tempRoot "$Prefix-$PID-$suffix"))
    New-Item -ItemType Directory -Path $path | Out-Null
    $created = Get-Item -LiteralPath $path -Force
    if (($created.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Smoke temp root unexpectedly resolved to a reparse point: $path"
    }
    return $created.FullName
}

function Wait-ForFileJson {
    param(
        [string]$Path,
        [int]$TimeoutSeconds = 20
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastJsonError = $null
    do {
        try {
            if (Test-Path -LiteralPath $Path) {
                $raw = Get-Content -LiteralPath $Path -Raw
                if (-not [string]::IsNullOrWhiteSpace($raw)) {
                    return $raw | ConvertFrom-Json
                }
            }
        } catch {
            $lastJsonError = $_
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    if ($null -ne $lastJsonError) {
        throw "Timed out waiting for complete JSON file: $Path LastError=$($lastJsonError.Exception.Message)"
    }
    throw "Timed out waiting for JSON file: $Path"
}

function Invoke-JsonGet {
    param(
        [string]$Uri,
        [hashtable]$Headers = $script:DaemonAuthHeaders
    )

    Assert-SmokeLoopbackUri -Uri $Uri
    return Invoke-RestMethod -Uri $Uri -Method Get -Headers $Headers -TimeoutSec 10 -MaximumRedirection 0
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body,
        [int]$TimeoutSec = 20,
        [hashtable]$Headers = $script:DaemonAuthHeaders
    )

    Assert-SmokeLoopbackUri -Uri $Uri
    $json = $Body | ConvertTo-Json -Depth 20
    try {
        return Invoke-RestMethod -Uri $Uri -Method Post -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec $TimeoutSec -MaximumRedirection 0
    }
    catch {
        throw "Smoke POST failed for ${Uri} (timeoutSec=$TimeoutSec): $($_.Exception.Message)"
    }
}

function Invoke-JsonPut {
    param(
        [string]$Uri,
        [object]$Body,
        [hashtable]$Headers = $script:DaemonAuthHeaders
    )

    Assert-SmokeLoopbackUri -Uri $Uri
    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $Uri -Method Put -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec 20 -MaximumRedirection 0
}

function Invoke-JsonDelete {
    param(
        [string]$Uri,
        [hashtable]$Headers = $script:DaemonAuthHeaders
    )

    Assert-SmokeLoopbackUri -Uri $Uri
    return Invoke-RestMethod -Uri $Uri -Method Delete -Headers $Headers -TimeoutSec 20 -MaximumRedirection 0
}

function Assert-SmokeLoopbackUri {
    param([string]$Uri)

    $parsedUri = $null
    if (-not [System.Uri]::TryCreate($Uri, [System.UriKind]::Absolute, [ref]$parsedUri)) {
        throw "Smoke HTTP URI must be absolute: $Uri"
    }
    if ($parsedUri.Scheme -notin @("http", "https") -or -not $parsedUri.IsLoopback) {
        throw "Smoke HTTP URI must target loopback over HTTP: $Uri"
    }
    if (-not [string]::IsNullOrEmpty($parsedUri.UserInfo)) {
        throw "Smoke HTTP URI must not contain user information: $Uri"
    }
}
