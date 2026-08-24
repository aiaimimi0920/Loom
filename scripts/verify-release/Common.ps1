<# Owns one release-script responsibility. #>

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
    }
    else {
        $Expected -eq $Actual
    }
    if (-not $matches) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Get-Sha256HexForBytes {
    param([byte[]]$Bytes)

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [System.BitConverter]::ToString($sha256.ComputeHash($Bytes)).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Invoke-CapturedPowerShell {
    param([string[]]$Arguments)

    $previousErrorActionPreference = $ErrorActionPreference
    $output = [System.Collections.Generic.List[string]]::new()
    $capturedBytes = 0L
    $truncated = $false
    $exitCode = 1
    try {
        $ErrorActionPreference = "Continue"
        & powershell.exe @Arguments 2>&1 | ForEach-Object {
            $line = $_.ToString()
            $line = [regex]::Replace($line, '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+', '$1=[REDACTED]')
            $lineBytes = [System.Text.Encoding]::UTF8.GetByteCount($line) + 2
            if ($capturedBytes + $lineBytes -le 4MB) {
                $output.Add($line)
                $capturedBytes += $lineBytes
            }
            else {
                $truncated = $true
            }
        }
        $exitCode = $LASTEXITCODE
    }
    catch {
        $line = [regex]::Replace($_.Exception.ToString(), '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+', '$1=[REDACTED]')
        $output.Add($line.Substring(0, [Math]::Min($line.Length, 64KB)))
        $exitCode = 1
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    return [pscustomobject]@{
        exitCode = $exitCode
        output = @($output)
        truncated = $truncated
    }
}

function Get-LoomVerifiedFileDigest {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Get-Variable -Name LoomVerifiedFileDigests -Scope Script -ErrorAction SilentlyContinue)) {
        $script:LoomVerifiedFileDigests = @{}
    }
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $itemBefore = Get-Item -LiteralPath $fullPath -Force
    $key = $fullPath.ToUpperInvariant()
    $digest = Get-LoomFileDigest -Path $fullPath
    $itemAfter = Get-Item -LiteralPath $fullPath -Force
    if (
        [int64]$itemBefore.Length -ne [int64]$itemAfter.Length -or
        [int64]$itemBefore.LastWriteTimeUtc.Ticks -ne [int64]$itemAfter.LastWriteTimeUtc.Ticks
    ) {
        throw "Package file changed during verification: $fullPath"
    }
    if ($script:LoomVerifiedFileDigests.ContainsKey($key)) {
        $cached = $script:LoomVerifiedFileDigests[$key]
        if (
            [int64]$cached.bytes -ne [int64]$digest.bytes -or
            -not [string]::Equals([string]$cached.sha256, [string]$digest.sha256, [System.StringComparison]::Ordinal)
        ) {
            throw "Package file changed during verification: $fullPath"
        }
    }
    $record = [pscustomobject]@{
        bytes = [int64]$digest.bytes
        sha256 = [string]$digest.sha256
        lastWriteTicks = [int64]$itemAfter.LastWriteTimeUtc.Ticks
    }
    $script:LoomVerifiedFileDigests[$key] = $record
    return $record
}

function Read-LoomVerifiedTextFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaxBytes = 8MB,
        [System.Text.Encoding]$Encoding = [System.Text.UTF8Encoding]::new($false, $true)
    )

    $bytes = Read-LoomBoundedFileBytes -Path $Path -MaxBytes $MaxBytes
    $digest = Get-LoomVerifiedFileDigest -Path $Path
    $bytesHash = Get-Sha256HexForBytes -Bytes $bytes
    if (
        [int64]$digest.bytes -ne [int64]$bytes.Length -or
        -not [string]::Equals([string]$digest.sha256, $bytesHash, [System.StringComparison]::Ordinal)
    ) {
        throw "Package file changed during verification: $Path"
    }
    return $Encoding.GetString($bytes)
}

