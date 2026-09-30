[CmdletBinding()]
param(
    [string]$VersionId = "",
    [string]$OutputRoot = ".\release\Loom",
    [string]$ExtensionCompatibilityPath = "",
    [string]$PreparedPayloadRoot = "",
    [switch]$NoZip,
    [switch]$DryRun,
    [switch]$RequireCleanSource
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$targetName = "windows-x64"
$layoutPath = Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1"
. $layoutPath
. (Join-Path $PSScriptRoot "ExtensionCompatibility.ps1")

$moduleRoot = Join-Path $PSScriptRoot "build-release"
. (Join-Path $moduleRoot "Common.ps1")
. (Join-Path $moduleRoot "Catalog.ps1")
. (Join-Path $moduleRoot "Plan.ps1")
. (Join-Path $moduleRoot "Execution.ps1")
. (Join-Path $moduleRoot "FrameworkPackages.ps1")
. (Join-Path $moduleRoot "McpPackages.ps1")
. (Join-Path $moduleRoot "ArtPackages.ps1")
. (Join-Path $moduleRoot "Metadata.ps1")
. (Join-Path $moduleRoot "Archives.ps1")

$resolvedVersionId = Resolve-VersionId -ExplicitVersionId $VersionId
$resolvedOutputRoot = Resolve-OutputRoot -Value $OutputRoot
Assert-LoomBuildOutputRoot -OutputRoot $resolvedOutputRoot
$destination = Resolve-LoomPackageRelativePath -PackageDir $resolvedOutputRoot -RelativePath $resolvedVersionId
$catalog = Get-LoomCatalog `
    -FrameworkPackageOutputRoot (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "packages\frameworks") `
    -McpServerPackageOutputRoot (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "packages\mcp-servers") `
    -SampleArtPackageOutputRoot (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "packages\arts")
$extensionCompatibility = $null
if (-not [string]::IsNullOrWhiteSpace($ExtensionCompatibilityPath)) {
    $extensionCompatibility = Read-LoomExtensionCompatibility -Path $ExtensionCompatibilityPath
    $catalog.supportFiles = @($catalog.supportFiles) + @(
        New-SupportSpec `
            -Source ([System.IO.Path]::GetFullPath($ExtensionCompatibilityPath)) `
            -DestinationRelativePath "extension-compatibility.json"
    )
}
$preparedPayload = $null
if (-not [string]::IsNullOrWhiteSpace($PreparedPayloadRoot)) {
    $preparedPayload = [System.IO.Path]::GetFullPath($PreparedPayloadRoot)
    if (-not (Test-Path -LiteralPath $preparedPayload -PathType Container)) {
        throw "Prepared Loom payload directory is missing: $preparedPayload"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath $preparedPayload -Path $preparedPayload
    $preparedManifest = Read-LoomBoundedJsonFile `
        -Path (Resolve-LoomPackageRelativePath -PackageDir $preparedPayload -RelativePath "manifest.json") `
        -MaxBytes 4MB
    $preparedGitHead = Get-GitText -Arguments @("rev-parse", "HEAD")
    if ([string]$preparedManifest.app -cne "Loom" -or
        [string]$preparedManifest.target -cne $targetName -or
        [string]$preparedManifest.gitHead -cne $preparedGitHead) {
        throw "Prepared Loom payload does not match the current Loom source commit and target."
    }
    $catalog.exes = @(
        New-ExeSpec -Name "Loom.exe" `
            -Source (Resolve-LoomPackageRelativePath -PackageDir $preparedPayload -RelativePath "Loom.exe") `
            -DestinationRelativePath "Loom.exe"
        New-ExeSpec -Name "loom-daemon.exe" `
            -Source (Resolve-LoomPackageRelativePath -PackageDir $preparedPayload -RelativePath "runtime\loom-daemon.exe") `
            -DestinationRelativePath "runtime\loom-daemon.exe"
    )
    foreach ($exe in $catalog.exes) {
        if (-not (Test-Path -LiteralPath $exe.source -PathType Leaf)) {
            throw "Prepared Loom executable is missing: $($exe.source)"
        }
        Assert-LoomPathHasNoReparsePoints -RootPath $preparedPayload -Path $exe.source
        $relative = ([string]$exe.destinationRelativePath).Replace("/", "\")
        $record = @($preparedManifest.exes | Where-Object {
            ([string]$_.path).Replace("/", "\") -ceq $relative
        })
        if ($record.Count -ne 1) {
            throw "Prepared Loom manifest must contain one executable record for $relative."
        }
        $digest = Get-LoomFileDigest -Path $exe.source
        if ([int64]$record[0].bytes -ne [int64]$digest.bytes -or
            [string]$record[0].sha256 -cne [string]$digest.sha256) {
            throw "Prepared Loom executable does not match its manifest record: $relative"
        }
    }
    $catalog.commands = @($catalog.commands | Select-Object -Skip 3)
}
$sourceGitDirty = Get-GitDirty
if ($null -eq $sourceGitDirty) {
    throw "Loom release build requires a readable Git worktree state."
}
if ($null -ne $preparedPayload) {
    if (-not ($preparedManifest.PSObject.Properties.Name -contains "sourceGitDirty") -or
        $null -eq $preparedManifest.sourceGitDirty) {
        throw "Prepared Loom manifest must record a definite sourceGitDirty value."
    }
    if ([bool]$preparedManifest.sourceGitDirty -ne [bool]$sourceGitDirty) {
        throw "Prepared Loom manifest does not match the current worktree state."
    }
    $sourceGitDirty = [bool]$preparedManifest.sourceGitDirty
}
if ($RequireCleanSource -and $sourceGitDirty -ne $false) {
    throw "Formal Loom release requires a clean, readable Git worktree. gitDirty=$sourceGitDirty"
}

if ($DryRun) {
    $plan = New-Plan -Catalog $catalog -ResolvedVersionId $resolvedVersionId -ResolvedOutputRoot $resolvedOutputRoot -Destination $destination
    $plan.preparedPayload = $null -ne $preparedPayload
    Write-Output ($plan | ConvertTo-Json -Depth 20)
    exit 0
}

if (Test-Path -LiteralPath $destination) {
    throw "Release destination already exists: $destination"
}

if (-not (Test-Path -LiteralPath $resolvedOutputRoot)) {
    New-Item -ItemType Directory -Path $resolvedOutputRoot -Force | Out-Null
}
Assert-LoomBuildOutputRoot -OutputRoot $resolvedOutputRoot
New-Item -ItemType Directory -Path $destination | Out-Null
Assert-LoomPathHasNoReparsePoints -RootPath $resolvedOutputRoot -Path $destination
$logRoot = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "logs"
New-Item -ItemType Directory -Path $logRoot | Out-Null

$commandRecords = @()
for ($index = 0; $index -lt @($catalog.commands).Count; $index++) {
    $command = $catalog.commands[$index]
    $logPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath (Join-Path "logs" $command.logName)
    Invoke-CommandToLog -Command $command -LogPath $logPath
    $commandRecords += [ordered]@{
        display = $command.display
        workingDirectory = Get-RepoRelativeOrExternal -Path $command.workingDirectory
        logPath = "logs\$($command.logName)"
    }
}

$exeRecords = @()
foreach ($exe in $catalog.exes) {
    $relative = [string]$exe.destinationRelativePath
    $destinationPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath $relative
    $exeRecords += Copy-PayloadFile `
        -PackageRoot $destination `
        -Source $exe.source `
        -Destination $destinationPath `
        -RelativePath $relative `
        -Kind "exe"
}

