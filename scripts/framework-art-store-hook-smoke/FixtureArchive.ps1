# Deterministic fixture ZIP creation and normalized JSON serialization.
function ConvertTo-SmokeZipRelativePath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path) -or [System.IO.Path]::IsPathRooted($Path)) {
        throw "Fixture archive path must be a non-empty relative path: $Path"
    }
    $normalized = $Path.Replace([char]0x5c, [char]0x2f)
    $reservedNames = '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)'
    foreach ($segment in $normalized.Split([char]0x2f)) {
        if (
            [string]::IsNullOrWhiteSpace($segment) -or
            $segment -eq "." -or
            $segment -eq ".." -or
            $segment.IndexOfAny([System.IO.Path]::GetInvalidFileNameChars()) -ge 0 -or
            $segment.EndsWith(".") -or
            $segment.EndsWith(" ") -or
            $segment -match $reservedNames
        ) {
            throw "Fixture archive path contains an unsafe segment: $Path"
        }
    }
    return $normalized
}

function Get-SmokeFixtureTargetPath {
    param(
        [string]$Stage,
        [string]$RelativePath
    )

    $normalized = ConvertTo-SmokeZipRelativePath -Path $RelativePath
    $nativeRelativePath = $normalized.Replace([char]0x2f, [System.IO.Path]::DirectorySeparatorChar)
    $target = [System.IO.Path]::GetFullPath((Join-Path $Stage $nativeRelativePath))
    return Assert-SmokePathInsideRoot -Root $Stage -Path $target -Label "fixture archive entry"
}

function Assert-SmokeDirectoryTreeIsReal {
    param(
        [string]$Path,
        [string]$Label
    )

    $root = Resolve-SmokeRealDirectory -Path $Path -Label $Label
    $pending = New-Object System.Collections.ArrayList
    [void]$pending.Add($root)
    for ($index = 0; $index -lt $pending.Count; $index++) {
        foreach ($item in @(Get-ChildItem -LiteralPath ([string]$pending[$index]) -Force)) {
            if (Test-SmokeReparsePoint -Item $item) {
                throw "$Label must not contain reparse points: $($item.FullName)"
            }
            if ($item.PSIsContainer) {
                [void]$pending.Add($item.FullName)
            } elseif (-not (Test-Path -LiteralPath $item.FullName -PathType Leaf)) {
                throw "$Label contains an unsupported filesystem entry: $($item.FullName)"
            }
        }
    }
    return $root
}

function Copy-SmokeDirectoryTree {
    param(
        [string]$Source,
        [string]$Destination,
        [string]$Stage
    )

    $sourceRoot = Assert-SmokeDirectoryTreeIsReal -Path $Source -Label "fixture directory source"
    if (Test-Path -LiteralPath $Destination) {
        throw "Fixture destination is already populated: $Destination"
    }
    $destinationParent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
    New-Item -ItemType Directory -Path $Destination | Out-Null
    foreach ($item in @(Get-ChildItem -LiteralPath $sourceRoot -Recurse -Force | Sort-Object FullName)) {
        $relative = $item.FullName.Substring($sourceRoot.Length).TrimStart(
            [System.IO.Path]::DirectorySeparatorChar,
            [System.IO.Path]::AltDirectorySeparatorChar
        )
        $target = Assert-SmokePathInsideRoot -Root $Stage -Path (Join-Path $Destination $relative) -Label "fixture directory copy"
        if ($item.PSIsContainer) {
            New-Item -ItemType Directory -Path $target -Force | Out-Null
        } else {
            $parent = Split-Path -Parent $target
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
            Copy-Item -LiteralPath $item.FullName -Destination $target
        }
    }
}

function Write-SmokeZipArchive {
    param(
        [string]$Stage,
        [string]$Destination
    )

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $output = [System.IO.File]::Open(
        $Destination,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::None
    )
    $archive = $null
    try {
        $archive = [System.IO.Compression.ZipArchive]::new(
            $output,
            [System.IO.Compression.ZipArchiveMode]::Create,
            $false
        )
        $fixedTimestamp = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $files = @(Get-ChildItem -LiteralPath $Stage -Recurse -File -Force | Sort-Object FullName)
        foreach ($file in $files) {
            if (Test-SmokeReparsePoint -Item $file) {
                throw "Fixture stage must not contain reparse points: $($file.FullName)"
            }
            $relativePath = $file.FullName.Substring($Stage.Length).TrimStart(
                [System.IO.Path]::DirectorySeparatorChar,
                [System.IO.Path]::AltDirectorySeparatorChar
            ).Replace([char]0x5c, [char]0x2f)
            $entry = $archive.CreateEntry(
                $relativePath,
                [System.IO.Compression.CompressionLevel]::Optimal
            )
            $entry.LastWriteTime = $fixedTimestamp
            $sourceStream = $null
            $entryStream = $null
            try {
                $sourceStream = $file.OpenRead()
                $entryStream = $entry.Open()
                $sourceStream.CopyTo($entryStream)
            } finally {
                if ($null -ne $entryStream) {
                    $entryStream.Dispose()
                }
                if ($null -ne $sourceStream) {
                    $sourceStream.Dispose()
                }
            }
        }
    } finally {
        if ($null -ne $archive) {
            $archive.Dispose()
        } else {
            $output.Dispose()
        }
    }
}

