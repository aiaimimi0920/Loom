<#
.SYNOPSIS
    Build and install Loom's script-based image-blend art.

.DESCRIPTION
    Packages a repo-owned `script` Art that blends an input image and a
    reference image by ratio:
      - stages the PowerShell script under a formal Loom art package
      - optionally publishes that zip into the local Loom art-store root
      - optionally installs the art into the local Loom control-plane

    This is intended as a real-world acceptance proof for Loom's `script`
    framework, beyond the existing fixture tests.

.PARAMETER BaseUrl
    Loom daemon base URL used for optional upload/store install and best-effort
    art-update broadcast. Default http://127.0.0.1:8765.

.PARAMETER ArtId
    Tool/art id to install. Defaults to custom-image-blend-script.

.PARAMETER ArtName
    Display name. Defaults to the existing Chinese image-blend label.

.PARAMETER StoreRoot
    Local art-store root used when publishing the generated zip. Defaults to
    <repo>/.loom-art-store-data.

.PARAMETER StoreUrl
    Local art-store URL used when installing through the store route. Default
    http://127.0.0.1:8790.

.PARAMETER ControlPlaneRoot
    Loom control-plane root used for local installation mode. Defaults to
    %APPDATA%\Loom\control-plane.

.PARAMETER InstallMode
    Install strategy:
      - local  : lay the art into <control-plane>/arts/<id>/ and update
                 tools.json directly
      - store  : publish the zip to StoreRoot, then ask Loom to install it from
                 the local art store URL
      - upload : upload the full zip payload directly to /v1/arts/install
    Default local.

.PARAMETER SkipInstall
    Only build/publish the package; do not install it.

.PARAMETER SkipPublish
    Do not copy the generated zip into the local Loom art-store root.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/Install-LoomImageBlendScriptArt.ps1
#>
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-image-blend-script",
    [string]$ArtName = "__AUTO_ART_NAME__",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "local",
    [switch]$SkipInstall,
    [switch]$SkipPublish
)

$ErrorActionPreference = "Stop"

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$imageBlendLabel = ConvertFrom-UnicodeCodePoints @(0x56FE, 0x7247, 0x6DF7, 0x5408)
$imageBlendDescription = ConvertFrom-UnicodeCodePoints @(
    0x4F7F, 0x7528, 0x0020, 0x0050, 0x006F, 0x0077, 0x0065, 0x0072, 0x0053, 0x0068, 0x0065, 0x006C, 0x006C, 0x0020,
    0x811A, 0x672C, 0x6309, 0x6BD4, 0x4F8B, 0x6DF7, 0x5408, 0x6E90, 0x56FE, 0x4E0E, 0x53C2, 0x8003, 0x56FE
)
$referenceLabel = ConvertFrom-UnicodeCodePoints @(0x53C2, 0x8003, 0x56FE)
$mixRatioLabel = ConvertFrom-UnicodeCodePoints @(0x6DF7, 0x5408, 0x6BD4, 0x4F8B)
$resultLabel = ConvertFrom-UnicodeCodePoints @(0x7ED3, 0x679C)
$sourceLabel = ConvertFrom-UnicodeCodePoints @(0x6E90, 0x56FE)

if ($ArtName -eq "__AUTO_ART_NAME__") {
    $ArtName = $imageBlendLabel
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $StoreRoot -or $StoreRoot.Trim().Length -eq 0) {
    $StoreRoot = Join-Path $repoRoot ".loom-art-store-data"
}
if (-not $ControlPlaneRoot -or $ControlPlaneRoot.Trim().Length -eq 0) {
    $ControlPlaneRoot = Join-Path $env:APPDATA "Loom\control-plane"
}

$sourceScriptPath = Join-Path $repoRoot "resources\script-arts\image-blend\main.ps1"
if (-not (Test-Path -LiteralPath $sourceScriptPath -PathType Leaf)) {
    throw "Image blend script source was not found: $sourceScriptPath"
}

$workRoot = Join-Path $repoRoot "target\art-packages\image-blend-script"
$stageRoot = Join-Path $workRoot "stage"
$scriptStageRoot = Join-Path $stageRoot "script"
$packagePath = Join-Path $workRoot "$ArtId.zip"

Remove-Item -Recurse -Force -LiteralPath $stageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $workRoot, $scriptStageRoot | Out-Null
Copy-Item -LiteralPath $sourceScriptPath -Destination (Join-Path $scriptStageRoot "main.ps1") -Force

