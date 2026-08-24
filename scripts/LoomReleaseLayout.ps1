Set-StrictMode -Version Latest

function Assert-LoomSafeRelativePath {
    param([Parameter(Mandatory = $true)][string]$RelativePath)

    if (
        [string]::IsNullOrWhiteSpace($RelativePath) -or
        $RelativePath.IndexOf([char]0) -ge 0 -or
        [regex]::IsMatch($RelativePath, '[\x00-\x1f:]') -or
        [System.IO.Path]::IsPathRooted($RelativePath)
    ) {
        throw "Invalid Loom package-relative path: $RelativePath"
    }

    $normalized = $RelativePath.Replace("/", "\")
    $segments = @($normalized.Split(@("\"), [System.StringSplitOptions]::None))
    foreach ($segment in $segments) {
        $baseName = $segment.Split('.')[0]
        if (
            [string]::IsNullOrWhiteSpace($segment) -or
            $segment -in @(".", "..") -or
            $segment.EndsWith(" ", [System.StringComparison]::Ordinal) -or
            $segment.EndsWith(".", [System.StringComparison]::Ordinal) -or
            $baseName -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$'
        ) {
            throw "Invalid Loom package-relative path: $RelativePath"
        }
    }
    return $normalized
}

function Resolve-LoomPackageRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir,
        [Parameter(Mandatory = $true)][string]$RelativePath
    )

    $normalized = Assert-LoomSafeRelativePath -RelativePath $RelativePath
    $packageFullPath = [System.IO.Path]::GetFullPath($PackageDir).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
    $candidate = [System.IO.Path]::GetFullPath((Join-Path $packageFullPath $normalized))
    $packagePrefix = $packageFullPath + [System.IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Invalid Loom package-relative path: $RelativePath"
    }
    return $candidate
}

function Get-LoomFileDigest {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [System.IO.FileStream]::new(
        [System.IO.Path]::GetFullPath($Path),
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read,
        64KB,
        [System.IO.FileOptions]::SequentialScan
    )
    $algorithm = [System.Security.Cryptography.SHA256]::Create()
    try {
        $length = $stream.Length
        $hash = ([System.BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
        return [pscustomobject]@{ bytes = [int64]$length; sha256 = $hash }
    }
    finally {
        $algorithm.Dispose()
        $stream.Dispose()
    }
}

function Read-LoomBoundedFileBytes {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaxBytes = 4MB
    )

    if ($MaxBytes -lt 0) {
        throw "Loom release byte limit must not be negative: $MaxBytes"
    }
    $stream = [System.IO.FileStream]::new(
        [System.IO.Path]::GetFullPath($Path),
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read,
        64KB,
        [System.IO.FileOptions]::SequentialScan
    )
    try {
        if ($stream.Length -gt $MaxBytes) {
            throw "Loom release file exceeds the ${MaxBytes}-byte limit: $Path"
        }
        $output = [System.IO.MemoryStream]::new([int]$stream.Length)
        try {
            $stream.CopyTo($output)
            return $output.ToArray()
        }
        finally {
            $output.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Read-LoomBoundedTextFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaxBytes = 8MB,
        [System.Text.Encoding]$Encoding = [System.Text.UTF8Encoding]::new($false, $true)
    )

    if ($MaxBytes -lt 0) {
        throw "Loom release byte limit must not be negative: $MaxBytes"
    }
    $stream = [System.IO.FileStream]::new(
        [System.IO.Path]::GetFullPath($Path),
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read,
        64KB,
        [System.IO.FileOptions]::SequentialScan
    )
    try {
        if ($stream.Length -gt $MaxBytes) {
            throw "Loom release text file exceeds the ${MaxBytes}-byte limit: $Path"
        }
        $reader = [System.IO.StreamReader]::new($stream, $Encoding, $true, 4096, $true)
        try {
            return $reader.ReadToEnd()
        }
        finally {
            $reader.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Read-LoomBoundedJsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaxBytes = 8MB
    )

    return Read-LoomBoundedTextFile -Path $Path -MaxBytes $MaxBytes | ConvertFrom-Json
}

function Read-LoomArchiveEntryBytes {
    param(
        [Parameter(Mandatory = $true)][object]$Entry,
        [int64]$MaxBytes = 4MB
    )

    if ($MaxBytes -lt 0) {
        throw "Loom archive entry byte limit must not be negative: $MaxBytes"
    }
    if ([int64]$Entry.Length -gt $MaxBytes) {
        throw "Loom archive entry exceeds the ${MaxBytes}-byte limit: $($Entry.FullName)"
    }
    $input = $Entry.Open()
    $output = [System.IO.MemoryStream]::new([int][Math]::Min([int64]$Entry.Length, [int64][int]::MaxValue))
    try {
        $buffer = New-Object byte[] 81920
        $total = 0L
        while (($read = $input.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $total += $read
            if ($total -gt $MaxBytes) {
                throw "Loom archive entry exceeds the ${MaxBytes}-byte limit: $($Entry.FullName)"
            }
            $output.Write($buffer, 0, $read)
        }
        return $output.ToArray()
    }
    finally {
        $output.Dispose()
        $input.Dispose()
    }
}

function Read-LoomArchiveEntryJson {
    param(
        [Parameter(Mandatory = $true)][object]$Entry,
        [int64]$MaxBytes = 4MB
    )

    $bytes = Read-LoomArchiveEntryBytes -Entry $Entry -MaxBytes $MaxBytes
    return ([System.Text.UTF8Encoding]::new($false, $true)).GetString($bytes) | ConvertFrom-Json
}

function Assert-LoomPathHasNoReparsePoints {
    param(
        [Parameter(Mandatory = $true)][string]$RootPath,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $rootFullPath = [System.IO.Path]::GetFullPath($RootPath).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
    $candidateFullPath = [System.IO.Path]::GetFullPath($Path)
    $rootPrefix = $rootFullPath + [System.IO.Path]::DirectorySeparatorChar
    if (-not [string]::Equals($candidateFullPath, $rootFullPath, [System.StringComparison]::OrdinalIgnoreCase) -and
        -not $candidateFullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Loom path is outside its trusted root: $candidateFullPath"
    }

    $relativePath = $candidateFullPath.Substring($rootFullPath.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar)
    $pathsToCheck = @($rootFullPath)
    $currentPath = $rootFullPath
    foreach ($segment in @($relativePath.Split(@([System.IO.Path]::DirectorySeparatorChar), [System.StringSplitOptions]::RemoveEmptyEntries))) {
        $currentPath = Join-Path $currentPath $segment
        $pathsToCheck += $currentPath
    }
    foreach ($pathToCheck in $pathsToCheck) {
        if (-not (Test-Path -LiteralPath $pathToCheck)) {
            break
        }
        $item = Get-Item -LiteralPath $pathToCheck -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Loom release paths must not contain reparse points: $($item.FullName)"
        }
    }
}

function Get-LoomSafeDescendantFiles {
    param([Parameter(Mandatory = $true)][string]$RootPath)

    $root = [System.IO.Path]::GetFullPath($RootPath).TrimEnd("\", "/")
    Assert-LoomPathHasNoReparsePoints -RootPath $root -Path $root
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($root)
    $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force)) {
            if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Loom release paths must not contain reparse points: $($item.FullName)"
            }
            if ($item.PSIsContainer) {
                $pending.Push($item.FullName)
            }
            else {
                $files.Add([System.IO.FileInfo]$item)
            }
        }
    }
    return @($files)
}

function Get-LoomArchiveFileEntries {
    param(
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [int]$MaxEntries = 65536,
        [int64]$MaxUncompressedBytes = 16GB
    )

    if ($MaxEntries -lt 1 -or $MaxUncompressedBytes -lt 0) {
        throw "Invalid Loom archive resource limit."
    }

    $zipFullPath = [System.IO.Path]::GetFullPath($ZipPath)
    if (-not (Test-Path -LiteralPath $zipFullPath -PathType Leaf)) {
        throw "Loom archive is missing: $zipFullPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath (Split-Path -Parent $zipFullPath) -Path $zipFullPath

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipFullPath)
    try {
        $entries = [System.Collections.Generic.List[string]]::new()
        $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        $entryCount = 0
        $uncompressedBytes = 0L
        foreach ($entry in $archive.Entries) {
            $entryCount += 1
            if ($entryCount -gt $MaxEntries) {
                throw "Loom archive exceeds the ${MaxEntries}-entry limit: $zipFullPath"
            }
            $rawPath = $entry.FullName.TrimEnd("/", "\")
            if ([string]::IsNullOrEmpty($rawPath)) {
                if ($entry.FullName -match '^[\\/]+$') {
                    continue
                }
                throw "Invalid Loom archive entry: $($entry.FullName)"
            }
            try {
                $relativePath = Assert-LoomSafeRelativePath -RelativePath $rawPath
            }
            catch {
                $displayPath = $entry.FullName.Replace("/", "\")
                throw "Invalid Loom archive entry: $displayPath"
            }
            if (-not $seen.Add($relativePath)) {
                throw "Duplicate Loom archive entry: $relativePath"
            }
            if ($entry.Name.Length -eq 0) {
                continue
            }
            $uncompressedBytes += [int64]$entry.Length
            if ($uncompressedBytes -gt $MaxUncompressedBytes) {
                throw "Loom archive exceeds the ${MaxUncompressedBytes}-byte uncompressed limit: $zipFullPath"
            }
            $entries.Add($relativePath)
        }
        return @($entries.ToArray() | Sort-Object)
    }
    finally {
        $archive.Dispose()
    }
}

function Assert-LoomDesktopRootExecutableBoundary {
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir
    )

    $rootExecutables = @(
        Get-ChildItem -LiteralPath $PackageDir -File |
            Where-Object { $_.Extension -ieq ".exe" }
    )
    $isExactEntry = (
        $rootExecutables.Count -eq 1 -and
        [string]::Equals($rootExecutables[0].Name, "Loom.exe", [System.StringComparison]::Ordinal)
    )
    if (-not $isExactEntry) {
        throw "Loom desktop package root must contain exactly one executable named Loom.exe."
    }
}

function Test-LoomArtifactKind {
    param(
        [object]$Artifact,
        [Parameter(Mandatory = $true)][string]$Kind
    )

    return (
        $null -ne $Artifact -and
        [string]::Equals([string]$Artifact.kind, $Kind, [System.StringComparison]::Ordinal)
    )
}

function Get-LoomReleaseLayout {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir,
        [string]$CliExtractRoot = ""
    )

    $packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
    if (-not (Test-Path -LiteralPath $packageFullPath -PathType Container)) {
        throw "Loom package directory is missing: $packageFullPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $packageFullPath -Path $packageFullPath
    Assert-LoomDesktopRootExecutableBoundary -PackageDir $packageFullPath

    $manifestPath = Join-Path $packageFullPath "manifest.json"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Loom package manifest is missing: $manifestPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $packageFullPath -Path $manifestPath
    $manifest = Read-LoomBoundedJsonFile -Path $manifestPath -MaxBytes 8MB

    $desktopExe = Join-Path $packageFullPath "Loom.exe"
    $runtimeRoot = Join-Path $packageFullPath "runtime"
    $daemonExe = Join-Path $runtimeRoot "loom-daemon.exe"
    foreach ($requiredPath in @($desktopExe, $daemonExe)) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Loom package executable is missing: $requiredPath"
        }
        Assert-LoomPathHasNoReparsePoints -RootPath $packageFullPath -Path $requiredPath
    }

    $cliArtifact = $manifest.PSObject.Properties["cliArtifact"]
    if ($null -eq $cliArtifact -or $null -eq $cliArtifact.Value) {
        throw "Loom package manifest is missing cliArtifact."
    }
    $cliEntryName = [string]$cliArtifact.Value.entryName
    if (-not [string]::Equals($cliEntryName, "loom.exe", [System.StringComparison]::Ordinal)) {
        throw "Loom CLI artifact entry must be loom.exe. Actual: $cliEntryName"
    }
    $cliZipName = [string]$cliArtifact.Value.zipName
    if (-not $cliZipName.StartsWith("Loom-CLI-", [System.StringComparison]::Ordinal)) {
        throw "Loom CLI artifact must use the Loom-CLI- naming contract. Actual: $cliZipName"
    }

    $artifactProperty = $manifest.PSObject.Properties["artifacts"]
    if ($null -eq $artifactProperty -or $null -eq $artifactProperty.Value) {
        throw "Loom CLI artifact metadata mismatch."
    }
    $cliZipRecords = @($artifactProperty.Value | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "cli-zip" })
    if ($cliZipRecords.Count -ne 1) {
        throw "Loom CLI artifact metadata mismatch."
    }
    $cliZipRecord = $cliZipRecords[0]
    $cliPath = ([string]$cliArtifact.Value.path).Replace("/", "\")
    $recordPath = ([string]$cliZipRecord.path).Replace("/", "\")
    $metadataMatches = (
        [string]::Equals($cliZipName, [string]$cliZipRecord.name, [System.StringComparison]::Ordinal) -and
        [string]::Equals($cliPath, $recordPath, [System.StringComparison]::Ordinal) -and
        ([int64]$cliArtifact.Value.bytes -eq [int64]$cliZipRecord.bytes) -and
        [string]::Equals([string]$cliArtifact.Value.sha256, [string]$cliZipRecord.sha256, [System.StringComparison]::Ordinal)
    )
    if (-not $metadataMatches) {
        throw "Loom CLI artifact metadata mismatch."
    }

    $cliZip = Resolve-LoomPackageRelativePath -PackageDir $packageFullPath -RelativePath ([string]$cliArtifact.Value.path)
    if (-not (Test-Path -LiteralPath $cliZip -PathType Leaf)) {
        throw "Loom CLI artifact is missing: $cliZip"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $packageFullPath -Path $cliZip
    $cliZipFile = Get-Item -LiteralPath $cliZip
    $cliDigest = Get-LoomFileDigest -Path $cliZip
    $actualCliHash = $cliDigest.sha256
    if (
        -not [string]::Equals($cliZipFile.Name, $cliZipName, [System.StringComparison]::Ordinal) -or
            [int64]$cliDigest.bytes -ne [int64]$cliArtifact.Value.bytes -or
        -not [string]::Equals($actualCliHash, [string]$cliArtifact.Value.sha256, [System.StringComparison]::Ordinal)
    ) {
        throw "Loom CLI artifact metadata mismatch."
    }

    $cliEntryMaxBytes = 512MB
    $cliEntries = @(Get-LoomArchiveFileEntries -ZipPath $cliZip -MaxUncompressedBytes $cliEntryMaxBytes)
    if ($cliEntries.Count -ne 1 -or -not [string]::Equals($cliEntries[0], "loom.exe", [System.StringComparison]::Ordinal)) {
        throw "Loom CLI ZIP must contain exactly one loom.exe entry."
    }

    $cliExe = $null
    if (-not [string]::IsNullOrWhiteSpace($CliExtractRoot)) {
        $cliExtractFullPath = [System.IO.Path]::GetFullPath($CliExtractRoot)
        if (Test-Path -LiteralPath $cliExtractFullPath) {
            Assert-LoomPathHasNoReparsePoints -RootPath $cliExtractFullPath -Path $cliExtractFullPath
            $existingEntries = @(Get-ChildItem -LiteralPath $cliExtractFullPath -Force)
            if ($existingEntries.Count -gt 0) {
                throw "Loom CLI extraction destination must be empty: $cliExtractFullPath"
            }
        }
        else {
            [System.IO.Directory]::CreateDirectory($cliExtractFullPath) | Out-Null
        }
        Assert-LoomPathHasNoReparsePoints -RootPath $cliExtractFullPath -Path $cliExtractFullPath
        $cliExe = Resolve-LoomPackageRelativePath -PackageDir $cliExtractFullPath -RelativePath $cliEntryName
        try {
            $archive = [System.IO.Compression.ZipFile]::OpenRead($cliZip)
            try {
                $entry = $archive.GetEntry($cliEntryName)
                if ($null -eq $entry -or [int64]$entry.Length -gt $cliEntryMaxBytes) {
                    throw "Loom CLI ZIP must contain exactly one bounded loom.exe entry."
                }
                $input = $entry.Open()
                Assert-LoomPathHasNoReparsePoints -RootPath $cliExtractFullPath -Path $cliExtractFullPath
                $output = [System.IO.FileStream]::new(
                    $cliExe,
                    [System.IO.FileMode]::CreateNew,
                    [System.IO.FileAccess]::Write,
                    [System.IO.FileShare]::None
                )
                try {
                    Assert-LoomPathHasNoReparsePoints -RootPath $cliExtractFullPath -Path $cliExe
                    $buffer = New-Object byte[] 81920
                    $total = 0L
                    while (($read = $input.Read($buffer, 0, $buffer.Length)) -gt 0) {
                        $total += $read
                        if ($total -gt $cliEntryMaxBytes) {
                            throw "Loom CLI ZIP entry exceeds its extraction limit."
                        }
                        $output.Write($buffer, 0, $read)
                    }
                    $output.Flush($true)
                }
                finally {
                    $output.Dispose()
                    $input.Dispose()
                }
            }
            finally {
                $archive.Dispose()
            }
            $expandedFiles = @(Get-LoomSafeDescendantFiles -RootPath $cliExtractFullPath)
            if (
                $expandedFiles.Count -ne 1 -or
                -not [string]::Equals($expandedFiles[0].FullName, $cliExe, [System.StringComparison]::OrdinalIgnoreCase) -or
                -not [string]::Equals($expandedFiles[0].Name, "loom.exe", [System.StringComparison]::Ordinal)
            ) {
                throw "Expanded Loom CLI artifact must contain exactly one loom.exe."
            }
            $postExtractionDigest = Get-LoomFileDigest -Path $cliZip
            if (
                [int64]$postExtractionDigest.bytes -ne [int64]$cliDigest.bytes -or
                -not [string]::Equals([string]$postExtractionDigest.sha256, [string]$cliDigest.sha256, [System.StringComparison]::Ordinal)
            ) {
                throw "Loom CLI artifact changed during extraction."
            }
        }
        catch {
            if (Test-Path -LiteralPath $cliExe -PathType Leaf) {
                Assert-LoomPathHasNoReparsePoints -RootPath $cliExtractFullPath -Path $cliExe
                ([System.IO.FileInfo]::new($cliExe)).Delete()
            }
            throw
        }
    }

    return [pscustomobject]@{
        packageDir = $packageFullPath
        manifest = $manifest
        manifestPath = $manifestPath
        desktopExe = $desktopExe
        runtimeRoot = $runtimeRoot
        daemonExe = $daemonExe
        cliZip = $cliZip
        cliExe = $cliExe
    }
}
