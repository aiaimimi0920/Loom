[CmdletBinding()]
param(
    [string]$OutputRoot = ".loom-art-store-data\arts",
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Assert-PathInside {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\') + '\'
    $fullRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($fullRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label must remain inside ${Root}: $Path"
    }
}

function Copy-DirectoryContents {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    foreach ($entry in Get-ChildItem -LiteralPath $Source -Force) {
        if ($entry.Name -eq "__pycache__" -or $entry.Extension -eq ".pyc") {
            continue
        }
        if ($entry.PSIsContainer) {
            Copy-DirectoryContents -Source $entry.FullName -Destination (Join-Path $Destination $entry.Name)
        }
        else {
            Copy-Item -LiteralPath $entry.FullName -Destination $Destination -Force
        }
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$sourceRoot = Join-Path $repoRoot "art-packages\samples"
$sharedRuntime = Join-Path $repoRoot "art-packages\shared\image-runtime-common.ps1"
$outputRootPath = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
    [System.IO.Path]::GetFullPath($OutputRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputRoot))
}
$stagingRoot = Join-Path $outputRootPath ".staging"
$appDataRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)
if (-not [string]::IsNullOrWhiteSpace($appDataRoot)) {
    $normalizedOutput = $outputRootPath.TrimEnd('\') + '\'
    $normalizedAppData = ([System.IO.Path]::GetFullPath($appDataRoot)).TrimEnd('\') + '\'
    if ($normalizedOutput.StartsWith($normalizedAppData, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Sample Art package output must not be inside APPDATA: $outputRootPath"
    }
}

$packageNames = @(
    "image-compress",
    "remove-bg",
    "image-search",
    "color-transfer",
    "image-blend",
    "image-blend-compress"
)

if (-not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
    throw "Sample Art package source root not found: $sourceRoot"
}
if (-not (Test-Path -LiteralPath $sharedRuntime -PathType Leaf)) {
    throw "Shared Art image runtime helper not found: $sharedRuntime"
}

New-Item -ItemType Directory -Force -Path $outputRootPath | Out-Null
Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "staging root"
if (Test-Path -LiteralPath $stagingRoot) {
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null

$summary = @()
$officialCertifications = [ordered]@{}
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem

    foreach ($packageName in $packageNames) {
        $sourceDirectory = Join-Path $sourceRoot $packageName
        $manifestPath = Join-Path $sourceDirectory "manifest.json"
        $runtimeManifestPath = Join-Path $sourceDirectory "art.runtime.json"
        $workflowPath = Join-Path $sourceDirectory "workflow.yaml"
        if (-not (Test-Path -LiteralPath $sourceDirectory -PathType Container)) {
            throw "Sample Art source directory not found: $sourceDirectory"
        }
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            throw "Sample Art manifest not found: $manifestPath"
        }
        $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
        $artId = [string]$manifest.id
        $framework = [string]$manifest.metadata.dependencies.framework
        $executionType = [string]$manifest.execution.type
        if ([string]::IsNullOrWhiteSpace($artId)) {
            throw "Sample Art manifest id is empty: $manifestPath"
        }
        if ($executionType -eq "framework_art") {
            if (-not (Test-Path -LiteralPath $runtimeManifestPath -PathType Leaf)) {
                throw "Sample Art runtime manifest not found: $runtimeManifestPath"
            }
            $runtimeManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimeManifestPath | ConvertFrom-Json
            if ([string]$manifest.execution.framework -ne $framework) {
                throw "Sample Art execution and dependency framework differ: $manifestPath"
            }
            if ([string]$runtimeManifest.protocolVersion -ne "loom.art.runtime.v1") {
                throw "Sample Art runtime protocol is invalid: $runtimeManifestPath"
            }
        }
        elseif ($executionType -eq "workflow") {
            if ($framework -ne "workflow") {
                throw "Workflow sample Art must depend on the workflow framework: $manifestPath"
            }
            if (-not (Test-Path -LiteralPath $workflowPath -PathType Leaf)) {
                throw "Workflow sample Art definition not found: $workflowPath"
            }
        }
        else {
            throw "Unsupported sample Art execution type '$executionType': $manifestPath"
        }

        $stageDirectory = Join-Path $stagingRoot $packageName
        Assert-PathInside -Path $stageDirectory -Root $stagingRoot -Label "Art stage directory"
        New-Item -ItemType Directory -Force -Path $stageDirectory | Out-Null
        Copy-DirectoryContents -Source $sourceDirectory -Destination $stageDirectory
        if ($executionType -eq "framework_art") {
            Copy-Item -LiteralPath $sharedRuntime -Destination (Join-Path $stageDirectory "runtime\common.ps1") -Force
        }

        $stageManifestPath = Join-Path $stageDirectory "manifest.json"
        $stageManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $stageManifestPath | ConvertFrom-Json
        if ([string]$stageManifest.metadata.dependencies.framework -ne $framework) {
            throw "Staged Art framework dependency changed unexpectedly: $stageManifestPath"
        }

        $zipPath = Join-Path $outputRootPath "$artId.zip"
        $hashPath = "$zipPath.sha256"
        Assert-PathInside -Path $zipPath -Root $outputRootPath -Label "Art ZIP"
        if (Test-Path -LiteralPath $zipPath) {
            Remove-Item -LiteralPath $zipPath -Force
        }
        if (Test-Path -LiteralPath $hashPath) {
            Remove-Item -LiteralPath $hashPath -Force
        }
        Compress-Archive -Path (Join-Path $stageDirectory "*") -DestinationPath $zipPath -CompressionLevel Optimal -Force
        $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8NoBomFile -Path $hashPath -Content "$hash  $artId.zip`n"

        $version = [string]$manifest.metadata.packageSecurity.version
        if ([string]::IsNullOrWhiteSpace($version)) {
            throw "Sample Art package version is empty: $manifestPath"
        }
        $publisherId = ""
        $publisherProperty = $manifest.metadata.packageSecurity.PSObject.Properties["publisher"]
        if ($null -ne $publisherProperty -and $null -ne $publisherProperty.Value) {
            $publisherId = [string]$publisherProperty.Value.id
        }
        $qualifiedId = if ([string]::IsNullOrWhiteSpace($publisherId)) {
            $artId
        }
        else {
            "$publisherId/$artId"
        }
        if (-not $officialCertifications.Contains($qualifiedId)) {
            $officialCertifications[$qualifiedId] = [ordered]@{}
        }
        $officialCertifications[$qualifiedId][$version] = $hash

        $summary += [ordered]@{
            source = $packageName
            id = $artId
            framework = $framework
            manifest = "art-packages/samples/$packageName/manifest.json"
            zip = "$artId.zip"
            bytes = (Get-Item -LiteralPath $zipPath).Length
            sha256 = $hash
            official = $true
        }
    }
}
finally {
    if (Test-Path -LiteralPath $stagingRoot) {
        Assert-PathInside -Path $stagingRoot -Root $outputRootPath -Label "staging cleanup"
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
}

$summaryPath = Join-Path $outputRootPath "summary.json"
Write-Utf8NoBomFile -Path $summaryPath -Content (([ordered]@{
    configuration = $Configuration
    packages = $summary
} | ConvertTo-Json -Depth 30) + "`n")

if ((Split-Path -Leaf $outputRootPath) -ieq "arts") {
    $certificationPath = Join-Path (Split-Path -Parent $outputRootPath) "official-art-certifications.json"
    Write-Utf8NoBomFile -Path $certificationPath -Content (([ordered]@{
        schemaVersion = 1
        certifications = $officialCertifications
    } | ConvertTo-Json -Depth 30) + "`n")
}

Write-Host "Built $($summary.Count) independent sample Art packages under $outputRootPath"
