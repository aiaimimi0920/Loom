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
$expectedIds = @(
    "cli_wrapper",
    "cloud_api",
    "script",
    "python_art",
    "mcp",
    "workflow"
)

Assert-True (Test-Path -LiteralPath $packagesRoot -PathType Container) "framework-packages directory is required."

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

Write-Host "Art framework package contract passed for $($manifestFiles.Count) manifests."
