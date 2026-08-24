<# Owns one release-script responsibility. #>

function Assert-SupplyChainMetadata {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    $sbomRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "sbom")
    Assert-Equal -Expected 2 -Actual $sbomRecords.Count -Message "Manifest must contain CycloneDX and SPDX SBOM records."
    foreach ($record in $sbomRecords) {
        $relative = Assert-FileRecord -PackagePath $PackagePath -Record $record
        $path = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relative
        $document = Read-LoomVerifiedJsonFile -Path $path -MaxBytes 8MB
        if ($relative.EndsWith(".cdx.json", [System.StringComparison]::OrdinalIgnoreCase)) {
            Assert-Equal -Expected "CycloneDX" -Actual ([string]$document.bomFormat) -Message "CycloneDX SBOM format mismatch."
            Assert-Equal -Expected "1.6" -Actual ([string]$document.specVersion) -Message "CycloneDX SBOM version mismatch."
            $componentIdentities = @($document.components | ForEach-Object { "$([string]$_.name)@$([string]$_.version)" })
            Assert-True -Condition ($componentIdentities -contains "stock-api@2.7.3") -Message "CycloneDX SBOM is missing stock-api@2.7.3."
            Assert-True -Condition ($componentIdentities -contains "pysnowball@0.1.8") -Message "CycloneDX SBOM is missing pysnowball@0.1.8."
            $pysnowballComponent = @($document.components | Where-Object { [string]$_.name -eq "pysnowball" })[0]
            Assert-Equal -Expected "Apache-2.0" -Actual ([string]$pysnowballComponent.licenses[0].license.id) -Message "CycloneDX pysnowball license mismatch."
            Assert-True -Condition ($componentIdentities -contains "nodejs@22.22.2") -Message "CycloneDX SBOM is missing the bundled Node.js runtime."
        }
        elseif ($relative.EndsWith(".spdx.json", [System.StringComparison]::OrdinalIgnoreCase)) {
            Assert-Equal -Expected "SPDX-2.3" -Actual ([string]$document.spdxVersion) -Message "SPDX SBOM version mismatch."
            $packageIdentities = @($document.packages | ForEach-Object { "$([string]$_.name)@$([string]$_.versionInfo)" })
            Assert-True -Condition ($packageIdentities -contains "stock-api@2.7.3") -Message "SPDX SBOM is missing stock-api@2.7.3."
            Assert-True -Condition ($packageIdentities -contains "pysnowball@0.1.8") -Message "SPDX SBOM is missing pysnowball@0.1.8."
            $pysnowballPackage = @($document.packages | Where-Object { [string]$_.name -eq "pysnowball" })[0]
            Assert-Equal -Expected "Apache-2.0" -Actual ([string]$pysnowballPackage.licenseDeclared) -Message "SPDX pysnowball declared license mismatch."
            Assert-True -Condition ($packageIdentities -contains "nodejs@22.22.2") -Message "SPDX SBOM is missing the bundled Node.js runtime."
        }
        else {
            throw "Unknown SBOM format: $relative"
        }
    }
    $provenanceRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "provenance")
    Assert-Equal -Expected 1 -Actual $provenanceRecords.Count -Message "Manifest must contain one provenance record."
    $provenanceRelative = Assert-FileRecord -PackagePath $PackagePath -Record $provenanceRecords[0]
    $provenancePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $provenanceRelative
    $provenance = Read-LoomVerifiedJsonFile -Path $provenancePath -MaxBytes 8MB
    Assert-Equal -Expected ([string]$Manifest.gitHead) -Actual ([string]$provenance.gitHead) -Message "Provenance Git head mismatch."
    Assert-Equal -Expected ([bool]$Manifest.gitDirty) -Actual ([bool]$provenance.gitDirty) -Message "Provenance dirty flag mismatch."
}

function Assert-ZipChecksumSidecar {
    param(
        [string]$PackagePath,
        [object]$ZipRecord,
        [object]$SidecarRecord
    )

    $zipName = [string]$ZipRecord.name
    $zipRelativePath = ([string]$ZipRecord.path).Replace("/", "\")
    $sidecarRelativePath = ([string]$SidecarRecord.path).Replace("/", "\")
    Assert-Equal -Expected "$zipName.sha256" -Actual ([string]$SidecarRecord.name) -Message "ZIP checksum sidecar metadata mismatch for $zipName."
    Assert-Equal -Expected "$zipRelativePath.sha256" -Actual $sidecarRelativePath -Message "ZIP checksum sidecar metadata mismatch for $zipName."

    $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $zipRelativePath
    $sidecarPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $sidecarRelativePath
    $actualZipHash = (Get-LoomVerifiedFileDigest -Path $zipPath).sha256
    $expectedLine = "$actualZipHash  $zipName"
    $expectedContentCrLf = $expectedLine + "`r`n"
    $expectedContentLf = $expectedLine + "`n"
    $actualContent = Read-LoomVerifiedTextFile -Path $sidecarPath -MaxBytes 4096 -Encoding ([System.Text.ASCIIEncoding]::new())
    $contentMatches = (
        [string]::Equals($actualContent, $expectedLine, [System.StringComparison]::Ordinal) -or
        [string]::Equals($actualContent, $expectedContentCrLf, [System.StringComparison]::Ordinal) -or
        [string]::Equals($actualContent, $expectedContentLf, [System.StringComparison]::Ordinal)
    )
    if (-not $contentMatches) {
        throw "ZIP checksum sidecar content mismatch for $zipName."
    }
    Assert-Equal -Expected ([string]$ZipRecord.sha256) -Actual $actualZipHash -Message "ZIP artifact SHA-256 mismatch for $zipName."
}