function Publish-SmokeFileAtomically {
    param(
        [string]$Source,
        [string]$Destination,
        [string]$Label
    )

    if (Test-Path -LiteralPath $Destination) {
        [void](Resolve-SmokeRealFile -Path $Destination -Label $Label)
        [System.IO.File]::Replace($Source, $Destination, $null)
    } else {
        [System.IO.File]::Move($Source, $Destination)
    }
}

function New-ZipFixture {
    param(
        [string]$ZipPath,
        [hashtable]$TextFiles = @{},
        [hashtable]$FileCopies = @{},
        [hashtable]$DirectoryCopies = @{}
    )

    $tempRoot = Resolve-SmokeRealDirectory -Path $env:TEMP -Label "temporary directory"
    $stage = Initialize-SmokeRealDirectory -Path (Join-Path $tempRoot ("loom-zip-stage-" + [System.Guid]::NewGuid().ToString("N"))) -Label "fixture ZIP stage"
    $zipFullPath = [System.IO.Path]::GetFullPath($ZipPath)
    $zipParent = Initialize-SmokeRealDirectory -Path (Split-Path -Parent $zipFullPath) -Label "fixture ZIP parent"
    $zipName = Split-Path -Leaf $zipFullPath
    $temporaryZipPath = Join-Path $zipParent (".$zipName.$([System.Guid]::NewGuid().ToString('N')).tmp")
    $sidecarPath = "$zipFullPath.sha256"
    $temporarySidecarPath = "$temporaryZipPath.sha256"
    try {
        foreach ($entry in $TextFiles.GetEnumerator()) {
            $target = Get-SmokeFixtureTargetPath -Stage $stage -RelativePath ([string]$entry.Key)
            if (Test-Path -LiteralPath $target) {
                throw "Fixture destination is already populated: $target"
            }
            Write-Utf8NoBomFile -Path $target -Content ([string]$entry.Value)
        }
        foreach ($entry in $FileCopies.GetEnumerator()) {
            $target = Get-SmokeFixtureTargetPath -Stage $stage -RelativePath ([string]$entry.Key)
            if (Test-Path -LiteralPath $target) {
                throw "Fixture destination is already populated: $target"
            }
            $source = Resolve-SmokeRealFile -Path ([string]$entry.Value) -Label "fixture file source"
            $parent = Split-Path -Parent $target
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
            Copy-Item -LiteralPath $source -Destination $target
        }
        foreach ($entry in $DirectoryCopies.GetEnumerator()) {
            $target = Get-SmokeFixtureTargetPath -Stage $stage -RelativePath ([string]$entry.Key)
            Copy-SmokeDirectoryTree -Source ([string]$entry.Value) -Destination $target -Stage $stage
        }

        Write-SmokeZipArchive -Stage $stage -Destination $temporaryZipPath
        $zipHash = (Get-FileHash -LiteralPath $temporaryZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8NoBomFile -Path $temporarySidecarPath -Content ("$zipHash  $zipName" + [Environment]::NewLine)
        Publish-SmokeFileAtomically -Source $temporaryZipPath -Destination $zipFullPath -Label "fixture ZIP"
        Publish-SmokeFileAtomically -Source $temporarySidecarPath -Destination $sidecarPath -Label "fixture ZIP checksum"
    } finally {
        foreach ($temporaryPath in @($temporaryZipPath, $temporarySidecarPath)) {
            if (Test-Path -LiteralPath $temporaryPath -PathType Leaf) {
                Remove-Item -LiteralPath $temporaryPath -Force
            }
        }
        [void](Remove-SmokeRealDirectoryTree -Path $stage -ExpectedRoot $tempRoot)
    }
}

function ConvertTo-NormalizedJson {
    param([object]$Value)
    return (($Value | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
}
