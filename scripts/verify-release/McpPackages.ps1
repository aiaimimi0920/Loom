<# Owns one release-script responsibility. #>

function Assert-McpServerPackages {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    if ([int]$Manifest.schemaVersion -lt 2) {
        return @()
    }

    $expected = [ordered]@{
        "neuro-image-search" = [ordered]@{
            qualifiedId = "neuro.official/neuro-image-search"
            version = "0.1.0"
            tools = @("brave_image_search")
        }
        "stock-api" = [ordered]@{
            qualifiedId = "neuro.official/stock-api"
            version = "2.9.0"
            tools = @("get_stock", "get_stocks", "get_klines", "get_market_series", "get_order_book", "search_stocks", "inspect_stock")
        }
    }
    $packageRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "mcpServerPackages")
    Assert-Equal -Expected $expected.Count -Actual $packageRecords.Count -Message "Manifest must contain two MCP server package records."
    $actualIds = @($packageRecords | ForEach-Object { [string]$_.id } | Sort-Object)
    Assert-Equal -Expected (@($expected.Keys | Sort-Object) -join ",") -Actual ($actualIds -join ",") -Message "MCP server package id set mismatch."
    $artifacts = @(Get-ManifestRecord -Manifest $Manifest -Name "artifacts")
    $payloadPaths = @()
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    foreach ($id in $expected.Keys) {
        $zipRecords = @($packageRecords | Where-Object { [string]$_.id -eq $id })
        Assert-Equal -Expected 1 -Actual $zipRecords.Count -Message "MCP server package record count mismatch for $id."
        $zipRecord = $zipRecords[0]
        Assert-Equal -Expected "mcp-server-package-zip" -Actual ([string]$zipRecord.kind) -Message "MCP server package kind mismatch for $id."
        Assert-Equal -Expected ([string]$expected[$id].qualifiedId) -Actual ([string]$zipRecord.qualifiedId) -Message "MCP server package qualified id mismatch for $id."
        Assert-Equal -Expected ([string]$expected[$id].version) -Actual ([string]$zipRecord.version) -Message "MCP server package version mismatch for $id."
        Assert-Equal -Expected "$id.zip" -Actual ([string]$zipRecord.name) -Message "MCP server package name mismatch for $id."
        Assert-Equal -Expected "packages\mcp-servers\$id.zip" -Actual (([string]$zipRecord.path).Replace("/", "\")) -Message "MCP server package path mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $zipRecord
        $sidecarRecord = @($artifacts | Where-Object {
            (Test-LoomArtifactKind -Artifact $_ -Kind "mcp-server-package-zip-sha256") -and
            [string]$_.id -eq $id
        })
        Assert-Equal -Expected 1 -Actual $sidecarRecord.Count -Message "MCP server package checksum record count mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $sidecarRecord[0]
        Assert-ZipChecksumSidecar -PackagePath $PackagePath -ZipRecord $zipRecord -SidecarRecord $sidecarRecord[0]

        $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord.path)
        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $entryNames = @($archive.Entries | ForEach-Object { $_.FullName.Replace("\", "/") })
            $manifestEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "mcp.server.json" })
            Assert-Equal -Expected 1 -Actual $manifestEntry.Count -Message "MCP server ZIP must contain one root manifest for $id."
            $serverManifest = Read-LoomArchiveEntryJson -Entry $manifestEntry[0] -MaxBytes 1MB
            Assert-Equal -Expected $id -Actual ([string]$serverManifest.id) -Message "MCP server ZIP manifest id mismatch for $id."
            Assert-Equal -Expected ([string]$zipRecord.version) -Actual ([string]$serverManifest.version) -Message "MCP server ZIP manifest version mismatch for $id."
            Assert-Equal -Expected "neuro.official" -Actual ([string]$serverManifest.publisher.id) -Message "MCP server ZIP publisher mismatch for $id."
            $actualTools = @($serverManifest.tools | ForEach-Object { [string]$_ } | Sort-Object)
            $expectedTools = @($expected[$id].tools | Sort-Object)
            Assert-Equal -Expected ($expectedTools -join ",") -Actual ($actualTools -join ",") -Message "MCP server ZIP tool declaration mismatch for $id."
            $runtimePath = ([string]$serverManifest.entry.command).Replace("\", "/")
            Assert-True -Condition ($entryNames -contains $runtimePath) -Message "MCP server ZIP runtime entry is missing for $id."
            if ($id -eq "stock-api") {
                foreach ($requiredPath in @(
                    "runtime/stock-api-entry.js",
                    "runtime/stock-api/constants.js",
                    "runtime/stock-api/executors.js",
                    "runtime/stock-api/helpers.js",
                    "runtime/stock-api/parsers.js",
                    "runtime/stock-api/providers.js",
                    "runtime/stock-api/server.js",
                    "runtime/stock-api/transport.js",
                    "runtime/node/node.exe",
                    "runtime/node/LICENSE",
                    "runtime/node-runtime.json",
                    "runtime/UPSTREAM.json",
                    "runtime/PYSNOWBALL.json",
                    "runtime/vendor/stock-api/package.json",
                    "runtime/vendor/stock-api/LICENSE",
                    "runtime/vendor/stock-api/dist/mcp/server.js",
                    "runtime/vendor/pysnowball/LICENSE",
                    "runtime/vendor/pysnowball/NOTICE.md"
                )) {
                    Assert-True -Condition ($entryNames -contains $requiredPath) -Message "stock-api MCP ZIP is missing $requiredPath."
                }
                Assert-True -Condition ((Get-Item -LiteralPath $zipPath).Length -le 64MB) -Message "stock-api MCP ZIP exceeds the installer package limit."
                $nodeEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "runtime/node/node.exe" })[0]
                $nodeStream = $nodeEntry.Open()
                $algorithm = [System.Security.Cryptography.SHA256]::Create()
                try {
                    $nodeHash = ([System.BitConverter]::ToString($algorithm.ComputeHash($nodeStream))).Replace('-', '').ToLowerInvariant()
                }
                finally {
                    $algorithm.Dispose()
                    $nodeStream.Dispose()
                }
                Assert-Equal -Expected "ae1a50511be58e987483fdbc12125407443926d2d394669ade2352776e920dd3" -Actual $nodeHash -Message "Bundled stock-api Node.js hash mismatch."
            }
        }
        finally {
            $archive.Dispose()
        }
    }

    $catalogRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "mcpServerCatalog")
    Assert-Equal -Expected 1 -Actual $catalogRecords.Count -Message "Manifest must contain one MCP server catalog record."
    Assert-Equal -Expected "packages\mcp-servers\summary.json" -Actual (([string]$catalogRecords[0].path).Replace("/", "\")) -Message "MCP server catalog path mismatch."
    $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $catalogRecords[0]
    $catalogArtifact = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "mcp-server-package-catalog" })
    Assert-Equal -Expected 1 -Actual $catalogArtifact.Count -Message "Manifest artifacts must contain one MCP server catalog."
    Assert-Equal -Expected ([string]$catalogRecords[0].sha256) -Actual ([string]$catalogArtifact[0].sha256) -Message "MCP server catalog artifact mismatch."

    $catalogPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$catalogRecords[0].path)
    $catalog = Read-LoomVerifiedJsonFile -Path $catalogPath -MaxBytes 4MB
    $catalogEntries = @($catalog.servers)
    Assert-Equal -Expected $expected.Count -Actual $catalogEntries.Count -Message "MCP server catalog entry count mismatch."
    foreach ($record in $packageRecords) {
        $entry = @($catalogEntries | Where-Object { [string]$_.id -eq [string]$record.id })
        Assert-Equal -Expected 1 -Actual $entry.Count -Message "MCP server catalog entry is missing for $($record.id)."
        Assert-Equal -Expected ([string]$record.qualifiedId) -Actual ([string]$entry[0].qualifiedId) -Message "MCP server catalog qualified id mismatch for $($record.id)."
        Assert-Equal -Expected ([string]$record.name) -Actual ([string]$entry[0].zip) -Message "MCP server catalog ZIP mismatch for $($record.id)."
        Assert-Equal -Expected ([string]$record.sha256) -Actual ([string]$entry[0].sha256) -Message "MCP server catalog hash mismatch for $($record.id)."
    }

    return @($payloadPaths)
}
