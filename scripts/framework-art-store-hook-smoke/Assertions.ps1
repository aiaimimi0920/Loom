# Assertions, UTF-8 output and Unicode fixture labels.
function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    $matches = if ($Expected -is [string] -or $Actual -is [string]) {
        [string]::Equals([string]$Expected, [string]$Actual, [System.StringComparison]::Ordinal)
    } else {
        $Expected -eq $Actual
    }
    if (-not $matches) {
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

function Write-Utf8NoBomFile {
    param(
        [string]$Path,
        [string]$Content
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function ConvertTo-SafeSmokeErrorText {
    param(
        [string]$Text,
        [string[]]$Secrets = @()
    )

    $safe = [string]$Text
    foreach ($secret in @($Secrets)) {
        if (-not [string]::IsNullOrEmpty($secret)) {
            $safe = $safe.Replace($secret, "<redacted>")
        }
    }
    $safe = [regex]::Replace($safe, '(?i)Bearer\s+[A-Za-z0-9._~+/=-]+', 'Bearer <redacted>')
    if ($safe.Length -gt 8192) {
        $safe = $safe.Substring(0, 8192) + "...<truncated>"
    }
    return $safe
}
