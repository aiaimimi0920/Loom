<# Owns one release-script responsibility. #>

function Assert-CliZipPayload {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    $artifacts = Get-ManifestRecord -Manifest $Manifest -Name "artifacts"
    $zipRecord = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "cli-zip" })
    Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Manifest must contain exactly one CLI ZIP."
    Assert-True -Condition ([string]$zipRecord[0].name).StartsWith("Loom-CLI-", [System.StringComparison]::Ordinal) -Message "CLI ZIP must use the Loom-CLI- naming contract."
    $shaRecord = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "cli-zip-sha256" })
    Assert-Equal -Expected 1 -Actual $shaRecord.Count -Message "Manifest must contain exactly one CLI ZIP checksum sidecar."
    Assert-Equal -Expected "$($zipRecord[0].name).sha256" -Actual ([string]$shaRecord[0].name) -Message "CLI ZIP checksum sidecar name mismatch."
    $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
    Assert-True -Condition (Test-Path -LiteralPath $zipPath -PathType Leaf) -Message "CLI ZIP is missing."

    $actualEntries = @(Get-LoomArchiveFileEntries -ZipPath $zipPath | ForEach-Object { $_.Replace("\", "/") } | Sort-Object)
    Assert-Equal -Expected "loom.exe" -Actual ($actualEntries -join "`n") -Message "Loom CLI ZIP must contain exactly one loom.exe entry."

    $cliProperty = $Manifest.PSObject.Properties["cliArtifact"]
    Assert-True -Condition ($null -ne $cliProperty -and $null -ne $cliProperty.Value) -Message "Manifest is missing cliArtifact."
    Assert-Equal -Expected "loom.exe" -Actual ([string]$cliProperty.Value.entryName) -Message "CLI entry name mismatch."
    Assert-Equal -Expected ([string]$zipRecord[0].name) -Actual ([string]$cliProperty.Value.zipName) -Message "CLI artifact ZIP name mismatch."
    $cliRecordPath = ([string]$zipRecord[0].path).Replace("/", "\")
    $cliArtifactPath = ([string]$cliProperty.Value.path).Replace("/", "\")
    Assert-Equal -Expected $cliRecordPath -Actual $cliArtifactPath -Message "CLI artifact ZIP path mismatch."
    Assert-Equal -Expected ([int64]$zipRecord[0].bytes) -Actual ([int64]$cliProperty.Value.bytes) -Message "CLI artifact ZIP byte count mismatch."
    Assert-Equal -Expected ([string]$zipRecord[0].sha256) -Actual ([string]$cliProperty.Value.sha256) -Message "CLI artifact ZIP SHA-256 mismatch."
}

function Assert-PluginSdkZipPayload {
    param(
        [string]$PackagePath,
        [object]$Manifest
    )

    $artifacts = Get-ManifestRecord -Manifest $Manifest -Name "artifacts"
    $zipRecord = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "plugin-sdk-zip" })
    Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Manifest must contain exactly one plugin SDK ZIP."
    $shaRecord = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "plugin-sdk-zip-sha256" })
    Assert-Equal -Expected 1 -Actual $shaRecord.Count -Message "Manifest must contain exactly one plugin SDK ZIP checksum sidecar."
    $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
    $expectedEntries = @(
        "docs/plugin-development.md",
        "docs/plugin-migration.md",
        "docs/plugin-permissions.md",
        "docs/plugin-security.md",
        "docs/plugin-signing-and-trust.md",
        "docs/release-provenance.md",
        "loom-plugin.exe",
        "protocol/README.md",
        "protocol/schemas/art-runtime.v1.schema.json",
        "protocol/schemas/framework-authoring.v1.schema.json",
        "protocol/schemas/framework-execute-request.v1.schema.json",
        "protocol/schemas/framework-execute-response.v1.schema.json",
        "protocol/schemas/framework-manifest.v1.schema.json",
        "protocol/schemas/device-session.v1.schema.json",
        "protocol/schemas/hook-message.v1.schema.json",
        "protocol/schemas/surface-manifest.v1.schema.json",
        "protocol/schemas/surface-message.v1.schema.json",
        "protocol/schemas/surface-scene.v1.schema.json",
        "protocol/schemas/surface-stream.v1.schema.json",
        "sdk/surface/README.md",
        "sdk/surface/neuro-surface.d.ts"
    ) | Sort-Object
    $actualEntries = @(Get-LoomArchiveFileEntries -ZipPath $zipPath | ForEach-Object { $_.Replace("\", "/") } | Sort-Object)
    Assert-Equal -Expected ($expectedEntries -join "`n") -Actual ($actualEntries -join "`n") -Message "Plugin SDK ZIP contents do not match the public SDK contract."

    $sourceReadmePath = Join-Path $repoRoot "protocol\README.md"
    Assert-True -Condition (Test-Path -LiteralPath $sourceReadmePath -PathType Leaf) -Message "Release source protocol README is missing."
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $readmeEntry = @($archive.Entries | Where-Object { $_.FullName.Replace("\", "/") -eq "protocol/README.md" })
        Assert-Equal -Expected 1 -Actual $readmeEntry.Count -Message "Plugin SDK ZIP must contain one protocol README."
        $sdkReadmeBytes = Read-LoomArchiveEntryBytes -Entry $readmeEntry[0] -MaxBytes 1MB
    }
    finally {
        $archive.Dispose()
    }
    $sourceReadmeBytes = Read-LoomBoundedFileBytes -Path $sourceReadmePath -MaxBytes 1MB
    Assert-Equal -Expected $sourceReadmeBytes.Length -Actual $sdkReadmeBytes.Length -Message "Plugin SDK protocol README byte count does not match the release source."
    Assert-Equal `
        -Expected (Get-Sha256HexForBytes -Bytes $sourceReadmeBytes) `
        -Actual (Get-Sha256HexForBytes -Bytes $sdkReadmeBytes) `
        -Message "Plugin SDK protocol README does not match the release source."

    $sdkProperty = $Manifest.PSObject.Properties["pluginSdkArtifact"]
    Assert-True -Condition ($null -ne $sdkProperty -and $null -ne $sdkProperty.Value) -Message "Manifest is missing pluginSdkArtifact."
    Assert-Equal -Expected "loom-plugin.exe" -Actual ([string]$sdkProperty.Value.entryName) -Message "Plugin SDK entry name mismatch."
    Assert-Equal -Expected "loom.framework.v1" -Actual ([string]$sdkProperty.Value.protocolVersion) -Message "Plugin SDK protocol version mismatch."
    Assert-Equal -Expected 11 -Actual ([int]$sdkProperty.Value.schemaCount) -Message "Plugin SDK schema count mismatch."
}
