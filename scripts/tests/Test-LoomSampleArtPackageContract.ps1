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
    "image-compress" = [ordered]@{ id = "custom-1770146354922"; framework = "cli_wrapper" }
    "remove-bg" = [ordered]@{ id = "custom-remove-bg-cloud"; framework = "cloud_api" }
    "image-search" = [ordered]@{ id = "custom-image-search"; framework = "mcp" }
    "color-transfer" = [ordered]@{ id = "custom-1770131241684"; framework = "python_art" }
    "image-blend" = [ordered]@{ id = "custom-image-blend-script"; framework = "script" }
    "image-blend-compress" = [ordered]@{ id = "custom-image-blend-compress-workflow"; framework = "workflow" }
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
    Assert-True ([string]$manifest.id -eq $entry.Value.id) "Sample Art id mismatch: $manifestPath"
    Assert-True ([string]$manifest.execution.framework -eq $entry.Value.framework) "Sample Art framework mismatch: $manifestPath"
    Assert-True ([string]$manifest.metadata.dependencies.framework -eq $entry.Value.framework) "Sample Art framework dependency mismatch: $manifestPath"
    Assert-True (($null -ne $manifest.inputs -and @($manifest.inputs).Count -gt 0) -or ($null -ne $manifest.params -and @($manifest.params).Count -gt 0)) "Sample Art inputs or params are required: $manifestPath"
    Assert-True ($null -ne $manifest.outputs -and @($manifest.outputs).Count -gt 0) "Sample Art outputs are required: $manifestPath"

    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath | ConvertFrom-Json
    Assert-True ([string]$runtime.protocolVersion -eq "loom.art.runtime.v1") "Sample Art runtime protocol is invalid: $runtimePath"
    Assert-True ($null -ne $runtime.entry) "Sample Art runtime entry is required: $runtimePath"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$runtime.entry.command)) "Sample Art runtime entry.command is required: $runtimePath"

    $runtimeCommand = ([string]$runtime.entry.command).Replace('/', '\')
    $runtimeCommandPath = Join-Path $sourceDirectory $runtimeCommand
    if ($runtimeCommand -match '\\|/') {
        Assert-True (Test-Path -LiteralPath $runtimeCommandPath -PathType Leaf) "Sample Art runtime entry is not bundled: $runtimeCommandPath"
    }
    else {
        $runtimeFile = @($runtime.entry.args | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^runtime[\\/]' } | Select-Object -First 1)
        Assert-True ($runtimeFile.Count -eq 1) "Sample Art runtime must reference a bundled runtime file: $runtimePath"
        Assert-True (Test-Path -LiteralPath (Join-Path $sourceDirectory ($runtimeFile[0] -replace '/', '\')) -PathType Leaf) "Sample Art runtime file is not bundled: $runtimeFile"
    }
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
    $expectedZipNames = @($expected.Values | ForEach-Object { "$($_.id).zip" })
    $zipFiles = @(Get-ChildItem -LiteralPath $artifactRootPath -Filter *.zip -File | Where-Object { $expectedZipNames -contains $_.Name })
    Assert-True ($zipFiles.Count -eq $expected.Count) "Expected all $($expected.Count) sample Art ZIPs, found $($zipFiles.Count)."

    foreach ($entry in $expected.GetEnumerator()) {
        $zipPath = Join-Path $artifactRootPath "$($entry.Value.id).zip"
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
            Assert-True ([string]$zipManifest.id -eq $entry.Value.id) "Sample Art ZIP id mismatch: $zipPath"
            Assert-True ([string]$zipManifest.metadata.dependencies.framework -eq $entry.Value.framework) "Sample Art ZIP framework dependency mismatch: $zipPath"

            $runtimeReader = [System.IO.StreamReader]::new($runtimeEntry.Open())
            try {
                $zipRuntime = $runtimeReader.ReadToEnd() | ConvertFrom-Json
            }
            finally {
                $runtimeReader.Dispose()
            }
            $command = ([string]$zipRuntime.entry.command).Replace('\', '/')
            if ($command -match '/') {
                $bundledRuntimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $command } | Select-Object -First 1
                Assert-True ($null -ne $bundledRuntimeEntry) "Sample Art ZIP runtime entry is not bundled: $zipPath -> $command"
            }
            else {
                $runtimeFile = @($zipRuntime.entry.args | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^runtime[\\/]' } | Select-Object -First 1)
                Assert-True ($runtimeFile.Count -eq 1) "Sample Art ZIP runtime must reference a bundled runtime file: $zipPath"
                $bundledRuntimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $runtimeFile[0].Replace('\', '/') } | Select-Object -First 1
                Assert-True ($null -ne $bundledRuntimeEntry) "Sample Art ZIP runtime file is not bundled: $zipPath -> $runtimeFile"
            }
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
