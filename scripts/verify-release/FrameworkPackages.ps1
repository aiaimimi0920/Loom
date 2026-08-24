<# Owns one release-script responsibility. #>

function Assert-FrameworkPackages {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    if ([int]$Manifest.schemaVersion -lt 2) {
        return @()
    }

    $expectedIds = @("process", "cloud_api", "mcp", "workflow")
    $packageRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "frameworkPackages")
    Assert-Equal -Expected $expectedIds.Count -Actual $packageRecords.Count -Message "Manifest must contain four framework package records."
    $actualIds = @($packageRecords | ForEach-Object { [string]$_.id } | Sort-Object)
    Assert-Equal -Expected (($expectedIds | Sort-Object) -join ",") -Actual ($actualIds -join ",") -Message "Framework package id set mismatch."

    $artifacts = @(Get-ManifestRecord -Manifest $Manifest -Name "artifacts")
    $payloadPaths = @()
    foreach ($id in $expectedIds) {
        $zipRecord = @($packageRecords | Where-Object { [string]$_.id -eq $id })
        Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Framework package record count mismatch for $id."
        Assert-Equal -Expected "framework-package-zip" -Actual ([string]$zipRecord[0].kind) -Message "Framework package kind mismatch for $id."
        Assert-Equal -Expected "loom.framework.v1" -Actual ([string]$zipRecord[0].protocolVersion) -Message "Framework package protocol mismatch for $id."
        Assert-Equal -Expected "$id.zip" -Actual ([string]$zipRecord[0].name) -Message "Framework package name mismatch for $id."
        Assert-Equal -Expected "packages\frameworks\$id.zip" -Actual (([string]$zipRecord[0].path).Replace("/", "\")) -Message "Framework package path mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $zipRecord[0]

        $sidecarRecord = @($artifacts | Where-Object {
            (Test-LoomArtifactKind -Artifact $_ -Kind "framework-package-zip-sha256") -and
            [string]$_.id -eq $id
        })
        Assert-Equal -Expected 1 -Actual $sidecarRecord.Count -Message "Framework package checksum record count mismatch for $id."
        $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $sidecarRecord[0]
        Assert-ZipChecksumSidecar -PackagePath $PackagePath -ZipRecord $zipRecord[0] -SidecarRecord $sidecarRecord[0]

        $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $manifestEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "framework.manifest.json" })
            Assert-Equal -Expected 1 -Actual $manifestEntry.Count -Message "Framework ZIP must contain one root manifest: $id"
            $frameworkManifest = Read-LoomArchiveEntryJson -Entry $manifestEntry[0] -MaxBytes 1MB
            Assert-Equal -Expected $id -Actual ([string]$frameworkManifest.id) -Message "Framework ZIP manifest id mismatch for $id."
            Assert-Equal -Expected "loom.framework.v1" -Actual ([string]$frameworkManifest.protocolVersion) -Message "Framework ZIP protocol mismatch for $id."
            $command = ([string]$frameworkManifest.entry.command).Replace("\", "/")
            $runtimeEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq $command })
            Assert-Equal -Expected 1 -Actual $runtimeEntry.Count -Message "Framework ZIP runtime entry is missing for $id."
        }
        finally {
            $archive.Dispose()
        }
    }

    $catalogRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "frameworkCatalog")
    Assert-Equal -Expected 1 -Actual $catalogRecords.Count -Message "Manifest must contain one framework catalog record."
    Assert-Equal -Expected "packages\frameworks\summary.json" -Actual (([string]$catalogRecords[0].path).Replace("/", "\")) -Message "Framework catalog path mismatch."
    $payloadPaths += Assert-FileRecord -PackagePath $PackagePath -Record $catalogRecords[0]
    $catalogArtifact = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "framework-package-catalog" })
    Assert-Equal -Expected 1 -Actual $catalogArtifact.Count -Message "Manifest artifacts must contain one framework catalog."
    Assert-Equal -Expected ([string]$catalogRecords[0].sha256) -Actual ([string]$catalogArtifact[0].sha256) -Message "Framework catalog artifact mismatch."

    $catalogPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$catalogRecords[0].path)
    $catalog = Read-LoomVerifiedJsonFile -Path $catalogPath -MaxBytes 4MB
    Assert-Equal -Expected "Release" -Actual ([string]$catalog.configuration) -Message "Framework catalog configuration mismatch."
    $catalogEntries = @($catalog.frameworks)
    Assert-Equal -Expected $expectedIds.Count -Actual $catalogEntries.Count -Message "Framework catalog entry count mismatch."
    foreach ($record in $packageRecords) {
        $entry = @($catalogEntries | Where-Object { [string]$_.id -eq [string]$record.id })
        Assert-Equal -Expected 1 -Actual $entry.Count -Message "Framework catalog entry is missing for $($record.id)."
        Assert-Equal -Expected ([string]$record.name) -Actual ([string]$entry[0].zip) -Message "Framework catalog ZIP mismatch for $($record.id)."
        Assert-Equal -Expected ([string]$record.sha256) -Actual ([string]$entry[0].sha256) -Message "Framework catalog hash mismatch for $($record.id)."
    }

    return @($payloadPaths)
}
