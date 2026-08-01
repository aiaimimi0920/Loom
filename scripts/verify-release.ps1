[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [switch]$RunSmoke,
    [switch]$RequireCleanSource
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$layoutPath = Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1"
. $layoutPath

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    $matches = if ($Expected -is [string] -or $Actual -is [string]) {
        [string]::Equals([string]$Expected, [string]$Actual, [System.StringComparison]::Ordinal)
    }
    else {
        $Expected -eq $Actual
    }
    if (-not $matches) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Invoke-CapturedPowerShell {
    param([string[]]$Arguments)

    $previousErrorActionPreference = $ErrorActionPreference
    $output = @()
    $exitCode = 1
    try {
        $ErrorActionPreference = "Continue"
        $output = @(& powershell.exe @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    catch {
        $output = @($_.Exception.ToString())
        $exitCode = 1
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    return [pscustomobject]@{
        exitCode = $exitCode
        output = @($output)
    }
}

function Resolve-PackageRelativePath {
    param(
        [string]$BasePath,
        [string]$RelativePath
    )

    Assert-True -Condition (-not [System.IO.Path]::IsPathRooted($RelativePath)) -Message "Package path must be relative: $RelativePath"
    Assert-True -Condition (-not $RelativePath.Contains("..")) -Message "Package path must not contain parent traversal: $RelativePath"
    return [System.IO.Path]::GetFullPath((Join-Path $BasePath $RelativePath))
}

function Get-ManifestRecord {
    param(
        [object]$Manifest,
        [string]$Name
    )

    $property = $Manifest.PSObject.Properties[$Name]
    Assert-True -Condition ($null -ne $property) -Message "Manifest is missing $Name."
    return @($property.Value)
}

function Assert-FileRecord {
    param(
        [string]$PackagePath,
        [object]$Record
    )

    $relativePath = [string]$Record.path
    Assert-True -Condition (-not [string]::IsNullOrWhiteSpace($relativePath)) -Message "Manifest file record has an empty path."
    $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
    Assert-True -Condition (Test-Path -LiteralPath $filePath -PathType Leaf) -Message "Manifest file is missing: $relativePath"
    $file = Get-Item -LiteralPath $filePath
    Assert-Equal -Expected ([int64]$Record.bytes) -Actual ([int64]$file.Length) -Message "Manifest byte count mismatch for $relativePath."
    $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Equal -Expected ([string]$Record.sha256) -Actual $actualHash -Message "Manifest SHA-256 mismatch for $relativePath."
    return $relativePath.Replace("/", "\")
}

function Get-ChecksumEntries {
    param([string]$PackagePath)

    $checksumPath = Join-Path $PackagePath "checksums.sha256"
    Assert-True -Condition (Test-Path -LiteralPath $checksumPath -PathType Leaf) -Message "Missing checksums.sha256."
    $entries = @{}
    foreach ($line in (Get-Content -LiteralPath $checksumPath -Encoding ASCII)) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            throw "Invalid checksum line: $line"
        }
        $relativePath = $Matches[2].Replace("/", "\")
        Assert-True -Condition (-not $entries.ContainsKey($relativePath)) -Message "Duplicate checksum entry: $relativePath"
        $entries[$relativePath] = $Matches[1].ToLowerInvariant()
    }
    return $entries
}

function Assert-Checksums {
    param(
        [string]$PackagePath,
        [hashtable]$Entries
    )

    $actualFiles = @(Get-ChildItem -LiteralPath $PackagePath -Recurse -File | ForEach-Object {
        $_.FullName.Substring($PackagePath.Length + 1).Replace("/", "\")
    } | Where-Object { $_ -ne "checksums.sha256" })
    Assert-Equal -Expected ($actualFiles.Count) -Actual ($Entries.Keys.Count) -Message "Checksum entry count must equal all package files except checksums.sha256."
    foreach ($relativePath in $actualFiles) {
        Assert-True -Condition $Entries.ContainsKey($relativePath) -Message "Missing checksum entry: $relativePath"
        $filePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $relativePath
        $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
        Assert-Equal -Expected $Entries[$relativePath] -Actual $actualHash -Message "Checksum mismatch for $relativePath"
    }
    foreach ($relativePath in $Entries.Keys) {
        Assert-True -Condition ($actualFiles -contains $relativePath) -Message "Checksum references an untracked package file: $relativePath"
    }
}

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
        "protocol/schemas/framework-manifest.v1.schema.json"
    ) | Sort-Object
    $actualEntries = @(Get-LoomArchiveFileEntries -ZipPath $zipPath | ForEach-Object { $_.Replace("\", "/") } | Sort-Object)
    Assert-Equal -Expected ($expectedEntries -join "`n") -Actual ($actualEntries -join "`n") -Message "Plugin SDK ZIP contents do not match the public SDK contract."

    $sdkProperty = $Manifest.PSObject.Properties["pluginSdkArtifact"]
    Assert-True -Condition ($null -ne $sdkProperty -and $null -ne $sdkProperty.Value) -Message "Manifest is missing pluginSdkArtifact."
    Assert-Equal -Expected "loom-plugin.exe" -Actual ([string]$sdkProperty.Value.entryName) -Message "Plugin SDK entry name mismatch."
    Assert-Equal -Expected "loom.framework.v1" -Actual ([string]$sdkProperty.Value.protocolVersion) -Message "Plugin SDK protocol version mismatch."
    Assert-Equal -Expected 5 -Actual ([int]$sdkProperty.Value.schemaCount) -Message "Plugin SDK schema count mismatch."
}

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
        $document = Get-Content -Raw -Encoding UTF8 -LiteralPath $path | ConvertFrom-Json
        if ($relative.EndsWith(".cdx.json", [System.StringComparison]::OrdinalIgnoreCase)) {
            Assert-Equal -Expected "CycloneDX" -Actual ([string]$document.bomFormat) -Message "CycloneDX SBOM format mismatch."
            Assert-Equal -Expected "1.6" -Actual ([string]$document.specVersion) -Message "CycloneDX SBOM version mismatch."
        }
        elseif ($relative.EndsWith(".spdx.json", [System.StringComparison]::OrdinalIgnoreCase)) {
            Assert-Equal -Expected "SPDX-2.3" -Actual ([string]$document.spdxVersion) -Message "SPDX SBOM version mismatch."
        }
        else {
            throw "Unknown SBOM format: $relative"
        }
    }
    $provenanceRecords = @(Get-ManifestRecord -Manifest $Manifest -Name "provenance")
    Assert-Equal -Expected 1 -Actual $provenanceRecords.Count -Message "Manifest must contain one provenance record."
    $provenanceRelative = Assert-FileRecord -PackagePath $PackagePath -Record $provenanceRecords[0]
    $provenancePath = Resolve-PackageRelativePath -BasePath $PackagePath -RelativePath $provenanceRelative
    $provenance = Get-Content -Raw -Encoding UTF8 -LiteralPath $provenancePath | ConvertFrom-Json
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
    $actualZipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $expectedLine = "$actualZipHash  $zipName"
    $expectedContent = $expectedLine + "`r`n"
    $actualContent = [System.IO.File]::ReadAllText($sidecarPath, [System.Text.Encoding]::ASCII)
    $contentMatches = (
        [string]::Equals($actualContent, $expectedLine, [System.StringComparison]::Ordinal) -or
        [string]::Equals($actualContent, $expectedContent, [System.StringComparison]::Ordinal)
    )
    if (-not $contentMatches) {
        throw "ZIP checksum sidecar content mismatch for $zipName."
    }
    Assert-Equal -Expected ([string]$ZipRecord.sha256) -Actual $actualZipHash -Message "ZIP artifact SHA-256 mismatch for $zipName."
}

$packageFullPath = [System.IO.Path]::GetFullPath($PackageDir)
Assert-True -Condition (Test-Path -LiteralPath $packageFullPath -PathType Container) -Message "Package directory is missing: $packageFullPath"
$layout = Get-LoomReleaseLayout -PackageDir $packageFullPath

$forbiddenOptionalPayloadPrefixes = @(
    "framework-packages/",
    "art-packages/samples/",
    "resources/script-arts/",
    "resources/workflow-arts/",
    "framework-runtimes/",
    "runtime/python/Arts/"
)
$packageFiles = Get-ChildItem -LiteralPath $packageFullPath -Recurse -File | ForEach-Object {
    $_.FullName.Substring($packageFullPath.Length).TrimStart('\', '/').Replace('\', '/')
}
foreach ($relativePath in $packageFiles) {
    foreach ($forbiddenPrefix in $forbiddenOptionalPayloadPrefixes) {
        Assert-True -Condition (-not $relativePath.StartsWith($forbiddenPrefix, [System.StringComparison]::OrdinalIgnoreCase)) -Message "Default Loom release must not contain optional plugin payload: $relativePath"
    }
}

$manifestPath = Join-Path $packageFullPath "manifest.json"
Assert-True -Condition (Test-Path -LiteralPath $manifestPath -PathType Leaf) -Message "Missing manifest.json."
$manifest = $layout.manifest
Assert-Equal -Expected 1 -Actual ([int]$manifest.schemaVersion) -Message "Manifest schema version mismatch."
Assert-Equal -Expected "Loom" -Actual ([string]$manifest.app) -Message "Manifest app mismatch."
Assert-Equal -Expected "Loom" -Actual ([string]$manifest.sourceProject) -Message "Manifest source project mismatch."
Assert-Equal -Expected "windows-x64" -Actual ([string]$manifest.target) -Message "Manifest target mismatch."
Assert-Equal -Expected "." -Actual (@($manifest.sourcePaths) -join ",") -Message "Manifest source paths must be standalone-relative."
Assert-True -Condition (-not ([string]$manifest.repoRoot).Contains("Neuro")) -Message "Manifest must not contain parent repository paths."
Assert-True -Condition (-not ([string]$manifest.destination).Contains(":")) -Message "Manifest destination must not contain an absolute local path."
if ($RequireCleanSource) {
    Assert-Equal -Expected $false -Actual ([bool]$manifest.gitDirty) -Message "Formal release manifest must record gitDirty=false."
    Assert-Equal -Expected $false -Actual ([bool]$manifest.sourceGitDirty) -Message "Formal release source must record sourceGitDirty=false."
}

$exeRecords = @(Get-ManifestRecord -Manifest $manifest -Name "exes")
$expectedExeNames = @("Loom.exe", "loom-daemon.exe")
Assert-Equal -Expected ($expectedExeNames -join ",") -Actual (@($exeRecords | ForEach-Object { [string]$_.name }) -join ",") -Message "Manifest executable set mismatch."
$expectedExePaths = @("Loom.exe", "runtime\loom-daemon.exe")
$actualExePaths = @($exeRecords | ForEach-Object { ([string]$_.path).Replace("/", "\") })
Assert-Equal -Expected ($expectedExePaths -join ",") -Actual ($actualExePaths -join ",") -Message "Manifest executable layout mismatch."
Assert-True -Condition (@($exeRecords | Where-Object { [string]$_.path -in @("loom-desktop.exe", "loom-daemon.exe") }).Count -eq 0) -Message "Desktop package must not expose root-level daemon or legacy desktop executables."
$payloadPaths = @()
foreach ($record in $exeRecords) {
    $payloadPaths += Assert-FileRecord -PackagePath $packageFullPath -Record $record
}

$supportRecords = @(Get-ManifestRecord -Manifest $manifest -Name "supportFiles")
foreach ($record in $supportRecords) {
    $payloadPaths += Assert-FileRecord -PackagePath $packageFullPath -Record $record
}

$buildInfo = @(Get-ManifestRecord -Manifest $manifest -Name "buildInfo")
Assert-Equal -Expected 1 -Actual $buildInfo.Count -Message "Manifest must contain one BUILD_INFO record."
[void](Assert-FileRecord -PackagePath $packageFullPath -Record $buildInfo[0])

$commands = @(Get-ManifestRecord -Manifest $manifest -Name "commands")
Assert-True -Condition ($commands.Count -ge 2) -Message "Manifest must retain build command provenance."
$artifactRecords = @(Get-ManifestRecord -Manifest $manifest -Name "artifacts")
foreach ($artifact in $artifactRecords) {
    [void](Assert-FileRecord -PackagePath $packageFullPath -Record $artifact)
}
$desktopZipRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "desktop-zip" })
Assert-Equal -Expected 1 -Actual $desktopZipRecord.Count -Message "Manifest must contain exactly one desktop payload ZIP."
$cliZipRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "cli-zip" })
Assert-Equal -Expected 1 -Actual $cliZipRecord.Count -Message "Manifest must contain exactly one CLI ZIP."
$desktopSidecarRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "zip-sha256" })
Assert-Equal -Expected 1 -Actual $desktopSidecarRecord.Count -Message "Manifest must contain exactly one desktop ZIP checksum sidecar."
$cliSidecarRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "cli-zip-sha256" })
Assert-Equal -Expected 1 -Actual $cliSidecarRecord.Count -Message "Manifest must contain exactly one CLI ZIP checksum sidecar."
$pluginSdkZipRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "plugin-sdk-zip" })
Assert-Equal -Expected 1 -Actual $pluginSdkZipRecord.Count -Message "Manifest must contain exactly one plugin SDK ZIP."
$pluginSdkSidecarRecord = @($artifactRecords | Where-Object { Test-LoomArtifactKind -Artifact $_ -Kind "plugin-sdk-zip-sha256" })
Assert-Equal -Expected 1 -Actual $pluginSdkSidecarRecord.Count -Message "Manifest must contain exactly one plugin SDK ZIP checksum sidecar."
$expectedDesktopZipName = "Loom-$($manifest.versionId)-windows-x64.zip"
$expectedCliZipName = "Loom-CLI-$($manifest.versionId)-windows-x64.zip"
$expectedPluginSdkZipName = "Loom-Plugin-SDK-$($manifest.versionId)-windows-x64.zip"
Assert-Equal -Expected $expectedDesktopZipName -Actual ([string]$desktopZipRecord[0].name) -Message "Desktop ZIP name does not match the manifest version."
Assert-Equal -Expected $expectedCliZipName -Actual ([string]$cliZipRecord[0].name) -Message "CLI ZIP name does not match the manifest version."
Assert-Equal -Expected $expectedPluginSdkZipName -Actual ([string]$pluginSdkZipRecord[0].name) -Message "Plugin SDK ZIP name does not match the manifest version."
Assert-Equal -Expected "packages\$expectedDesktopZipName" -Actual (([string]$desktopZipRecord[0].path).Replace("/", "\")) -Message "Desktop ZIP path does not match its name."
Assert-Equal -Expected "packages\$expectedCliZipName" -Actual (([string]$cliZipRecord[0].path).Replace("/", "\")) -Message "CLI ZIP path does not match its name."
Assert-Equal -Expected "packages\$expectedPluginSdkZipName" -Actual (([string]$pluginSdkZipRecord[0].path).Replace("/", "\")) -Message "Plugin SDK ZIP path does not match its name."
Assert-ZipChecksumSidecar -PackagePath $packageFullPath -ZipRecord $desktopZipRecord[0] -SidecarRecord $desktopSidecarRecord[0]
Assert-ZipChecksumSidecar -PackagePath $packageFullPath -ZipRecord $cliZipRecord[0] -SidecarRecord $cliSidecarRecord[0]
Assert-ZipChecksumSidecar -PackagePath $packageFullPath -ZipRecord $pluginSdkZipRecord[0] -SidecarRecord $pluginSdkSidecarRecord[0]

$checksumEntries = Get-ChecksumEntries -PackagePath $packageFullPath
Assert-Checksums -PackagePath $packageFullPath -Entries $checksumEntries
Assert-ZipPayload -PackagePath $packageFullPath -Manifest $manifest -ExpectedPayloadPaths $payloadPaths
Assert-CliZipPayload -PackagePath $packageFullPath -Manifest $manifest
Assert-PluginSdkZipPayload -PackagePath $packageFullPath -Manifest $manifest
Assert-SupplyChainMetadata -PackagePath $packageFullPath -Manifest $manifest

$smokeStatus = "not-run"
$hookCanvasSmokeStatus = "not-run"
$hookErrorPreviewSmokeStatus = "not-run"
$frameworkArtStoreHookSmokeStatus = "not-run"
$pluginBoundarySmokeStatus = "not-run"
if ($RunSmoke) {
    $smokePath = Join-Path $repoRoot "scripts\smoke-release.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $smokePath -PathType Leaf) -Message "Missing standalone smoke script: $smokePath"
    $evidenceRoot = Join-Path $repoRoot "target\runtime-smoke"
    $smokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $smokePath,
        "-PackageDir", $packageFullPath,
        "-EvidenceRoot", $evidenceRoot
    )
    $smokeOutput = @($smokeResult.output)
    if ([int]$smokeResult.exitCode -ne 0) {
        throw "Standalone release smoke failed: $($smokeOutput -join [Environment]::NewLine)"
    }
    $smokeStatus = "passed"

    $hookCanvasSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookCanvasUiSmoke.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $hookCanvasSmokePath -PathType Leaf) -Message "Missing Hook canvas UI smoke script: $hookCanvasSmokePath"
    $hookCanvasEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\hook-canvas"
    $hookCanvasSmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $hookCanvasSmokePath,
        "-PackageDir", $packageFullPath,
        "-EvidenceRoot", $hookCanvasEvidenceRoot
    )
    $hookCanvasSmokeOutput = @($hookCanvasSmokeResult.output)
    if ([int]$hookCanvasSmokeResult.exitCode -ne 0) {
        throw "Hook canvas UI smoke failed: $($hookCanvasSmokeOutput -join [Environment]::NewLine)"
    }
    $hookCanvasSmokeStatus = "passed"

    $hookErrorPreviewSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookErrorPreviewSmoke.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $hookErrorPreviewSmokePath -PathType Leaf) -Message "Missing Hook error preview smoke script: $hookErrorPreviewSmokePath"
    $hookErrorPreviewEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\hook-error-preview"
    $hookErrorPreviewSmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $hookErrorPreviewSmokePath,
        "-PackageDir", $packageFullPath,
        "-EvidenceRoot", $hookErrorPreviewEvidenceRoot
    )
    $hookErrorPreviewSmokeOutput = @($hookErrorPreviewSmokeResult.output)
    if ([int]$hookErrorPreviewSmokeResult.exitCode -ne 0) {
        throw "Hook error preview smoke failed: $($hookErrorPreviewSmokeOutput -join [Environment]::NewLine)"
    }
    $hookErrorPreviewSmokeStatus = "passed"

    $frameworkArtStoreHookSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $frameworkArtStoreHookSmokePath -PathType Leaf) -Message "Missing framework art-store Hook smoke script: $frameworkArtStoreHookSmokePath"
    $frameworkArtStoreEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\framework-art-store-hook"
    $frameworkArtStoreHookSmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $frameworkArtStoreHookSmokePath,
        "-PackageDir", $packageFullPath,
        "-Configuration", "Release",
        "-EvidenceRoot", $frameworkArtStoreEvidenceRoot
    )
    $frameworkArtStoreHookSmokeOutput = @($frameworkArtStoreHookSmokeResult.output)
    if ([int]$frameworkArtStoreHookSmokeResult.exitCode -ne 0) {
        throw "Framework art-store Hook smoke failed: $($frameworkArtStoreHookSmokeOutput -join [Environment]::NewLine)"
    }
    $frameworkArtStoreHookSmokeStatus = "passed"

    $pluginBoundarySmokePath = Join-Path $repoRoot "scripts\Invoke-LoomPluginBoundarySmoke.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $pluginBoundarySmokePath -PathType Leaf) -Message "Missing plugin boundary smoke script: $pluginBoundarySmokePath"
    $pluginBoundaryEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\plugin-boundary"
    $pluginBoundarySmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $pluginBoundarySmokePath,
        "-DaemonExecutable", (Join-Path $packageFullPath "runtime\loom-daemon.exe"),
        "-EvidenceRoot", $pluginBoundaryEvidenceRoot
    )
    $pluginBoundarySmokeOutput = @($pluginBoundarySmokeResult.output)
    if ([int]$pluginBoundarySmokeResult.exitCode -ne 0) {
        throw "Plugin Art boundary smoke failed: $($pluginBoundarySmokeOutput -join [Environment]::NewLine)"
    }
    $pluginBoundarySmokeStatus = "passed"
}

$result = [ordered]@{
    schemaVersion = 1
    mode = "verify"
    app = "Loom"
    packageDir = $packageFullPath
    manifest = $manifestPath
    filesChecked = @($checksumEntries.Keys).Count
    smoke = $smokeStatus
    hookCanvasSmoke = $hookCanvasSmokeStatus
    hookErrorPreviewSmoke = $hookErrorPreviewSmokeStatus
    frameworkArtStoreHookSmoke = $frameworkArtStoreHookSmokeStatus
    pluginBoundarySmoke = $pluginBoundarySmokeStatus
}
Write-Output ($result | ConvertTo-Json -Depth 10)
