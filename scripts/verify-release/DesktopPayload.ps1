<# Owns one release-script responsibility. #>

function Assert-ZipPayload {
    param(
        [string]$PackagePath,
        [object]$Manifest,
        [string[]]$ExpectedPayloadPaths
    )

    $artifacts = Get-ManifestRecord -Manifest $Manifest -Name "artifacts"
    $zipRecord = @($artifacts | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "desktop-zip" })
    Assert-Equal -Expected 1 -Actual $zipRecord.Count -Message "Manifest must contain exactly one desktop payload ZIP."
    $zipPath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath ([string]$zipRecord[0].path)
    Assert-True -Condition (Test-Path -LiteralPath $zipPath -PathType Leaf) -Message "Payload ZIP is missing."

    $actualEntries = @(Get-LoomArchiveFileEntries -ZipPath $zipPath)
    $expected = @($ExpectedPayloadPaths | Sort-Object)
    Assert-Equal -Expected ($expected -join "`n") -Actual ($actualEntries -join "`n") -Message "Payload ZIP contents do not match executable/support files."
}
