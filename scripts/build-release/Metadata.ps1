<# Owns one release-script responsibility. #>

function New-BuildInfo {
    param(
        [string]$ResolvedVersionId,
        [string]$ResolvedOutputRoot,
        [System.Collections.Specialized.OrderedDictionary]$Catalog,
        [string]$GitHead,
        [object]$GitDirty
    )

    $lines = @(
        "Loom Windows release artifact"
        "versionId=$ResolvedVersionId"
        "target=$targetName"
        "repository=https://github.com/aiaimimi0920/Loom"
        "sourcePaths=."
        "gitHead=$GitHead"
        "gitDirty=$GitDirty"
        "outputRoot=$ResolvedOutputRoot"
        ""
        "Commands:"
    )
    foreach ($command in $Catalog.commands) {
        $lines += "- $($command.display)"
    }
    return ($lines -join [Environment]::NewLine) + [Environment]::NewLine
}

function Get-RelativeFiles {
    param([string]$BasePath)

    $base = [System.IO.Path]::GetFullPath($BasePath).TrimEnd("\", "/")
    return @(Get-LoomSafeDescendantFiles -RootPath $base | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($base.Length + 1).Replace("\", "/")
        [ordered]@{ file = $_; relative = $relative }
    })
}

function Write-Checksums {
    param([string]$Destination)

    $checksumPath = Join-Path $Destination "checksums.sha256"
    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($entry in (Get-RelativeFiles -BasePath $Destination)) {
        if ($entry.relative -ieq "checksums.sha256") {
            continue
        }
        $hash = (Get-LoomFileDigest -Path $entry.file.FullName).sha256
        $lines.Add("$hash  $($entry.relative)")
    }
    Write-Ascii -Path $checksumPath -Value (($lines.ToArray() -join "`r`n") + "`r`n")
    return [ordered]@{
        path = "checksums.sha256"
        entries = $lines.Count
        sha256 = (Get-LoomFileDigest -Path $checksumPath).sha256
    }
}