$manifest = [ordered]@{
    id = $ArtId
    name = $ArtName
    description = $imageBlendDescription
    enabled = $true
    execution = [ordered]@{
        type = "script"
        path = "script/main.ps1"
    }
    inputs = @(
        [ordered]@{
            name = "input"
            label = $sourceLabel
            type = "image"
            execution_type = "image_buffer"
        }
        [ordered]@{
            name = "reference"
            label = $referenceLabel
            type = "image"
            execution_type = "image_buffer"
            exposePort = $true
        }
    )
    outputs = @(
        [ordered]@{
            name = "output"
            label = $resultLabel
            type = "image"
            execution_type = "image_buffer"
        }
    )
    params = @(
        [ordered]@{
            id = "reference"
            label = $referenceLabel
            widget = "image_link"
            default = ""
            disabled = $false
            data_type = "image_path"
        }
        [ordered]@{
            id = "mix_ratio"
            label = $mixRatioLabel
            widget = "slider"
            default = 50
            min = 0
            max = 100
            step = 1
            disabled = $false
            data_type = "number"
        }
    )
    metadata = [ordered]@{
        dependencies = [ordered]@{
            framework = "script"
        }
        artloomCompat = [ordered]@{
            defaults = [ordered]@{}
            executionType = "script"
            icon = "#13c2c2"
            source = "loom-local"
            execution = [ordered]@{
                path = "script/main.ps1"
                outputs = @(
                    [ordered]@{
                        name = "output"
                        label = $resultLabel
                        type = "image"
                        execution_type = "image_buffer"
                    }
                )
                sourceType = "installed"
            }
        }
    }
}

$manifestPath = Join-Path $stageRoot "manifest.json"
[System.IO.File]::WriteAllText(
    $manifestPath,
    ($manifest | ConvertTo-Json -Depth 30),
    [System.Text.UTF8Encoding]::new($false)
)

if (Test-Path -LiteralPath $packagePath -PathType Leaf) {
    Remove-Item -LiteralPath $packagePath -Force
}

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
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
    if ($InstallMode -eq "local") {
        $artDir = Join-Path $ControlPlaneRoot ("arts\" + $ArtId)
        $toolsDir = Join-Path $ControlPlaneRoot "tools"
        $toolsPath = Join-Path $toolsDir "tools.json"

        Remove-Item -Recurse -Force -LiteralPath $artDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force -Path $artDir, $toolsDir | Out-Null
        Get-ChildItem -LiteralPath $stageRoot -Force | Copy-Item -Destination $artDir -Recurse -Force

        $tool = $manifest | ConvertTo-Json -Depth 30 | ConvertFrom-Json
        $tool.execution.path = (Join-Path $artDir "script\main.ps1")
        if ($null -ne $tool.metadata -and $null -ne $tool.metadata.artloomCompat -and $null -ne $tool.metadata.artloomCompat.execution) {
            $tool.metadata.artloomCompat.execution.path = $tool.execution.path
        }

        $tools = @()
        if (Test-Path -LiteralPath $toolsPath -PathType Leaf) {
            $parsed = Get-Content -LiteralPath $toolsPath -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($parsed -is [System.Array]) {
                $tools = @($parsed)
            }
            elseif ($null -ne $parsed) {
                $tools = @($parsed)
            }
        }

        $remaining = @($tools | Where-Object { [string]$_.id -ne $ArtId })
        $nextTools = @($remaining + $tool) | Sort-Object { [string]$_.id }
        [System.IO.File]::WriteAllText(
            $toolsPath,
            (($nextTools | ConvertTo-Json -Depth 40) + [Environment]::NewLine),
            [System.Text.UTF8Encoding]::new($false)
        )

        try {
            $null = Invoke-RestMethod `
                -Uri ($BaseUrl.TrimEnd('/') + "/v1/artloom-compat/arts/broadcast-updated") `
                -Method Post `
                -ContentType "application/json" `
                -Body "{}" `
                -TimeoutSec 15
        }
        catch {
        }

        $installReport = [ordered]@{
            mode = "local"
            artDir = $artDir
            toolsPath = $toolsPath
            scriptPath = $tool.execution.path
        }
    }
    elseif ($InstallMode -eq "store") {
        if ($SkipPublish) {
            throw "InstallMode=store requires the package to be published into StoreRoot. Remove -SkipPublish or switch to -InstallMode upload."
        }
        try {
            $null = Invoke-RestMethod -Uri ($StoreUrl.TrimEnd('/') + "/health") -Method Get -TimeoutSec 5
        }
        catch {
            throw "InstallMode=store requires a running Loom art store at $StoreUrl. Start it first, for example with scripts/run-art-store.ps1."
        }
        $installBody = @{ artId = $ArtId; store = $StoreUrl } | ConvertTo-Json -Depth 5
        $installReport = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/store/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body $installBody `
            -TimeoutSec 120
    }
    else {
        $zipBytes = [System.IO.File]::ReadAllBytes($packagePath)
        $zipBase64 = "data:application/zip;base64," + [Convert]::ToBase64String($zipBytes)
        $installBody = @{ zipBase64 = $zipBase64 } | ConvertTo-Json -Depth 5
        $installReport = Invoke-RestMethod `
            -Uri ($BaseUrl.TrimEnd('/') + "/v1/arts/install") `
            -Method Post `
            -ContentType "application/json" `
            -Body $installBody `
            -TimeoutSec 120
    }
}

[ordered]@{
    artId = $ArtId
    artName = $ArtName
    scriptSourcePath = $sourceScriptPath
    packagePath = $packagePath
    publishedZipPath = $publishedZipPath
    controlPlaneRoot = $ControlPlaneRoot
    installMode = $InstallMode
    installReport = $installReport
} | ConvertTo-Json -Depth 20
