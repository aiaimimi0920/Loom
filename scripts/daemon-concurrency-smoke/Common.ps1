<# Owns assertions, bounded evidence IO, and text redaction for the daemon concurrency smoke. #>

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

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Write-Utf8NoBom {
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

function Write-JsonEvidence {
    param(
        [string]$Path,
        [object]$Value
    )

    Write-Utf8NoBom -Path $Path -Content (ConvertTo-Json -InputObject $Value -Depth 40)
}

function Redact-Text {
    param([AllowNull()][string]$Text)

    if ($null -eq $Text) {
        return ""
    }
    $redacted = $Text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)([?&](?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)=)[^&\s#]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)("(?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)"\s*:\s*")[^"]*"', '$1<redacted>"'
    $redacted = $redacted -replace '(?i)((?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)\s*[=:]\s*)[^\s,;}\r\n&]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)(LOOM_(?:DAEMON|GATEWAY)_TOKEN\s*[=:]\s*)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted.Replace("loom-concurrency-smoke-token", "<redacted>")
    return $redacted
}

function Read-BoundedUtf8Text {
    param(
        [string]$Path,
        [int]$MaximumBytes = (4 * 1024 * 1024)
    )

    if ($MaximumBytes -lt 1024) {
        throw "MaximumBytes must be at least 1024."
    }
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        if ($stream.Length -le $MaximumBytes) {
            $buffer = New-Object byte[] ([int]$stream.Length)
            $totalRead = 0
            while ($totalRead -lt $buffer.Length) {
                $read = $stream.Read($buffer, $totalRead, $buffer.Length - $totalRead)
                if ($read -eq 0) { break }
                $totalRead += $read
            }
            return [System.Text.Encoding]::UTF8.GetString($buffer, 0, $totalRead)
        }

        $partBytes = [int]($MaximumBytes / 2)
        $head = New-Object byte[] $partBytes
        $tail = New-Object byte[] $partBytes
        $headRead = 0
        while ($headRead -lt $head.Length) {
            $read = $stream.Read($head, $headRead, $head.Length - $headRead)
            if ($read -eq 0) { break }
            $headRead += $read
        }
        [void]$stream.Seek(-$partBytes, [System.IO.SeekOrigin]::End)
        $tailRead = 0
        while ($tailRead -lt $tail.Length) {
            $read = $stream.Read($tail, $tailRead, $tail.Length - $tailRead)
            if ($read -eq 0) { break }
            $tailRead += $read
        }
        $omitted = $stream.Length - $headRead - $tailRead
        $headText = [System.Text.Encoding]::UTF8.GetString($head, 0, $headRead)
        $tailText = [System.Text.Encoding]::UTF8.GetString($tail, 0, $tailRead)
        return "$headText`r`n<loom-evidence-truncated omittedBytes=$omitted>`r`n$tailText"
    }
    finally {
        $stream.Dispose()
    }
}

function Write-RedactedFile {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (Test-Path -LiteralPath $SourcePath -PathType Leaf) {
        $content = Read-BoundedUtf8Text -Path $SourcePath
        Write-Utf8NoBom -Path $DestinationPath -Content (Redact-Text $content)
    }
}
