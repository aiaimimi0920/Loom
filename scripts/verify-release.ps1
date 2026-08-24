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

$moduleRoot = Join-Path $PSScriptRoot "verify-release"
. (Join-Path $moduleRoot "Common.ps1")
. (Join-Path $moduleRoot "DesktopPayload.ps1")
. (Join-Path $moduleRoot "CliSdkPayload.ps1")
. (Join-Path $moduleRoot "FrameworkPackages.ps1")
. (Join-Path $moduleRoot "McpPackages.ps1")
. (Join-Path $moduleRoot "ArtPackages.ps1")
. (Join-Path $moduleRoot "SupplyChain.ps1")

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
$packageFiles = Get-LoomSafeDescendantFiles -RootPath $packageFullPath | ForEach-Object {
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
Assert-True -Condition ([int]$manifest.schemaVersion -in @(1, 2)) -Message "Manifest schema version mismatch."
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
$payloadPaths += @(Assert-FrameworkPackages -PackagePath $packageFullPath -Manifest $manifest)
$payloadPaths += @(Assert-McpServerPackages -PackagePath $packageFullPath -Manifest $manifest)
$payloadPaths += @(Assert-SampleArtPackages -PackagePath $packageFullPath -Manifest $manifest)

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
$surfacePrototypeSmokeStatus = "not-run"
$authoredArtCreationSmokeStatus = "not-run"
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

    $surfacePrototypeSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomSurfacePrototypeSmoke.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $surfacePrototypeSmokePath -PathType Leaf) -Message "Missing Surface prototype smoke script: $surfacePrototypeSmokePath"
    $surfacePrototypeEvidenceRoot = Join-Path $repoRoot "target\runtime-smoke\surface-prototypes"
    $surfacePrototypeSmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $surfacePrototypeSmokePath,
        "-PackageDir", $packageFullPath,
        "-Configuration", "Release",
        "-EvidenceRoot", $surfacePrototypeEvidenceRoot
    )
    $surfacePrototypeSmokeOutput = @($surfacePrototypeSmokeResult.output)
    if ([int]$surfacePrototypeSmokeResult.exitCode -ne 0) {
        throw "Surface prototype smoke failed: $($surfacePrototypeSmokeOutput -join [Environment]::NewLine)"
    }
    $surfacePrototypeSmokeStatus = "passed"

    $authoredArtCreationSmokePath = Join-Path $repoRoot "scripts\tests\Test-LoomAuthoredArtCreateExecution.ps1"
    Assert-True -Condition (Test-Path -LiteralPath $authoredArtCreationSmokePath -PathType Leaf) -Message "Missing authored Art creation smoke script: $authoredArtCreationSmokePath"
    $authoredArtCreationSmokeResult = Invoke-CapturedPowerShell -Arguments @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $authoredArtCreationSmokePath,
        "-DaemonExecutable", (Join-Path $packageFullPath "runtime\loom-daemon.exe"),
        "-FrameworkArtifactRoot", (Join-Path $packageFullPath "packages\frameworks")
    )
    $authoredArtCreationSmokeOutput = @($authoredArtCreationSmokeResult.output)
    if ([int]$authoredArtCreationSmokeResult.exitCode -ne 0) {
        throw "Authored Art creation smoke failed: $($authoredArtCreationSmokeOutput -join [Environment]::NewLine)"
    }
    $authoredArtCreationSmokeStatus = "passed"
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
    surfacePrototypeSmoke = $surfacePrototypeSmokeStatus
    authoredArtCreationSmoke = $authoredArtCreationSmokeStatus
}
Write-Output ($result | ConvertTo-Json -Depth 10)
