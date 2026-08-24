<# Owns one release-script responsibility. #>

function Get-McpServerPackageArtifacts {
    param([System.Collections.Specialized.OrderedDictionary]$McpCatalog)

    $catalogRoot = [string]$McpCatalog.outputRoot
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $catalogRoot
    $summaryPath = Join-Path $catalogRoot "summary.json"
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "MCP server package catalog summary is missing: $summaryPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $summaryPath
    $summary = Read-LoomBoundedJsonFile -Path $summaryPath -MaxBytes 4MB
    $expectedIds = @($McpCatalog.expectedIds | ForEach-Object { [string]$_ })
    $summaryEntries = @($summary.servers)
    $actualIds = @($summaryEntries | ForEach-Object { [string]$_.id })
    if (-not [string]::Equals(
        (@($expectedIds | Sort-Object) -join "`n"),
        (@($actualIds | Sort-Object) -join "`n"),
        [System.StringComparison]::Ordinal
    )) {
        throw "MCP server package catalog ids do not match the release contract."
    }

    $packageRecords = @()
    $artifactRecords = @()
    foreach ($id in $expectedIds) {
        $entry = @($summaryEntries | Where-Object { [string]$_.id -eq $id })
        if ($entry.Count -ne 1) {
            throw "MCP server package catalog must contain exactly one entry for ${id}."
        }
        $zipPath = Join-Path $catalogRoot "$id.zip"
        $sidecarPath = "$zipPath.sha256"
        foreach ($required in @($zipPath, $sidecarPath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "MCP server package artifact is missing: $required"
            }
            Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $required
        }
        $zipHash = (Get-LoomFileDigest -Path $zipPath).sha256
        $sidecarFields = @((Read-LoomBoundedTextFile -Path $sidecarPath -MaxBytes 4096).Trim() -split '\s+')
        if ($sidecarFields.Count -ne 2 -or
            -not [string]::Equals($sidecarFields[0], $zipHash, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not [string]::Equals($sidecarFields[1], "$id.zip", [System.StringComparison]::Ordinal)) {
            throw "MCP server package checksum sidecar is invalid: $sidecarPath"
        }
        if (-not [string]::Equals([string]$entry[0].sha256, $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "MCP server package summary hash mismatch: $id"
        }
        $zipFile = Get-Item -LiteralPath $zipPath
        $zipRelative = "packages\mcp-servers\$id.zip"
        $zipRecord = [ordered]@{
            kind = "mcp-server-package-zip"
            role = "mcp-server"
            id = $id
            qualifiedId = [string]$entry[0].qualifiedId
            version = [string]$entry[0].version
            name = $zipFile.Name
            path = $zipRelative
            bytes = [int64]$zipFile.Length
            sha256 = $zipHash
        }
        $sidecarFile = Get-Item -LiteralPath $sidecarPath
        $sidecarRecord = [ordered]@{
            kind = "mcp-server-package-zip-sha256"
            role = "mcp-server"
            id = $id
            name = $sidecarFile.Name
            path = "$zipRelative.sha256"
            bytes = [int64]$sidecarFile.Length
            sha256 = (Get-LoomFileDigest -Path $sidecarPath).sha256
        }
        $packageRecords += $zipRecord
        $artifactRecords += @($zipRecord, $sidecarRecord)
    }
    $summaryFile = Get-Item -LiteralPath $summaryPath
    $summaryRecord = [ordered]@{
        kind = "mcp-server-package-catalog"
        name = $summaryFile.Name
        path = "packages\mcp-servers\summary.json"
        bytes = [int64]$summaryFile.Length
        sha256 = (Get-LoomFileDigest -Path $summaryPath).sha256
    }
    $artifactRecords += $summaryRecord
    return [pscustomobject]@{
        packages = @($packageRecords)
        catalog = $summaryRecord
        artifacts = @($artifactRecords)
        payload = @($artifactRecords)
    }
}