$supportRecords = @()
foreach ($support in $catalog.supportFiles) {
    $relative = [string]$support.destinationRelativePath
    $destinationPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath $relative
    $supportRecords += Copy-PayloadFile `
        -PackageRoot $destination `
        -Source $support.source `
        -Destination $destinationPath `
        -RelativePath $relative `
        -Kind "support-file"
}

$frameworkArtifacts = Get-FrameworkPackageArtifacts -FrameworkCatalog $catalog.frameworkPackageCatalog
$frameworkPackageRecords = @($frameworkArtifacts.packages)
$frameworkCatalogRecord = $frameworkArtifacts.catalog
$frameworkArtifactRecords = @($frameworkArtifacts.artifacts)
$frameworkPayloadRecords = @($frameworkArtifacts.payload)
$mcpServerArtifacts = Get-McpServerPackageArtifacts -McpCatalog $catalog.mcpServerPackageCatalog
$mcpServerPackageRecords = @($mcpServerArtifacts.packages)
$mcpServerCatalogRecord = $mcpServerArtifacts.catalog
$mcpServerArtifactRecords = @($mcpServerArtifacts.artifacts)
$mcpServerPayloadRecords = @($mcpServerArtifacts.payload)
$sampleArtArtifacts = Get-SampleArtPackageArtifacts -ArtCatalog $catalog.sampleArtPackageCatalog
$sampleArtPackageRecords = @($sampleArtArtifacts.packages)
$sampleArtCatalogRecord = $sampleArtArtifacts.catalog
$sampleArtArtifactRecords = @($sampleArtArtifacts.artifacts)
$sampleArtPayloadRecords = @($sampleArtArtifacts.payload)

$gitHead = Get-GitText -Arguments @("rev-parse", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitHead)) {
    $gitHead = "unknown"
}
$gitShortSha = Get-GitText -Arguments @("rev-parse", "--short=8", "HEAD")
if ([string]::IsNullOrWhiteSpace($gitShortSha)) {
    $gitShortSha = "nogit"
}
$gitDirty = $sourceGitDirty
$extensionCompatibilityRecord = @($supportRecords | Where-Object {
    ([string]$_.path).Replace("\", "/") -ceq "extension-compatibility.json"
})
if ($null -ne $extensionCompatibility) {
    if ($extensionCompatibilityRecord.Count -ne 1) {
        throw "The Loom release must contain one extension compatibility record."
    }
    Assert-LoomExtensionCompatibility `
        -Document $extensionCompatibility `
        -GitHead $gitHead `
        -ExecutableRecords $exeRecords
}

$buildInfoPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "BUILD_INFO.txt"
Write-Utf8NoBom -Path $buildInfoPath -Value (New-BuildInfo `
    -ResolvedVersionId $resolvedVersionId `
    -ResolvedOutputRoot (Get-RepoRelativeOrExternal -Path $resolvedOutputRoot) `
    -Catalog $catalog `
    -GitHead $gitHead `
    -GitDirty $gitDirty)
$buildInfoDigest = Get-LoomFileDigest -Path $buildInfoPath
$buildInfo = [ordered]@{
    kind = "build-info"
    name = "BUILD_INFO.txt"
    path = "BUILD_INFO.txt"
    bytes = [int64]$buildInfoDigest.bytes
    sha256 = $buildInfoDigest.sha256
}

$payloadRecords = @($exeRecords + $supportRecords + $frameworkPayloadRecords + $mcpServerPayloadRecords + $sampleArtPayloadRecords)
$artifactRecords = @($frameworkArtifactRecords + $mcpServerArtifactRecords + $sampleArtArtifactRecords)
$cliArtifactManifest = $null
$pluginSdkArtifactManifest = $null
if (-not $NoZip) {
    $desktopArtifactRecords = @(New-PayloadZip -Destination $destination -ResolvedVersionId $resolvedVersionId -PayloadRecords $payloadRecords)
    $cliArtifactRecords = @(New-CliZip -Destination $destination -ResolvedVersionId $resolvedVersionId -CliArtifact $catalog.cliArtifact)
    $pluginSdkArtifactRecords = @(New-PluginSdkZip -Destination $destination -ResolvedVersionId $resolvedVersionId -PluginSdkArtifact $catalog.pluginSdkArtifact)
    $artifactRecords = @(
        $desktopArtifactRecords +
        $cliArtifactRecords +
        $pluginSdkArtifactRecords +
        $frameworkArtifactRecords +
        $mcpServerArtifactRecords +
        $sampleArtArtifactRecords
    )
    $cliZipRecord = @($cliArtifactRecords | Where-Object { [string]$_.kind -eq "cli-zip" })[0]
    $cliArtifactManifest = [ordered]@{
        name = $catalog.cliArtifact.name
        entryName = $catalog.cliArtifact.entryName
        zipName = $cliZipRecord.name
        path = $cliZipRecord.path
        bytes = $cliZipRecord.bytes
        sha256 = $cliZipRecord.sha256
    }
    $pluginSdkZipRecord = @($pluginSdkArtifactRecords | Where-Object { [string]$_.kind -eq "plugin-sdk-zip" })[0]
    $pluginSdkArtifactManifest = [ordered]@{
        name = $catalog.pluginSdkArtifact.name
        entryName = $catalog.pluginSdkArtifact.pluginCliEntryName
        zipName = $pluginSdkZipRecord.name
        path = $pluginSdkZipRecord.path
        bytes = $pluginSdkZipRecord.bytes
        sha256 = $pluginSdkZipRecord.sha256
        protocolVersion = "loom.framework.v1"
        schemaCount = @($catalog.pluginSdkArtifact.files | Where-Object {
            ([string]$_.destinationRelativePath).Replace("\", "/") -like "protocol/schemas/*.schema.json"
        }).Count
    }
}

$sbomDir = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "sbom"
$sbomCommand = New-CommandSpec `
    -Executable "powershell.exe" `
    -Arguments @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $PSScriptRoot "New-LoomSbom.ps1"),
        "-OutputDirectory", $sbomDir, "-Version", $resolvedVersionId
    ) `
    -WorkingDirectory $repoRoot `
    -Display "New-LoomSbom.ps1 -OutputDirectory sbom" `
    -LogName "sbom.log"
Invoke-CommandToLog -Command $sbomCommand -LogPath (Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "logs\sbom.log")
Assert-LoomPathHasNoReparsePoints -RootPath $destination -Path $sbomDir
$sbomRecords = @(Get-LoomSafeDescendantFiles -RootPath $sbomDir | Sort-Object Name | ForEach-Object {
    $digest = Get-LoomFileDigest -Path $_.FullName
    [ordered]@{
        kind = "sbom"
        name = $_.Name
        path = "sbom\$($_.Name)"
        bytes = [int64]$digest.bytes
        sha256 = $digest.sha256
    }
})

$provenanceDir = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "provenance"
$provenancePath = Resolve-LoomPackageRelativePath -PackageDir $provenanceDir -RelativePath "build-provenance.json"
$provenanceSubjects = @($artifactRecords | Where-Object { ([string]$_.kind).EndsWith("-zip") } | ForEach-Object {
    [ordered]@{ name = $_.name; sha256 = $_.sha256; bytes = $_.bytes }
})
if ($extensionCompatibilityRecord.Count -eq 1) {
    $provenanceSubjects += [ordered]@{
        name = "extension-compatibility.json"
        sha256 = $extensionCompatibilityRecord[0].sha256
        bytes = $extensionCompatibilityRecord[0].bytes
    }
}
$provenance = [ordered]@{
    schemaVersion = 1
    builder = "Loom scripts/build-release.ps1"
    versionId = $resolvedVersionId
    target = $targetName
    gitHead = $gitHead
    gitDirty = $gitDirty
    sourcePaths = @(".")
    commands = @($commandRecords)
    subjects = $provenanceSubjects
}
Write-Utf8NoBom -Path $provenancePath -Value (($provenance | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
$provenanceDigest = Get-LoomFileDigest -Path $provenancePath
$provenanceRecord = [ordered]@{
    kind = "provenance"
    name = "build-provenance.json"
    path = "provenance\build-provenance.json"
    bytes = [int64]$provenanceDigest.bytes
    sha256 = $provenanceDigest.sha256
}

$manifest = [ordered]@{
    schemaVersion = 2
    app = "Loom"
    sourceProject = "Loom"
    versionId = $resolvedVersionId
    builtAt = (Get-Date).ToString("o")
    gitHead = $gitHead
    gitShortSha = $gitShortSha
    gitDirty = $gitDirty
    profile = "release"
    target = $targetName
    repoRoot = "."
    outputRoot = Get-RepoRelativeOrExternal -Path $resolvedOutputRoot
    destination = Get-RepoRelativeOrExternal -Path $destination
    commands = $commandRecords
    exes = $exeRecords
    supportFiles = $supportRecords
    extensionCompatibility = if ($extensionCompatibilityRecord.Count -eq 1) { $extensionCompatibilityRecord[0] } else { $null }
    cliArtifact = $cliArtifactManifest
    pluginSdkArtifact = $pluginSdkArtifactManifest
    frameworkPackages = $frameworkPackageRecords
    frameworkCatalog = $frameworkCatalogRecord
    mcpServerPackages = $mcpServerPackageRecords
    mcpServerCatalog = $mcpServerCatalogRecord
    sampleArtPackages = $sampleArtPackageRecords
    sampleArtCatalog = $sampleArtCatalogRecord
    sbom = $sbomRecords
    provenance = $provenanceRecord
    buildInfo = $buildInfo
    artifacts = $artifactRecords
    checksums = "checksums.sha256"
    sourceGitDirty = $gitDirty
    sourcePaths = @(".")
}
$manifestPath = Resolve-LoomPackageRelativePath -PackageDir $destination -RelativePath "manifest.json"
Write-Utf8NoBom -Path $manifestPath -Value (($manifest | ConvertTo-Json -Depth 20) + [Environment]::NewLine)
$checksumRecord = Write-Checksums -Destination $destination

$result = [ordered]@{
    schemaVersion = 1
    mode = "build"
    app = "Loom"
    versionId = $resolvedVersionId
    outputRoot = $resolvedOutputRoot
    destination = $destination
    manifest = $manifestPath
    checksums = (Join-Path $destination "checksums.sha256")
    checksumEntries = $checksumRecord.entries
    zip = (-not $NoZip)
    exes = $exeRecords
    supportFiles = $supportRecords
    cliArtifact = $cliArtifactManifest
    pluginSdkArtifact = $pluginSdkArtifactManifest
    frameworkPackages = $frameworkPackageRecords
    frameworkCatalog = $frameworkCatalogRecord
    sampleArtPackages = $sampleArtPackageRecords
    sampleArtCatalog = $sampleArtCatalogRecord
    sbom = $sbomRecords
    provenance = $provenanceRecord
    artifacts = $artifactRecords
}
Write-Output ($result | ConvertTo-Json -Depth 20)
