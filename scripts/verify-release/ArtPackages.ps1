<# Owns one release-script responsibility. #>

function Assert-SampleArtPackages {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    if ([int]$Manifest.schemaVersion -lt 2) {
        return @()
    }

    $expected = [ordered]@{
        "custom-1770146354922" = [ordered]@{ framework = "process"; executionType = "framework_art" }
        "custom-remove-bg-cloud" = [ordered]@{ framework = "cloud_api"; executionType = "framework_art" }
        "custom-image-search" = [ordered]@{ framework = "mcp"; executionType = "framework_art" }
        "custom-1770131241684" = [ordered]@{ framework = "process"; executionType = "framework_art" }
        "custom-image-blend-script" = [ordered]@{ framework = "process"; executionType = "framework_art" }
        "custom-image-blend-compress-workflow" = [ordered]@{ framework = "workflow"; executionType = "workflow" }
        "custom-stock-monitor" = [ordered]@{ framework = "mcp"; executionType = "framework_art" }
    }
    $packageRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "sampleArtPackages")
    Assert-Equal -Expected $expected.Count -Actual $packageRecords.Count -Message "Manifest must contain seven curated Art package records."
    $actualIds = @($packageRecords | ForEach-Object { [string]$_.id } | Sort-Object)
    Assert-Equal -Expected ((@($expected.Keys) | Sort-Object) -join ",") -Actual ($actualIds -join ",") -Message "Sample Art package id set mismatch."
    $artPackageRoot = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath "packages\arts"
    $expectedZipNames = @($expected.Keys | ForEach-Object { "$_.zip" } | Sort-Object)
    $actualZipNames = @(Get-ChildItem -LiteralPath $artPackageRoot -Filter *.zip -File | ForEach-Object { $_.Name } | Sort-Object)
    Assert-Equal -Expected ($expectedZipNames -join ",") -Actual ($actualZipNames -join ",") -Message "Release sample Art ZIP set mismatch."

    $artifacts = @(Get-ManifestRecord -Manifest $Manifest -Name "artifacts")
    $payloadPaths = @()
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    foreach ($id in $expected.Keys) {
        $framework = [string]$expected[$id].framework
        $executionType = [string]$expected[$id].executionType
        $zipRecord = @($packageRecords | Where-Object { [string]$_.id -eq $id })
        Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Sample Art package record count mismatch for $id."
        Assert-Equal -Expected "sample-art-package-zip" -Actual ([string]$zipRecord[0].kind) -Message "Sample Art package kind mismatch for $id."
        Assert-Equal -Expected $framework -Actual ([string]$zipRecord[0].framework) -Message "Sample Art framework mismatch for $id."
        Assert-Equal -Expected "$id.zip" -Actual ([string]$zipRecord[0].name) -Message "Sample Art package name mismatch for $id."
        Assert-Equal -Expected "packages\arts\$id.zip" -Actual (([string]$zipRecord[0].path).Replace("/", "\")) -Message "Sample Art package path mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $zipRecord[0]

        $sidecarRecord = @($artifacts | Where-Object {
            (Test-LoomArtifactKind -Artifact $_ -Kind "sample-art-package-zip-sha256") -and
            [string]$_.id -eq $id
        })
        Assert-Equal -Expected 1 -Actual $sidecarRecord.Count -Message "Sample Art package checksum record count mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $sidecarRecord[0]
        Assert-ZipChecksumSidecar -PackagePath $PackagePath -ZipRecord $zipRecord[0] -SidecarRecord $sidecarRecord[0]

        $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $manifestEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "manifest.json" })
            $runtimeManifestEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "art.runtime.json" })
            $workflowEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "workflow.yaml" })
            Assert-Equal -Expected 1 -Actual $manifestEntry.Count -Message "Sample Art ZIP must contain one root manifest: $id"
            if ($executionType -eq "framework_art") {
                Assert-Equal -Expected 1 -Actual $runtimeManifestEntry.Count -Message "Sample Art ZIP must contain one runtime manifest: $id"
            }
            else {
                Assert-Equal -Expected 0 -Actual $runtimeManifestEntry.Count -Message "Workflow sample Art ZIP must not contain a local runtime manifest: $id"
                Assert-Equal -Expected 1 -Actual $workflowEntry.Count -Message "Workflow sample Art ZIP must contain one workflow definition: $id"
            }
            $artManifest = Read-LoomArchiveEntryJson -Entry $manifestEntry[0] -MaxBytes 1MB
            Assert-Equal -Expected $id -Actual ([string]$artManifest.id) -Message "Sample Art ZIP manifest id mismatch for $id."
            Assert-True -Condition (-not [string]::IsNullOrWhiteSpace([string]$artManifest.name)) -Message "Sample Art ZIP manifest name is empty for $id."
            Assert-Equal -Expected $executionType -Actual ([string]$artManifest.execution.type) -Message "Sample Art ZIP execution type mismatch for $id."
            if ($executionType -eq "framework_art") {
                Assert-Equal -Expected $framework -Actual ([string]$artManifest.execution.framework) -Message "Sample Art ZIP execution framework mismatch for $id."
            }
            else {
                Assert-Equal -Expected "image-blend-compress-workflow" -Actual ([string]$artManifest.execution.workflowId) -Message "Workflow sample Art execution id mismatch for $id."
            }
            Assert-Equal -Expected $framework -Actual ([string]$artManifest.metadata.dependencies.framework) -Message "Sample Art ZIP dependency framework mismatch for $id."
            if ($id -eq "custom-stock-monitor") {
                $entryNames = @($archive.Entries | ForEach-Object { $_.FullName.Replace("\", "/") })
                $expectedRuntimeEntries = @(
                    "runtime/common.ps1",
                    "runtime/lib/Constants.ps1",
                    "runtime/lib/Domain.ps1",
                    "runtime/lib/Mcp.ps1",
                    "runtime/lib/Output.ps1",
                    "runtime/lib/Protocol.ps1",
                    "runtime/lib/Snapshot.ps1",
                    "runtime/lib/Transforms.ps1",
                    "runtime/main.ps1"
                )
                $actualRuntimeEntries = @($entryNames | Where-Object { $_ -match '^runtime/(?:lib/)?[^/]+\.ps1$' } | Sort-Object)
                Assert-Equal -Expected ($expectedRuntimeEntries -join ",") -Actual ($actualRuntimeEntries -join ",") -Message "Stock Monitor ZIP runtime module set mismatch."
                Assert-True -Condition ($entryNames -contains "surface/main.js") -Message "Stock Monitor ZIP is missing the JavaScript Surface entry."
                Assert-True -Condition ($entryNames -contains "surface/fallback.json") -Message "Stock Monitor ZIP is missing the declarative fallback."
                Assert-Equal -Expected "stock-api" -Actual ([string]$artManifest.metadata.marketData.providerId) -Message "Stock Monitor provider metadata mismatch."
                Assert-Equal -Expected "neuro.official/stock-api" -Actual ([string]$artManifest.metadata.mcp.packageId) -Message "Stock Monitor MCP package metadata mismatch."
                Assert-Equal -Expected 4 -Actual @($artManifest.metadata.mcp.calls).Count -Message "Stock Monitor MCP call count mismatch."
                $orderBookCalls = @($artManifest.metadata.mcp.calls | Where-Object { [string]$_.toolName -eq "get_order_book" })
                Assert-Equal -Expected 1 -Actual $orderBookCalls.Count -Message "Stock Monitor must declare one order-book MCP call."
                Assert-Equal -Expected "auto" -Actual ([string]$orderBookCalls[0].arguments.source) -Message "Stock Monitor order-book source mismatch."
                $favoritesCalls = @($artManifest.metadata.mcp.calls | Where-Object { [string]$_.toolName -eq "get_stocks" })
                Assert-Equal -Expected 1 -Actual $favoritesCalls.Count -Message "Stock Monitor must declare one aggregate favorites MCP call."
                Assert-Equal -Expected 4 -Actual @($favoritesCalls[0].arguments.codes).Count -Message "Stock Monitor favorites list must remain bounded."
                Assert-Equal -Expected "full" -Actual ([string]$artManifest.metadata.capabilities.surface.defaultViewId) -Message "Stock Monitor default view mismatch."
                Assert-Equal -Expected 4 -Actual @($artManifest.metadata.capabilities.surface.views).Count -Message "Stock Monitor view count mismatch."
                Assert-Equal -Expected $false -Actual ([bool]$artManifest.metadata.marketData.trading) -Message "Stock Monitor must not advertise trading."
            }
        }
        finally {
            $archive.Dispose()
        }
    }

    $catalogRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "sampleArtCatalog")
    Assert-Equal -Expected 1 -Actual $catalogRecords.Count -Message "Manifest must contain one sample Art catalog record."
    Assert-Equal -Expected "packages\arts\summary.json" -Actual (([string]$catalogRecords[0].path).Replace("/", "\")) -Message "Sample Art catalog path mismatch."
    $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $catalogRecords[0]
    $catalogArtifact = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "sample-art-package-catalog" })
    Assert-Equal -Expected 1 -Actual $catalogArtifact.Count -Message "Manifest artifacts must contain one sample Art catalog."
    Assert-Equal -Expected ([string]$catalogRecords[0].sha256) -Actual ([string]$catalogArtifact[0].sha256) -Message "Sample Art catalog artifact mismatch."

    $catalogPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$catalogRecords[0].path)
    $catalog = Read-LoomVerifiedJsonFile -Path $catalogPath -MaxBytes 4MB
    Assert-Equal -Expected "Release" -Actual ([string]$catalog.configuration) -Message "Sample Art catalog configuration mismatch."
    $catalogEntries = @($catalog.packages)
    Assert-Equal -Expected $expected.Count -Actual $catalogEntries.Count -Message "Sample Art catalog entry count mismatch."
    foreach ($record in $packageRecords) {
        $entry = @($catalogEntries | Where-Object { [string]$_.id -eq [string]$record.id })
        Assert-Equal -Expected 1 -Actual $entry.Count -Message "Sample Art catalog entry is missing for $($record.id)."
        Assert-Equal -Expected ([string]$record.framework) -Actual ([string]$entry[0].framework) -Message "Sample Art catalog framework mismatch for $($record.id)."
        Assert-Equal -Expected ([string]$record.name) -Actual ([string]$entry[0].zip) -Message "Sample Art catalog ZIP mismatch for $($record.id)."
        Assert-Equal -Expected ([string]$record.sha256) -Actual ([string]$entry[0].sha256) -Message "Sample Art catalog hash mismatch for $($record.id)."
    }

    return @($payloadPaths)
}
