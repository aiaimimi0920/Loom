param(
    [string]$ArtifactRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$packagesRoot = Join-Path $repoRoot "framework-packages"
$buildScript = Join-Path $repoRoot "scripts\Build-LoomArtFrameworkPackages.ps1"
$runtimeHostManifest = Join-Path $packagesRoot "runtime-host\Cargo.toml"
$expectedIds = @(
    "process",
    "cloud_api",
    "mcp",
    "workflow"
)

Assert-True (Test-Path -LiteralPath $packagesRoot -PathType Container) "framework-packages directory is required."
Assert-True (Test-Path -LiteralPath $buildScript -PathType Leaf) "Independent framework package build script is required."
Assert-True (Test-Path -LiteralPath $runtimeHostManifest -PathType Leaf) "External framework runtime host manifest is required."

$manifestFiles = @(Get-ChildItem -LiteralPath $packagesRoot -Directory | ForEach-Object {
    $manifestPath = Join-Path $_.FullName "framework.manifest.json"
    if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
        Get-Item -LiteralPath $manifestPath
    }
})

Assert-True ($manifestFiles.Count -eq $expectedIds.Count) "Expected exactly $($expectedIds.Count) repo-owned framework manifests, found $($manifestFiles.Count)."

$actualIds = @()
foreach ($manifestFile in $manifestFiles) {
    $directoryId = Split-Path -Leaf (Split-Path -Parent $manifestFile.FullName)
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestFile.FullName | ConvertFrom-Json

    Assert-True ($null -ne $manifest.id -and [string]$manifest.id -ne "") "Manifest id is required: $($manifestFile.FullName)"
    Assert-True ([string]$manifest.id -eq $directoryId) "Manifest id '$($manifest.id)' must equal directory '$directoryId'."
    Assert-True ($null -ne $manifest.publisher -and -not [string]::IsNullOrWhiteSpace([string]$manifest.publisher.id)) "Manifest publisher id is required: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.version -and [string]$manifest.version -ne "") "Manifest version is required: $($manifestFile.FullName)"
    Assert-True ([string]$manifest.protocolVersion -eq "loom.framework.v1") "Manifest protocolVersion must be loom.framework.v1: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.platforms -and @($manifest.platforms) -contains "windows-x64") "Manifest platforms must contain windows-x64: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.entry) "Manifest entry is required: $($manifestFile.FullName)"
    Assert-True ([string]$manifest.entry.kind -eq "process") "Manifest entry.kind must be process: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.entry.command -and [string]$manifest.entry.command -ne "") "Manifest entry.command is required: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.permissions) "Manifest permissions is required: $($manifestFile.FullName)"
    Assert-True ($manifest.permissions -is [array]) "Manifest permissions must be an array: $($manifestFile.FullName)"
    Assert-True ($null -ne $manifest.artExecution) "Manifest artExecution is required: $($manifestFile.FullName)"
    Assert-True ([string]$manifest.artExecution.requestSchema -eq "loom.art.execute.v1") "Manifest request schema is invalid: $($manifestFile.FullName)"
    Assert-True ([string]$manifest.artExecution.responseSchema -eq "loom.art.result.v1") "Manifest response schema is invalid: $($manifestFile.FullName)"

    $actualIds += [string]$manifest.id
}

foreach ($expectedId in $expectedIds) {
    Assert-True ($actualIds -contains $expectedId) "Missing repo-owned framework manifest: $expectedId"
}

if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $artifactRootPath = if ([System.IO.Path]::IsPathRooted($ArtifactRoot)) {
        [System.IO.Path]::GetFullPath($ArtifactRoot)
    }
    else {
        [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
    }
    Assert-True (Test-Path -LiteralPath $artifactRootPath -PathType Container) "Framework artifact root is required: $artifactRootPath"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipFiles = @(Get-ChildItem -LiteralPath $artifactRootPath -Filter *.zip -File)
    Assert-True ($zipFiles.Count -eq $expectedIds.Count) "Expected exactly $($expectedIds.Count) framework ZIPs, found $($zipFiles.Count)."
    foreach ($expectedId in $expectedIds) {
        $zipPath = Join-Path $artifactRootPath "$expectedId.zip"
        $hashPath = "$zipPath.sha256"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Missing framework ZIP: $zipPath"
        Assert-True (Test-Path -LiteralPath $hashPath -PathType Leaf) "Missing framework ZIP hash: $hashPath"
        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $manifestEntry = $archive.Entries | Where-Object { $_.FullName -eq "framework.manifest.json" } | Select-Object -First 1
            Assert-True ($null -ne $manifestEntry) "Framework ZIP lacks framework.manifest.json: $zipPath"
            $reader = [System.IO.StreamReader]::new($manifestEntry.Open())
            try {
                $zipManifest = $reader.ReadToEnd() | ConvertFrom-Json
            }
            finally {
                $reader.Dispose()
            }
            Assert-True ([string]$zipManifest.id -eq $expectedId) "Framework ZIP manifest id mismatch: $zipPath"
            $command = ([string]$zipManifest.entry.command).Replace('\', '/')
            $runtimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $command } | Select-Object -First 1
            Assert-True ($null -ne $runtimeEntry) "Framework ZIP entry.command is not staged: $zipPath -> $command"
        }
        finally {
            $archive.Dispose()
        }
        $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $expectedHash = ((Get-Content -Raw -Encoding UTF8 -LiteralPath $hashPath).Trim() -split '\s+')[0].ToLowerInvariant()
        Assert-True ($actualHash -eq $expectedHash) "Framework ZIP hash mismatch: $zipPath"
    }

    $summaryPath = Join-Path $artifactRootPath "summary.json"
    Assert-True (Test-Path -LiteralPath $summaryPath -PathType Leaf) "Framework package summary is required: $summaryPath"
    $summary = Get-Content -Raw -Encoding UTF8 -LiteralPath $summaryPath | ConvertFrom-Json
    Assert-True ([string]$summary.configuration -in @("Debug", "Release")) "Framework package summary configuration is invalid."
    Assert-True (@($summary.frameworks).Count -eq $expectedIds.Count) "Framework package summary entry count mismatch."
    foreach ($expectedId in $expectedIds) {
        $entry = @($summary.frameworks | Where-Object { [string]$_.id -eq $expectedId })
        Assert-True ($entry.Count -eq 1) "Framework package summary must contain one entry for $expectedId."
        Assert-True ([string]$entry[0].zip -eq "$expectedId.zip") "Framework package summary ZIP must be relative: $expectedId"
        Assert-True ([string]$entry[0].manifest -eq "framework-packages/$expectedId/framework.manifest.json") "Framework package summary manifest must be repository-relative: $expectedId"
        Assert-True ([string]$entry[0].protocolVersion -eq "loom.framework.v1") "Framework package summary protocol mismatch: $expectedId"
        Assert-True (-not [System.IO.Path]::IsPathRooted([string]$entry[0].zip)) "Framework package summary must not expose an absolute ZIP path: $expectedId"
        Assert-True (-not [System.IO.Path]::IsPathRooted([string]$entry[0].manifest)) "Framework package summary must not expose an absolute manifest path: $expectedId"
    }
}

Write-Host "Art framework package contract passed for $($manifestFiles.Count) manifests."
