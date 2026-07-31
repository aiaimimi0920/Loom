<#
.SYNOPSIS
    Build and install Loom's image-blend-and-compress workflow Art.

.DESCRIPTION
    Packages a repository-owned workflow Art that executes the existing image
    blend Script Art and then the existing Pingo CLI wrapper Art. The installer
    persists both the Art definition and the saved workflow.
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-image-blend-compress-workflow",
    [string]$WorkflowId = "image-blend-compress-workflow",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "local",
    [switch]$SkipInstall,
    [switch]$SkipPublish
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-PathInsideRoot {
    param(
        [string]$Path,
        [string]$Root,
        [string]$Label
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $fullRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    )
    $prefix = $fullRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label must stay inside $fullRoot. Resolved path: $fullPath"
    }
}

function Read-JsonArrayFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return @()
    }
    $parsed = Get-Content -Raw -Encoding UTF8 -LiteralPath $Path | ConvertFrom-Json
    if ($null -eq $parsed) {
        return @()
    }
    return @($parsed)
}

function Write-JsonUtf8NoBom {
    param(
        [string]$Path,
        [object]$Value,
        [int]$Depth = 40
    )

    $parent = Split-Path -Parent $Path
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::WriteAllText(
        $Path,
        (($Value | ConvertTo-Json -Depth $Depth) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Test-DaemonReady {
    param([string]$Url)

    try {
        $status = Invoke-RestMethod -Uri ($Url.TrimEnd('/') + "/status") -Method Get -TimeoutSec 5
        return [string]$status.status -eq "ready"
    }
    catch {
        return $false
    }
}

function Save-WorkflowThroughDaemon {
    param(
        [string]$Url,
        [string]$Id,
        [string]$Yaml
    )

    $body = @{ data = $Yaml } | ConvertTo-Json -Depth 10
    $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
    return Invoke-RestMethod -Uri ($Url.TrimEnd('/') + "/v1/workflows/$Id") -Method Put -ContentType "application/json; charset=utf-8" -Body $bodyBytes -TimeoutSec 30
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $StoreRoot -or $StoreRoot.Trim().Length -eq 0) {
    $StoreRoot = Join-Path $repoRoot ".loom-art-store-data"
}
if (-not $ControlPlaneRoot -or $ControlPlaneRoot.Trim().Length -eq 0) {
    $ControlPlaneRoot = Join-Path $env:APPDATA "Loom\control-plane"
}
$ControlPlaneRoot = [System.IO.Path]::GetFullPath($ControlPlaneRoot)
$StoreRoot = [System.IO.Path]::GetFullPath($StoreRoot)

$resourceRoot = Join-Path $repoRoot "resources\workflow-arts\image-blend-compress"
$manifestSourcePath = Join-Path $resourceRoot "manifest.json"
$workflowSourcePath = Join-Path $resourceRoot "workflow.yaml"
if (-not (Test-Path -LiteralPath $manifestSourcePath -PathType Leaf)) {
    throw "Workflow Art manifest was not found: $manifestSourcePath"
}
if (-not (Test-Path -LiteralPath $workflowSourcePath -PathType Leaf)) {
    throw "Workflow YAML was not found: $workflowSourcePath"
}

$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestSourcePath | ConvertFrom-Json
if ([string]$manifest.execution.type -ne "workflow") {
    throw "Manifest must use workflow execution."
}
$manifest.id = $ArtId
$manifest.execution.workflowId = $WorkflowId
if ($null -ne $manifest.metadata -and $null -ne $manifest.metadata.artloomCompat) {
    $manifest.metadata.artloomCompat.execution.workflowId = $WorkflowId
}

$requiredChildArtIds = @(
    "custom-image-blend-script",
    "custom-1770146354922"
)
$manifest.metadata.dependencies.arts = $requiredChildArtIds
$workflowYaml = Get-Content -Raw -Encoding UTF8 -LiteralPath $workflowSourcePath

$workRoot = Join-Path $repoRoot "target\art-packages\image-blend-compress-workflow"
$stageRoot = Join-Path $workRoot "stage"
$stageWorkflowRoot = Join-Path $stageRoot "workflow"
$packagePath = Join-Path $workRoot "$ArtId.zip"
Assert-PathInsideRoot -Path $stageRoot -Root $workRoot -Label "Stage root"

Remove-Item -Recurse -Force -LiteralPath $stageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stageWorkflowRoot | Out-Null
Write-JsonUtf8NoBom -Path (Join-Path $stageRoot "manifest.json") -Value $manifest
[System.IO.File]::WriteAllText(
    (Join-Path $stageWorkflowRoot "workflow.yaml"),
    $workflowYaml,
    [System.Text.UTF8Encoding]::new($false)
)

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path -LiteralPath $packagePath -PathType Leaf) {
    Remove-Item -LiteralPath $packagePath -Force
}
[System.IO.Compression.ZipFile]::CreateFromDirectory($stageRoot, $packagePath)

$publishedZipPath = $null
if (-not $SkipPublish) {
    $artsRoot = Join-Path $StoreRoot "arts"
    New-Item -ItemType Directory -Force -Path $artsRoot | Out-Null
    $publishedZipPath = Join-Path $artsRoot "$ArtId.zip"
    Copy-Item -LiteralPath $packagePath -Destination $publishedZipPath -Force
}

$installReport = $null
if (-not $SkipInstall) {
    $frameworksPath = Join-Path $ControlPlaneRoot "frameworks.json"
    $installedFrameworks = if (Test-Path -LiteralPath $frameworksPath -PathType Leaf) {
        @(Read-JsonArrayFile -Path $frameworksPath | ForEach-Object { [string]$_ })
    }
    else {
        @("cli_wrapper", "cloud_api", "script", "workflow")
    }
    if ($installedFrameworks -notcontains "workflow") {
        throw "Required Art framework is not installed: workflow"
    }

    $toolsPath = Join-Path $ControlPlaneRoot "tools\tools.json"
    $tools = @(Read-JsonArrayFile -Path $toolsPath)
    $installedToolIds = @($tools | ForEach-Object { [string]$_.id })
    foreach ($requiredChildArtId in $requiredChildArtIds) {
        if ($installedToolIds -notcontains $requiredChildArtId) {
            throw "Required child Art is not installed: $requiredChildArtId"
        }
    }

    if ($InstallMode -eq "local") {
        $artDir = Join-Path $ControlPlaneRoot "arts\$ArtId"
        $workflowsDir = Join-Path $ControlPlaneRoot "workflows"
        $installedWorkflowPath = Join-Path $workflowsDir "$WorkflowId.yaml"
        Assert-PathInsideRoot -Path $artDir -Root $ControlPlaneRoot -Label "Art install directory"
        Assert-PathInsideRoot -Path $installedWorkflowPath -Root $ControlPlaneRoot -Label "Workflow install path"

        Remove-Item -Recurse -Force -LiteralPath $artDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $artDir, $workflowsDir, (Split-Path -Parent $toolsPath) | Out-Null
        Get-ChildItem -LiteralPath $stageRoot -Force | Copy-Item -Destination $artDir -Recurse -Force
        [System.IO.File]::WriteAllText(
            $installedWorkflowPath,
            $workflowYaml,
            [System.Text.UTF8Encoding]::new($false)
        )

        $remainingTools = @($tools | Where-Object { [string]$_.id -ne $ArtId })
        $nextTools = @($remainingTools + $manifest) | Sort-Object { [string]$_.id }
        Write-JsonUtf8NoBom -Path $toolsPath -Value $nextTools

        $workflowApiSaved = $false
        if (Test-DaemonReady -Url $BaseUrl) {
            $null = Save-WorkflowThroughDaemon -Url $BaseUrl -Id $WorkflowId -Yaml $workflowYaml
            $workflowApiSaved = $true
        }
        $installReport = [ordered]@{
            mode = "local"
            artDir = $artDir
            workflowPath = $installedWorkflowPath
            toolsPath = $toolsPath
            workflowApiSaved = $workflowApiSaved
        }
    }
    elseif ($InstallMode -eq "store") {
        if ($SkipPublish) {
            throw "InstallMode=store requires publishing. Remove -SkipPublish or use upload."
        }
        $null = Invoke-RestMethod -Uri ($StoreUrl.TrimEnd('/') + "/health") -Method Get -TimeoutSec 5
        $body = @{ artId = $ArtId; store = $StoreUrl } | ConvertTo-Json -Depth 5
        $artInstall = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/store/install") -Method Post -ContentType "application/json" -Body $body -TimeoutSec 120
        $workflowInstall = Save-WorkflowThroughDaemon -Url $BaseUrl -Id $WorkflowId -Yaml $workflowYaml
        $installReport = [ordered]@{
            mode = "store"
            art = $artInstall
            workflow = $workflowInstall
        }
    }
    else {
        $zipBytes = [System.IO.File]::ReadAllBytes($packagePath)
        $zipBase64 = "data:application/zip;base64," + [Convert]::ToBase64String($zipBytes)
        $body = @{ zipBase64 = $zipBase64 } | ConvertTo-Json -Depth 5
        $artInstall = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/install") -Method Post -ContentType "application/json" -Body $body -TimeoutSec 120
        $workflowInstall = Save-WorkflowThroughDaemon -Url $BaseUrl -Id $WorkflowId -Yaml $workflowYaml
        $installReport = [ordered]@{
            mode = "upload"
            art = $artInstall
            workflow = $workflowInstall
        }
    }

    try {
        $null = Invoke-RestMethod -Uri ($BaseUrl.TrimEnd('/') + "/v1/artloom-compat/arts/broadcast-updated") -Method Post -ContentType "application/json" -Body "{}" -TimeoutSec 15
    }
    catch {
    }
}

[ordered]@{
    artId = $ArtId
    workflowId = $WorkflowId
    packagePath = $packagePath
    publishedZipPath = $publishedZipPath
    controlPlaneRoot = $ControlPlaneRoot
    installMode = $InstallMode
    installReport = $installReport
} | ConvertTo-Json -Depth 30