function Read-LoomVerifiedJsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaxBytes = 8MB
    )

    return Read-LoomVerifiedTextFile -Path $Path -MaxBytes $MaxBytes | ConvertFrom-Json
}

function Resolve-PackageRelativePath {
    param(
        [string]$BasePath,
        [string]$RelativePath
    )

    $normalized = Assert-LoomSafeRelativePath -RelativePath $RelativePath
    $resolved = Resolve-LoomPackageRelativePath -PackageDir $BasePath -RelativePath $normalized
    Assert-LoomPathHasNoReparsePoints -RootPath $BasePath -Path $resolved
    return $resolved
}

function Get-ManifestRecord {
    param(
        [object]$Manifest,
        [string]$Name
    )

    $property = $Manifest.PSObject.Properties[$Name]
    Assert-True -Condition ($null -ne $property) -Message "Manifest is missing $Name."
    return @($property.Value)
}

function Assert-FileRecord {
    param(
        [string]$PackagePath,
        [object]$Record
    )

    $relativePath = [string]$Record.path
    Assert-True -Condition (-not [string]::IsNullOrWhiteSpace($relativePath)) -Message "Manifest file record has an empty path."
    $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
    Assert-True -Condition (Test-Path -LiteralPath $filePath -PathType Leaf) -Message "Manifest file is missing: $relativePath"
    $digest = Get-LoomVerifiedFileDigest -Path $filePath
    Assert-Equal -Expected ([int64]$Record.bytes) -Actual ([int64]$digest.bytes) -Message "Manifest byte count mismatch for $relativePath."
    Assert-Equal -Expected ([string]$Record.sha256) -Actual ([string]$digest.sha256) -Message "Manifest SHA-256 mismatch for $relativePath."
    return Assert-LoomSafeRelativePath -RelativePath $relativePath
}

function Get-ChecksumEntries {
    param([string]$PackagePath)

    $checksumPath = Join-Path $PackagePath "checksums.sha256"
    Assert-True -Condition (Test-Path -LiteralPath $checksumPath -PathType Leaf) -Message "Missing checksums.sha256."
    $entries = @{}
    Assert-LoomPathHasNoReparsePoints -RootPath $PackagePath -Path $checksumPath
    $checksumText = Read-LoomBoundedTextFile -Path $checksumPath -MaxBytes 4MB -Encoding ([System.Text.ASCIIEncoding]::new())
    foreach ($line in @($checksumText -split '\r?\n')) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            throw "Invalid checksum line: $line"
        }
        $hash = $Matches[1].ToLowerInvariant()
        $relativePath = Assert-LoomSafeRelativePath -RelativePath $Matches[2]
        Assert-True -Condition (-not $entries.ContainsKey($relativePath)) -Message "Duplicate checksum entry: $relativePath"
        $entries[$relativePath] = $hash
    }
    return $entries
}

function Assert-Checksums {
    param(
        [string]$PackagePath,
        [hashtable]$Entries
    )

    $actualFiles = @(Get-LoomSafeDescendantFiles -RootPath $PackagePath | ForEach-Object {
        $_.FullName.Substring($PackagePath.Length + 1).Replace("/", "\")
    } | Where-Object { $_ -ne "checksums.sha256" } | Sort-Object)
    Assert-Equal -Expected ($actualFiles.Count) -Actual ($Entries.Keys.Count) -Message "Checksum entry count must equal all package files except checksums.sha256."
    foreach ($relativePath in $actualFiles) {
        Assert-True -Condition $Entries.ContainsKey($relativePath) -Message "Missing checksum entry: $relativePath"
        $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
        $digest = Get-LoomVerifiedFileDigest -Path $filePath
        Assert-Equal -Expected $Entries[$relativePath] -Actual ([string]$digest.sha256) -Message "Checksum mismatch for $relativePath"
    }
    foreach ($relativePath in $Entries.Keys) {
        Assert-True -Condition ($actualFiles -contains $relativePath) -Message "Checksum references an untracked package file: $relativePath"
    }
}
