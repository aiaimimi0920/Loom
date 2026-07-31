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

function Assert-PathExists {
    param([string]$Path)
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "Missing required file: $Path"
}

function Assert-Contains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True $Haystack.Contains($Needle) $Message
}

function Assert-PowerShellParses {
    param([string]$Path)

    $tokens = $null
    $errors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $Path,
        [ref]$tokens,
        [ref]$errors
    )
    Assert-Equal 0 @($errors).Count "PowerShell parse errors in $Path."
}

function Get-ScriptFunctionDefinition {
    param(
        [System.Management.Automation.Language.ScriptBlockAst]$Ast,
        [string]$Name
    )

    $definition = $Ast.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq $Name
    }, $true)
    Assert-True ($null -ne $definition) "Missing script function: $Name"
    return [scriptblock]::Create($definition.Extent.Text)
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$resourceRoot = Join-Path $repoRoot "resources\workflow-arts\image-blend-compress"
$manifestPath = Join-Path $resourceRoot "manifest.json"
$workflowPath = Join-Path $resourceRoot "workflow.yaml"
$installerPath = Join-Path $repoRoot "scripts\Install-LoomImageBlendCompressWorkflowArt.ps1"
$smokePath = Join-Path $repoRoot "scripts\Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1"

Assert-PathExists $manifestPath
Assert-PathExists $workflowPath

$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
Assert-Equal "custom-image-blend-compress-workflow" ([string]$manifest.id) "Workflow Art id mismatch."
Assert-Equal "workflow" ([string]$manifest.execution.type) "Workflow Art execution type mismatch."
Assert-Equal "image-blend-compress-workflow" ([string]$manifest.execution.workflowId) "Workflow id mismatch."
Assert-Equal 2 @($manifest.inputs).Count "Workflow Art must expose two image inputs."
Assert-Equal "input" ([string]$manifest.inputs[0].name) "Primary input id mismatch."
Assert-Equal "reference" ([string]$manifest.inputs[1].name) "Reference input id mismatch."
Assert-Equal 2 @($manifest.params).Count "Workflow Art must expose two scalar params."

$mixRatio = @($manifest.params | Where-Object { [string]$_.id -eq "mix_ratio" })[0]
$quality = @($manifest.params | Where-Object { [string]$_.id -eq "quality_num" })[0]
Assert-Equal 50 ([int]$mixRatio.default) "Blend ratio default mismatch."
Assert-Equal 0 ([int]$mixRatio.min) "Blend ratio minimum mismatch."
Assert-Equal 100 ([int]$mixRatio.max) "Blend ratio maximum mismatch."
Assert-Equal 90 ([int]$quality.default) "Compression quality default mismatch."
Assert-Equal 60 ([int]$quality.min) "Compression quality minimum mismatch."
Assert-Equal 100 ([int]$quality.max) "Compression quality maximum mismatch."

$bindings = @{}
foreach ($binding in @($manifest.execution.workflowBindings.inputs)) {
    $key = [string]$binding.workflowParam
    $bindings[$key] = "{0}|{1}|{2}" -f $binding.nodeId, $binding.target, $binding.kind
}
Assert-Equal 4 $bindings.Count "Workflow binding count mismatch."
Assert-Equal "blend|input|input_image" $bindings["input"] "Primary image binding mismatch."
Assert-Equal "blend|reference|param" $bindings["reference"] "Reference image binding mismatch."
Assert-Equal "blend|mix_ratio|param" $bindings["mix_ratio"] "Blend ratio binding mismatch."
Assert-Equal "compress|quality_num|param" $bindings["quality_num"] "Compression quality binding mismatch."
Assert-Equal "compress" ([string]$manifest.execution.workflowBindings.primaryOutput.nodeId) "Primary output node mismatch."
Assert-Equal "output_base64" ([string]$manifest.execution.workflowBindings.primaryOutput.output) "Primary output field mismatch."
Assert-Equal "node_result" ([string]$manifest.execution.workflowBindings.primaryOutput.kind) "Primary output binding kind mismatch."

$dependencies = @($manifest.metadata.dependencies.arts | ForEach-Object { [string]$_ } | Sort-Object)
Assert-Equal "custom-1770146354922,custom-image-blend-script" ($dependencies -join ",") "Child Art dependencies mismatch."
Assert-Equal "workflow" ([string]$manifest.metadata.dependencies.framework) "Framework dependency mismatch."

$workflow = Get-Content -Raw -Encoding UTF8 -LiteralPath $workflowPath
Assert-Contains "uses: custom-image-blend-script" $workflow "Workflow must call the image blend Art."
Assert-Contains "uses: custom-1770146354922" $workflow "Workflow must call the image compression Art."
Assert-Contains "needs:" $workflow "Compression node must declare a dependency."
Assert-Contains '- blend' $workflow "Compression node must depend on blend."
Assert-Contains '${{ nodes.blend.outputs.output_base64 }}' $workflow "Compression input must reference the blend output."
Assert-Contains "level_num: 2" $workflow "Compression level must remain fixed at 2."
Assert-Contains "quality_num: 90" $workflow "Compression quality default must be 90."
Assert-Contains "lossless: false" $workflow "Workflow must use lossy compression."

Assert-PathExists $installerPath
Assert-PathExists $smokePath
Assert-PowerShellParses $installerPath
Assert-PowerShellParses $smokePath

$installer = Get-Content -Raw -Encoding UTF8 -LiteralPath $installerPath
Assert-Contains "custom-image-blend-compress-workflow" $installer "Installer must carry the stable Art id."
Assert-Contains "image-blend-compress-workflow" $installer "Installer must carry the stable workflow id."
Assert-Contains "custom-image-blend-script" $installer "Installer must validate the blend dependency."
Assert-Contains "custom-1770146354922" $installer "Installer must validate the compression dependency."
Assert-Contains "/v1/artloom-compat/arts/broadcast-updated" $installer "Installer must broadcast the Art update."
Assert-Contains '[System.Text.Encoding]::UTF8.GetBytes($body)' $installer "Workflow PUT must encode JSON as UTF-8 bytes."
Assert-Contains 'application/json; charset=utf-8' $installer "Workflow PUT must declare UTF-8 JSON."

$smoke = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokePath
Assert-Contains 'method = "art/process"' $smoke "Smoke must use the AHRP process route."
Assert-Contains 'reference = $reference' $smoke "Smoke must send the auxiliary reference image."
Assert-Contains 'quality_num = $Quality' $smoke "Smoke must send the compression quality."
Assert-Contains 'summary.json' $smoke "Smoke must persist JSON evidence."
Assert-Contains 'output.png' $smoke "Smoke must persist image evidence."

$tokens = $null
$parseErrors = $null
$smokeAst = [System.Management.Automation.Language.Parser]::ParseFile(
    $smokePath,
    [ref]$tokens,
    [ref]$parseErrors
)
. (Get-ScriptFunctionDefinition -Ast $smokeAst -Name "Get-ResponseError")
$errorMessage = Get-ResponseError -Response ([pscustomobject]@{
    status = "EngineError"
    error = "reference image is required"
})
Assert-Equal "reference image is required" $errorMessage "Smoke must preserve string AHRP errors under StrictMode."

Write-Output "Image blend compress workflow Art contract passed."
