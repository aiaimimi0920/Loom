<# Owns temporary-root cleanup, failure diagnostics, evidence runs, UTF-8 writes, and secret redaction. #>

function Remove-SmokeTempRoot {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    try {
        if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
            return
        }
        $tempRoot = [System.IO.Path]::GetFullPath($env:TEMP).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
        $candidate = Get-Item -LiteralPath ([System.IO.Path]::GetFullPath($Path)) -Force
        $candidateParent = [System.IO.Path]::GetFullPath($candidate.Parent.FullName).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
        if (-not [string]::Equals($tempRoot, $candidateParent, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove a smoke temp root outside TEMP: $($candidate.FullName)"
        }
        if ($candidate.Name -notmatch '^[A-Za-z0-9._-]+-\d+-[0-9A-Fa-f]{32}$') {
            throw "Refusing to remove an unrecognized smoke temp root: $($candidate.FullName)"
        }
        if (($candidate.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to remove a reparse-point smoke temp root: $($candidate.FullName)"
        }
        Remove-Item -LiteralPath $candidate.FullName -Recurse -Force -ErrorAction Stop
    } catch {
        Write-Warning "Failed to remove smoke temp root '$Path': $($_.Exception.Message)"
    }
}

function Get-SmokeFailureEvidenceFiles {
    param([string]$Root)

    $pending = New-Object System.Collections.Stack
    $pending.Push((Get-Item -LiteralPath $Root -Force))
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($item in @(Get-ChildItem -LiteralPath $directory.FullName -Force -ErrorAction SilentlyContinue)) {
            if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                continue
            }
            if ($item.PSIsContainer) {
                $pending.Push($item)
            } else {
                $item
            }
        }
    }
}

function Assert-SmokeEvidencePath {
    param(
        [string]$Root,
        [string]$Path
    )

    $rootFullPath = [System.IO.Path]::GetFullPath($Root).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
    $candidateFullPath = [System.IO.Path]::GetFullPath($Path)
    $rootPrefix = $rootFullPath + [System.IO.Path]::DirectorySeparatorChar
    if (-not [string]::Equals($rootFullPath, $candidateFullPath, [System.StringComparison]::OrdinalIgnoreCase) -and
        -not $candidateFullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Smoke evidence path escaped its root: $candidateFullPath"
    }

    $relativePath = $candidateFullPath.Substring($rootFullPath.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar)
    $currentPath = $rootFullPath
    foreach ($segment in @($relativePath.Split(@([System.IO.Path]::DirectorySeparatorChar), [System.StringSplitOptions]::RemoveEmptyEntries))) {
        $currentPath = Join-Path $currentPath $segment
        if (-not (Test-Path -LiteralPath $currentPath)) {
            break
        }
        $item = Get-Item -LiteralPath $currentPath -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Smoke evidence paths must not contain reparse points: $($item.FullName)"
        }
    }
}

function Test-SmokeSensitiveEvidencePath {
    param([string]$RelativePath)

    foreach ($segment in @($RelativePath -split '[\\/]')) {
        if ($segment -match '(?i)(^|[-_.])(auth|authorization|credentials?|cookies?|passwords?|secrets?|tokens?)([-_.]|$)') {
            return $true
        }
    }
    return $false
}

