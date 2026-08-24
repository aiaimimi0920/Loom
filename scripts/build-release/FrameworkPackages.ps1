<# Owns one release-script responsibility. #>

function Get-FrameworkPackageArtifacts {
    param([System.Collections.Specialized.OrderedDictionary]$FrameworkCatalog)

    $catalogRoot = [string]$FrameworkCatalog.outputRoot
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $catalogRoot
    $summaryPath = Join-Path $catalogRoot "summary.json"
    if (-not (Test-Path -LiteralPath $summaryPath -PathType Leaf)) {
        throw "Framework package catalog summary is missing: $summaryPath"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $catalogRoot -Path $summaryPath
    $summary = Read-LoomBoundedJsonFile -Path $summaryPath -MaxBytes 4MB
    $expectedIds = @($FrameworkCatalog.expectedIds | ForEach-Object { [string]$_ })
    $summaryEntries = @($summary.frameworks)
    $actualIds = @($summaryEntries | ForEach-Object { [string]$_.id })
    $actualIdSet = (@($actualIds | Sort-Object) -join "`n")
    $expectedIdSet = (@($expectedIds | Sort-Object) -join "`n")
    if (-not [string]::Equals($actualIdSet, $expectedIdSet, [System.StringComparison]::Ordinal)) {
        throw "Framework package catalog ids do not match the release contract."
    }

    $packageRecords = @()
    $artifactRecords = @()
    $payloadRecords = @()
    foreach ($id in $expectedIds) {
        $entry = @($summaryEntries | Where-Object { [string]$_.id -eq $id })
        if ($entry.Count -ne 1) {
            throw "Framework package catalog must contain exactly one entry for ${id}."
        }
        $zipPath = Join-Path $catalogRoot "$id.zip"
        $sidecarPath = "$zipPath.sha256"
        foreach ($required in @($zipPath, $sidecarPath)) {
            if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                throw "Framework package artifact is missing: $required"
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
            throw "Framework package checksum sidecar is invalid: $sidecarPath"
        }
        if (-not [string]::Equals([string]$entry[0].sha256, $zipHash, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Framework package summary hash mismatch: $id"
        }

        $zipFile = Get-Item -LiteralPath $zipPath
        $zipRelative = "packages\frameworks\$id.zip"
        $zipRecord = [ordered]@{
            kind = "framework-package-zip"
            role = "framework"
            id = $id
            version = [string]$entry[0].version
            protocolVersion = [string]$entry[0].protocolVersion
            name = $zipFile.Name
            path = $zipRelative
            bytes = [int64]$zipFile.Length
            sha256 = $zipHash
        }
        $sidecarFile = Get-Item -LiteralPath $sidecarPath
        $sidecarRelative = "$zipRelative.sha256"
        $sidecarRecord = [ordered]@{
            kind = "framework-package-zip-sha256"
            role = "framework"
            id = $id
            name = $sidecarFile.Name
            path = $sidecarRelative
            bytes = [int64]$sidecarFile.Length
            sha256 = (Get-LoomFileDigest -Path $sidecarPath).sha256
        }
        $packageRecords += $zipRecord
        $artifactRecords += @($zipRecord, $sidecarRecord)
        $payloadRecords += @($zipRecord, $sidecarRecord)
    }

    $summaryFile = Get-Item -LiteralPath $summaryPath
    $summaryRecord = [ordered]@{
        kind = "framework-package-catalog"
        name = $summaryFile.Name
        path = "packages\frameworks\summary.json"
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
