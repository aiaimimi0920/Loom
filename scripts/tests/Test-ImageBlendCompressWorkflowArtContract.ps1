[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)
    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Contains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True $Haystack.Contains($Needle) $Message
}

function Assert-PowerShellParses {
    param([string]$Path)
    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$errors)
    Assert-Equal 0 @($errors).Count "PowerShell parse errors in $Path."
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$packageRoot = Join-Path $repoRoot "art-packages\samples\image-blend-compress"
$manifestPath = Join-Path $packageRoot "manifest.json"
$runtimeManifestPath = Join-Path $packageRoot "art.runtime.json"
$workflowPath = Join-Path $packageRoot "workflow.yaml"
$installerPath = Join-Path $repoRoot "scripts\Install-LoomImageBlendCompressWorkflowArt.ps1"
$genericInstallerPath = Join-Path $repoRoot "scripts\Install-LoomSampleArtPackage.ps1"
$runtimeSmokePath = Join-Path $repoRoot "scripts\tests\Test-LoomSampleArtRuntime.ps1"

foreach ($path in @($manifestPath, $runtimeManifestPath, $workflowPath, $installerPath, $genericInstallerPath, $runtimeSmokePath)) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Missing required workflow package file: $path"
}
Assert-PowerShellParses $installerPath
Assert-PowerShellParses $genericInstallerPath
Assert-PowerShellParses $runtimeSmokePath

$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
Assert-Equal "custom-image-blend-compress-workflow" ([string]$manifest.id) "Workflow Art id mismatch."
Assert-Equal "framework_art" ([string]$manifest.execution.type) "Workflow Art must use generic framework execution."
Assert-Equal "workflow" ([string]$manifest.execution.framework) "Workflow Art framework mismatch."
Assert-Equal "workflow" ([string]$manifest.metadata.dependencies.framework) "Workflow framework dependency mismatch."
Assert-Equal 2 @($manifest.inputs).Count "Workflow Art must expose two image inputs."
Assert-Equal 2 @($manifest.params).Count "Workflow Art must expose two scalar params."
Assert-Equal 2 @($manifest.metadata.dependencies.arts).Count "Workflow Art must declare two child Arts."

$mixRatio = @($manifest.params | Where-Object { [string]$_.id -eq "mix_ratio" })[0]
$quality = @($manifest.params | Where-Object { [string]$_.id -eq "quality_num" })[0]
Assert-Equal 50 ([int]$mixRatio.default) "Blend ratio default mismatch."
Assert-Equal 0 ([int]$mixRatio.min) "Blend ratio minimum mismatch."
Assert-Equal 100 ([int]$mixRatio.max) "Blend ratio maximum mismatch."
Assert-Equal 90 ([int]$quality.default) "Compression quality default mismatch."
Assert-Equal 60 ([int]$quality.min) "Compression quality minimum mismatch."
Assert-Equal 100 ([int]$quality.max) "Compression quality maximum mismatch."

$workflow = Get-Content -Raw -Encoding UTF8 -LiteralPath $workflowPath
Assert-Contains "uses: custom-image-blend-script" $workflow "Workflow must call the image blend Art."
Assert-Contains "uses: custom-1770146354922" $workflow "Workflow must call the image compression Art."
Assert-Contains "needs: [blend]" $workflow "Compression step must depend on blend."
Assert-Contains '${{ params.mix_ratio }}' $workflow "Workflow must expose the blend parameter."
Assert-Contains '${{ params.quality_num }}' $workflow "Workflow must expose the compression parameter."

$installer = Get-Content -Raw -Encoding UTF8 -LiteralPath $installerPath
Assert-Contains "Install-LoomSampleArtPackage.ps1" $installer "Workflow installer must delegate to the package installer."
Assert-Contains '"image-blend-compress"' $installer "Workflow installer must select the package source."
Assert-Contains "custom-image-blend-compress-workflow" $installer "Workflow installer must validate the stable Art id."

$smoke = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimeSmokePath
Assert-Contains "custom-image-blend-compress-workflow" $smoke "Runtime smoke must execute the workflow package."
Assert-Contains "PASS" $smoke "Runtime smoke must report package execution evidence."

Write-Output "Pluginized image blend compress workflow Art contract passed."