function Save-SmokeFailureEvidence {
    param(
        [string]$TempRoot,
        [string]$Label
    )

    if ([string]::IsNullOrWhiteSpace($TempRoot) -or -not (Test-Path -LiteralPath $TempRoot)) {
        return $null
    }
    $tempItem = Get-Item -LiteralPath $TempRoot -Force
    if (($tempItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Smoke failure temp root must not be a reparse point: $($tempItem.FullName)"
    }
    $TempRoot = $tempItem.FullName

    $safeVersion = $VersionId -replace "[^A-Za-z0-9._-]", "_"
    $safeLabel = $Label -replace "[^A-Za-z0-9._-]", "_"
    $evidenceDir = Get-SmokeEvidenceDir
    $failureRoot = Join-Path $evidenceDir "failures"
    if (-not (Test-Path -LiteralPath $failureRoot)) {
        New-Item -ItemType Directory -Path $failureRoot | Out-Null
    }
    Assert-SmokeEvidencePath -Root $evidenceDir -Path $failureRoot
    $failureEvidencePath = Join-Path $failureRoot "$safeVersion-$safeLabel-$PID-$([System.Guid]::NewGuid().ToString("N"))"
    New-Item -ItemType Directory -Path $failureEvidencePath | Out-Null
    Assert-SmokeEvidencePath -Root $evidenceDir -Path $failureEvidencePath

    Get-SmokeFailureEvidenceFiles -Root $TempRoot | ForEach-Object {
        $relativePath = $_.FullName.Substring($TempRoot.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
        if (Test-SmokeSensitiveEvidencePath -RelativePath $relativePath) {
            return
        }
        if ($_.Length -gt (16 * 1024 * 1024)) {
            return
        }
        $destination = Join-Path $failureEvidencePath $relativePath
        $destinationDir = Split-Path -Parent $destination
        New-Item -ItemType Directory -Force -Path $destinationDir | Out-Null
        $extension = $_.Extension.ToLowerInvariant()
        if ($extension -in @(".json", ".log", ".txt", ".toml")) {
            $content = Get-Content -LiteralPath $_.FullName -Raw
            Write-Utf8NoBomFile -Path $destination -Content (Redact-SmokeJsonContent -Content $content)
        } else {
            Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
        }
    }

    Write-Warning "Saved smoke failure diagnostics to failureEvidencePath=$failureEvidencePath"
    return $failureEvidencePath
}

function Write-Utf8NoBomFile {
    param(
        [string]$Path,
        [string]$Content
    )

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-SmokeEvidenceDir {
    $evidenceDir = [System.IO.Path]::GetFullPath($EvidenceRoot)
    New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null
    $evidenceItem = Get-Item -LiteralPath $evidenceDir -Force
    if (($evidenceItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Smoke evidence root must not be a reparse point: $($evidenceItem.FullName)"
    }
    return $evidenceDir
}

function New-SmokeEvidenceRunId {
    $safeApps = (@($resolvedApps) -join "-") -replace "[^A-Za-z0-9._-]", "_"
    return "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$safeApps-$PID-$([System.Guid]::NewGuid().ToString('N'))"
}

function Initialize-SmokeEvidenceRun {
    if ([string]::IsNullOrWhiteSpace($script:SmokeEvidenceRunId)) {
        $script:SmokeEvidenceRunId = New-SmokeEvidenceRunId
    }

    if ([string]::IsNullOrWhiteSpace($script:SmokeEvidenceRunDir)) {
        $evidenceDir = Get-SmokeEvidenceDir
        $runRoot = Join-Path $evidenceDir "runs"
        if (-not (Test-Path -LiteralPath $runRoot)) {
            New-Item -ItemType Directory -Path $runRoot | Out-Null
        }
        Assert-SmokeEvidencePath -Root $evidenceDir -Path $runRoot
        $script:SmokeEvidenceRunDir = Join-Path $runRoot $script:SmokeEvidenceRunId
        New-Item -ItemType Directory -Path $script:SmokeEvidenceRunDir | Out-Null
        Assert-SmokeEvidencePath -Root $evidenceDir -Path $script:SmokeEvidenceRunDir
    }
}

function Get-SmokeRunEvidenceDir {
    Initialize-SmokeEvidenceRun
    return $script:SmokeEvidenceRunDir
}

function Write-SmokeJsonEvidence {
    param(
        [string]$FileName,
        [object]$Value,
        [switch]$Latest
    )

    if ([string]::IsNullOrWhiteSpace($FileName) -or
        -not [string]::Equals([System.IO.Path]::GetFileName($FileName), $FileName, [System.StringComparison]::Ordinal) -or
        [System.IO.Path]::GetExtension($FileName) -ne ".json") {
        throw "Smoke evidence file name must be a single JSON path segment: $FileName"
    }

    $root = Get-SmokeRunEvidenceDir
    if ($Latest) {
        $evidenceDir = Get-SmokeEvidenceDir
        $root = Join-Path $evidenceDir "latest"
        if (-not (Test-Path -LiteralPath $root)) {
            New-Item -ItemType Directory -Path $root | Out-Null
        }
        Assert-SmokeEvidencePath -Root $evidenceDir -Path $root
    }
    $evidencePath = Join-Path $root $FileName
    $json = Redact-SmokeJsonContent -Content ($Value | ConvertTo-Json -Depth 40)
    Write-Utf8NoBomFile -Path $evidencePath -Content $json
    return $evidencePath
}

function Redact-SmokeJsonContent {
    param([AllowEmptyString()][string]$Content)

    $redacted = $Content -replace '("(?:authToken|authorization|accessToken|apiKey|api_key|password|secret|token|cookie)"\s*:\s*")[^"]*(")', '$1<redacted>$2'
    $redacted = $redacted -replace '(?im)^(\s*(?:auth_token|authorization|access_token|api_key|password|secret|token|cookie)\s*=\s*)(?:"[^"]*"|''[^'']*''|[^\r\n#]+)', '$1<redacted>'
    $redacted = $redacted -replace '(?im)^(\s*(?:auth_token|authorization|access_token|api_key|password|secret|token|cookie)\s*:\s*)[^\r\n]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)([?&](?:auth_token|access_token|api_key|password|secret|token|cookie)=)[^&#\s"'']+', '$1<redacted>'
    $redacted = $redacted -replace '(Bearer\s+)[A-Za-z0-9._~+/\-=]+', '$1<redacted>'
    return $redacted
}

function Redact-SmokeFailureText {
    param([AllowEmptyString()][string]$Text)

    return Redact-SmokeJsonContent -Content $Text
}
