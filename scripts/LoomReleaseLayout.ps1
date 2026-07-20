Set-StrictMode -Version Latest

function Resolve-LoomPackageRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$PackageDir,
        [Parameter(Mandatory = $true)][string]$RelativePath
    )

    if ([System.IO.Path]::IsPathRooted($RelativePath) -or $RelativePath.Contains("..")) {
        throw "Invalid Loom package-relative path: $RelativePath"
    }
    return [System.IO.Path]::GetFullPath((Join-Path $PackageDir $RelativePath))
}

function Get-LoomArchiveFileEntries {
    param(
        [Parameter(Mandatory = $true)][string]$ZipPath
    )

    $zipFullPath = [System.IO.Path]::GetFullPath($ZipPath)
    if (-not (Test-Path -LiteralPath $zipFullPath -PathType Leaf)) {
        throw "Loom archive is missing: $zipFullPath"
    }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipFullPath)
    try {
        $entries = @()
        foreach ($entry in $archive.Entries) {
            $relativePath = $entry.FullName.Replace("/", "\")
            $segments = @($relativePath.Split(@("\"), [System.StringSplitOptions]::RemoveEmptyEntries))
            if (
                [string]::IsNullOrWhiteSpace($relativePath) -or
                [System.IO.Path]::IsPathRooted($relativePath) -or
                $relativePath.StartsWith("\") -or
                ($segments -contains "..")
            ) {
                throw "Invalid Loom archive entry: $relativePath"
            }
            if ([string]::IsNullOrWhiteSpace($entry.Name)) {
                continue
            }
            $entries += $relativePath
        }
        return @($entries | Sort-Object)
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
    Assert-LoomDesktopRootExecutableBoundary -PackageDir $packageFullPath

    $manifestPath = Join-Path $packageFullPath "manifest.json"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Loom package manifest is missing: $manifestPath"
    }
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json

    $desktopExe = Join-Path $packageFullPath "Loom.exe"
    $runtimeRoot = Join-Path $packageFullPath "runtime"
    $daemonExe = Join-Path $runtimeRoot "loom-daemon.exe"
    foreach ($requiredPath in @($desktopExe, $daemonExe)) {
        if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
            throw "Loom package executable is missing: $requiredPath"
        }
    }

    $cliArtifact = $manifest.PSObject.Properties["cliArtifact"]
    if ($null -eq $cliArtifact -or $null -eq $cliArtifact.Value) {
        throw "Loom package manifest is missing cliArtifact."
    }
    $cliEntryName = [string]$cliArtifact.Value.entryName
    if ($cliEntryName -ne "loom.exe") {
        throw "Loom CLI artifact entry must be loom.exe. Actual: $cliEntryName"
    }
    $cliZipName = [string]$cliArtifact.Value.zipName
    if (-not $cliZipName.StartsWith("Loom-CLI-")) {
        throw "Loom CLI artifact must use the Loom-CLI- naming contract. Actual: $cliZipName"
    }

    $artifactProperty = $manifest.PSObject.Properties["artifacts"]
    if ($null -eq $artifactProperty -or $null -eq $artifactProperty.Value) {
        throw "Loom CLI artifact metadata mismatch."
    }
    $cliZipRecords = @($artifactProperty.Value | Where-Object { [string]$_.kind -eq "cli-zip" })
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
    $cliZipFile = Get-Item -LiteralPath $cliZip
    $actualCliHash = (Get-FileHash -LiteralPath $cliZip -Algorithm SHA256).Hash.ToLowerInvariant()
    if (
        -not [string]::Equals($cliZipFile.Name, $cliZipName, [System.StringComparison]::Ordinal) -or
        [int64]$cliZipFile.Length -ne [int64]$cliArtifact.Value.bytes -or
        -not [string]::Equals($actualCliHash, [string]$cliArtifact.Value.sha256, [System.StringComparison]::Ordinal)
    ) {
        throw "Loom CLI artifact metadata mismatch."
    }

    $cliEntries = @(Get-LoomArchiveFileEntries -ZipPath $cliZip)
    if ($cliEntries.Count -ne 1 -or -not [string]::Equals($cliEntries[0], "loom.exe", [System.StringComparison]::Ordinal)) {
        throw "Loom CLI ZIP must contain exactly one loom.exe entry."
    }

    $cliExe = $null
    if (-not [string]::IsNullOrWhiteSpace($CliExtractRoot)) {
        $cliExtractFullPath = [System.IO.Path]::GetFullPath($CliExtractRoot)
        if (Test-Path -LiteralPath $cliExtractFullPath) {
            $existingEntries = @(Get-ChildItem -LiteralPath $cliExtractFullPath -Force)
            if ($existingEntries.Count -gt 0) {
                throw "Loom CLI extraction destination must be empty: $cliExtractFullPath"
            }
        }
        else {
            New-Item -ItemType Directory -Path $cliExtractFullPath -Force | Out-Null
        }
        Expand-Archive -LiteralPath $cliZip -DestinationPath $cliExtractFullPath
        $cliExe = Join-Path $cliExtractFullPath $cliEntryName
        $expandedFiles = @(Get-ChildItem -LiteralPath $cliExtractFullPath -Recurse -File)
        if (
            $expandedFiles.Count -ne 1 -or
            -not [string]::Equals($expandedFiles[0].FullName, $cliExe, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not [string]::Equals($expandedFiles[0].Name, "loom.exe", [System.StringComparison]::Ordinal)
        ) {
            throw "Expanded Loom CLI artifact must contain exactly one loom.exe."
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
