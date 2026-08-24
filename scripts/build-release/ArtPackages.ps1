<# Owns one release-script responsibility. #>

function Get-SampleArtPackageArtifacts {
    param([System.Collections.Specialized.OrderedDictionary]$ArtCatalog)

    $catalogRoot = [string]$ArtCatalog.outputRoot
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $catalogRoot
    $summaryPath = Join-Path $catalogRoot "summary.json"
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "Sample Art package catalog summary is missing: $summaryPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $summaryPath
    $summary = Read-LoomBoundedJsonFile -Path $summaryPath -MaxBytes 4MB
    $expectedEntries = @($ArtCatalog.expected)
    $summaryEntries = @($summary.packages)
    $expectedIds = @($expectedEntries | ForEach-Object { [string]$_.id })
    $actualIds = @($summaryEntries | ForEach-Object { [string]$_.id })
    if (-not [string]::Equals(
        (@($expectedIds | Sort-Object) -join "`n"),
        (@($actualIds | Sort-Object) -join "`n"),
        [System.StringComparison]::Ordinal
    )) {
        throw "Sample Art package catalog ids do not match the release contract."
    }

    $packageRecords = @()
    $artifactRecords = @()
    $payloadRecords = @()
    foreach ($expected in $expectedEntries) {
        $id = [string]$expected.id
        $framework = [string]$expected.framework
        $entry = @($summaryEntries | Where-Object { [string]$_.id -eq $id })
        if ($entry.Count -ne 1) {
            throw "Sample Art package catalog must contain exactly one entry for ${id}."
        }
        if (-not [string]::Equals([string]$entry[0].framework, $framework, [System.StringComparison]::Ordinal)) {
            throw "Sample Art framework mismatch in catalog: $id"
        }
        if (-not [string]::Equals([string]$entry[0].zip, "$id.zip", [System.StringComparison]::Ordinal)) {
            throw "Sample Art ZIP name mismatch in catalog: $id"
        }

        $zipPath = Join-Path $catalogRoot "$id.zip"
        $sidecarPath = "$zipPath.sha256"
        foreach ($required in @($zipPath, $sidecarPath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "Sample Art package artifact is missing: $required"
            }
            Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $required
        }
        $zipHash = (Get-LoomFileDigest -Path $zipPath).sha256
        $sidecarFields = @((Read-LoomBoundedTextFile -Path $sidecarPath -MaxBytes 4096).Trim() -split '\s+')
        if (
            $sidecarFields.Count -ne 2 -or
            -not [string]::Equals($sidecarFields[0], $zipHash, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not [string]::Equals($sidecarFields[1], "$id.zip", [System.StringComparison]::Ordinal)
        ) {
            throw "Sample Art package checksum sidecar is invalid: $sidecarPath"
        }
        if (-not [string]::Equals([string]$entry[0].sha256, $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Sample Art package summary hash mismatch: $id"
        }

        $zipFile = Get-Item -LiteralPath $zipPath
        $zipRelative = "packages\arts\$id.zip"
        $zipRecord = [ordered]@{
            kind = "sample-art-package-zip"
            role = "sample-art"
            id = $id
            framework = $framework
            name = $zipFile.Name
            path = $zipRelative
            bytes = [int64]$zipFile.Length
            sha256 = $zipHash
        }
        $sidecarFile = Get-Item -LiteralPath $sidecarPath
        $sidecarRecord = [ordered]@{
            kind = "sample-art-package-zip-sha256"
            role = "sample-art"
            id = $id
            name = $sidecarFile.Name
            path = "$zipRelative.sha256"
            bytes = [int64]$sidecarFile.Length
            sha256 = (Get-LoomFileDigest -Path $sidecarPath).sha256
        }
        $packageRecords += $zipRecord
        $artifactRecords += @($zipRecord, $sidecarRecord)
        $payloadRecords += @($zipRecord, $sidecarRecord)
    }

    $summaryFile = Get-Item -LiteralPath $summaryPath
    $summaryRecord = [ordered]@{
        kind = "sample-art-package-catalog"
        name = $summaryFile.Name
        path = "packages\arts\summary.json"
        bytes = [int64]$summaryFile.Length
        sha256 = (Get-LoomFileDigest -Path $summaryPath).sha256
    }
    $artifactRecords += $summaryRecord
    $payloadRecords += $summaryRecord
    return [pscustomobject]@{
        packages = @($packageRecords)
        catalog = $summaryRecord
        artifacts = @($artifactRecords)
        payload = @($payloadRecords)
    }
}
