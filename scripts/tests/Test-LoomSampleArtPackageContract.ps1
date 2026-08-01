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
$packagesRoot = Join-Path $repoRoot "art-packages\samples"
$buildScript = Join-Path $repoRoot "scripts\Build-LoomSampleArtPackages.ps1"
$expected = [ordered]@{
    "image-compress" = "cli_wrapper"
    "remove-bg" = "cloud_api"
    "image-search" = "mcp"
    "color-transfer" = "python_art"
    "image-blend" = "script"
    "image-blend-compress" = "workflow"
}

Assert-True (Test-Path -LiteralPath $packagesRoot -PathType Container) "Sample Art package source directory is required: $packagesRoot"
Assert-True (Test-Path -LiteralPath $buildScript -PathType Leaf) "Independent sample Art package build script is required: $buildScript"

$sourceDirectories = @(Get-ChildItem -LiteralPath $packagesRoot -Directory)
Assert-True ($sourceDirectories.Count -eq $expected.Count) "Expected exactly $($expected.Count) sample Art source directories, found $($sourceDirectories.Count)."

foreach ($entry in $expected.GetEnumerator()) {
    $sourceDirectory = Join-Path $packagesRoot $entry.Key
    Assert-True (Test-Path -LiteralPath $sourceDirectory -PathType Container) "Missing sample Art source directory: $sourceDirectory"

    $manifestPath = Join-Path $sourceDirectory "manifest.json"
    $runtimePath = Join-Path $sourceDirectory "art.runtime.json"
    Assert-True (Test-Path -LiteralPath $manifestPath -PathType Leaf) "Sample Art manifest is required: $manifestPath"
    Assert-True (Test-Path -LiteralPath $runtimePath -PathType Leaf) "Sample Art runtime manifest is required: $runtimePath"

    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$manifest.id)) "Sample Art id is required: $manifestPath"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$manifest.name)) "Sample Art name is required: $manifestPath"
    Assert-True ([bool]$manifest.enabled) "Sample Art must be enabled by default in its package manifest: $manifestPath"
    Assert-True ([string]$manifest.execution.type -eq "framework_art") "Sample Art must use framework_art execution: $manifestPath"
    Assert-True ([string]$manifest.execution.framework -eq $entry.Value) "Sample Art framework mismatch: $manifestPath"
    Assert-True ([string]$manifest.metadata.dependencies.framework -eq $entry.Value) "Sample Art framework dependency mismatch: $manifestPath"
    Assert-True ($null -ne $manifest.inputs -and @($manifest.inputs).Count -gt 0) "Sample Art inputs are required: $manifestPath"
    Assert-True ($null -ne $manifest.outputs -and @($manifest.outputs).Count -gt 0) "Sample Art outputs are required: $manifestPath"

    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath | ConvertFrom-Json
    Assert-True ([string]$runtime.protocolVersion -eq "loom.art.runtime.v1") "Sample Art runtime protocol is invalid: $runtimePath"
    Assert-True ($null -ne $runtime.entry) "Sample Art runtime entry is required: $runtimePath"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$runtime.entry.command)) "Sample Art runtime entry.command is required: $runtimePath"

    $runtimeCommand = ([string]$runtime.entry.command).Replace('/', '\')
    $runtimeCommandPath = Join-Path $sourceDirectory $runtimeCommand
    Assert-True (Test-Path -LiteralPath $runtimeCommandPath -PathType Leaf) "Sample Art runtime entry is not bundled: $runtimeCommandPath"
}

if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $artifactRootPath = if ([System.IO.Path]::IsPathRooted($ArtifactRoot)) {
        [System.IO.Path]::GetFullPath($ArtifactRoot)
    }
    else {
        [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
    }
    Assert-True (Test-Path -LiteralPath $artifactRootPath -PathType Container) "Sample Art artifact root is required: $artifactRootPath"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipFiles = @(Get-ChildItem -LiteralPath $artifactRootPath -Filter *.zip -File)
    Assert-True ($zipFiles.Count -eq $expected.Count) "Expected exactly $($expected.Count) sample Art ZIPs, found $($zipFiles.Count)."

    foreach ($entry in $expected.GetEnumerator()) {
        $zipPath = Join-Path $artifactRootPath "$($entry.Key).zip"
        $hashPath = "$zipPath.sha256"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Missing sample Art ZIP: $zipPath"
        Assert-True (Test-Path -LiteralPath $hashPath -PathType Leaf) "Missing sample Art ZIP hash: $hashPath"

        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            $manifestEntry = $archive.Entries | Where-Object { $_.FullName -eq "manifest.json" } | Select-Object -First 1
            $runtimeEntry = $archive.Entries | Where-Object { $_.FullName -eq "art.runtime.json" } | Select-Object -First 1
            Assert-True ($null -ne $manifestEntry) "Sample Art ZIP lacks manifest.json: $zipPath"
            Assert-True ($null -ne $runtimeEntry) "Sample Art ZIP lacks art.runtime.json: $zipPath"

            $reader = [System.IO.StreamReader]::new($manifestEntry.Open())
            try {
                $zipManifest = $reader.ReadToEnd() | ConvertFrom-Json
            }
            finally {
                $reader.Dispose()
            }
            Assert-True ([string]$zipManifest.execution.type -eq "framework_art") "Sample Art ZIP execution type is invalid: $zipPath"
            Assert-True ([string]$zipManifest.metadata.dependencies.framework -eq $entry.Value) "Sample Art ZIP framework dependency mismatch: $zipPath"

            $runtimeReader = [System.IO.StreamReader]::new($runtimeEntry.Open())
            try {
                $zipRuntime = $runtimeReader.ReadToEnd() | ConvertFrom-Json
            }
            finally {
                $runtimeReader.Dispose()
            }
            $command = ([string]$zipRuntime.entry.command).Replace('\', '/')
            $bundledRuntimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $command } | Select-Object -First 1
            Assert-True ($null -ne $bundledRuntimeEntry) "Sample Art ZIP runtime entry is not bundled: $zipPath -> $command"
        }
        finally {
            $archive.Dispose()
        }

        $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $expectedHash = ((Get-Content -Raw -Encoding UTF8 -LiteralPath $hashPath).Trim() -split '\s+')[0].ToLowerInvariant()
        Assert-True ($actualHash -eq $expectedHash) "Sample Art ZIP hash mismatch: $zipPath"
    }
}

Write-Host "Sample Art package contract passed for $($expected.Count) packages."
