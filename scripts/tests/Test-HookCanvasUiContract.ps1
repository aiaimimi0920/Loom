$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-PathExists {
    param([string]$Path)
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "Missing required file: $Path"
}

function Assert-Contains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True $Haystack.Contains($Needle) $Message
}

function Assert-NotContains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True (-not $Haystack.Contains($Needle)) $Message
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$appPath = Join-Path $repoRoot "apps\desktop\src\App.tsx"
$thumbnailPath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasThumbnail.tsx"
$viewPath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasView.tsx"
$nodePath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasNode.tsx"
$smokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookCanvasUiSmoke.ps1"
$inspectorPath = Join-Path $repoRoot "scripts\Inspect-LoomWebView.mjs"

Assert-PathExists $thumbnailPath
Assert-PathExists $viewPath
Assert-PathExists $nodePath

$app = Get-Content -Raw -Encoding UTF8 $appPath
$thumbnail = Get-Content -Raw -Encoding UTF8 $thumbnailPath
$view = Get-Content -Raw -Encoding UTF8 $viewPath
$node = Get-Content -Raw -Encoding UTF8 $nodePath

Assert-Contains 'data-testid=' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'nav-hook-bridge' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'data-testid="hook-canvas-thumbnail"' $thumbnail "Screenshot Sync must render a real Hook canvas thumbnail."
Assert-Contains 'data-testid="hook-canvas-node"' $node "Hook canvas nodes need stable smoke targets."
Assert-Contains 'data-testid="hook-canvas-view"' $view "Hook workflow must render a full visual canvas."
Assert-Contains '打开可视化工作流' $thumbnail "Thumbnail must expose the visual workflow entry."
Assert-Contains 'data-testid="advanced-technical-information"' $app "Technical workflow formats must be in an explicit disclosure."
Assert-Contains '保存工作流' $app "Normal save action must not require YAML wording."
Assert-Contains '打开工作流' $app "Normal load action must not require YAML wording."
Assert-NotContains 'eyebrow: "YAML 存储"' $app "Navigation must not advertise YAML to normal users."
Assert-NotContains '>加载 YAML<' $app "Saved workflow action must use visual language."

Assert-PathExists $smokePath
Assert-PathExists $inspectorPath
$smoke = Get-Content -Raw -Encoding UTF8 $smokePath
Assert-Contains 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' $smoke "Smoke must use an isolated CDP port."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $smoke "Smoke must isolate Loom data."
Assert-Contains 'APPDATA' $smoke "Smoke must isolate the Hook session."
Assert-Contains 'ExpectedExecutablePath' $smoke "Smoke cleanup must validate exact process paths."

Write-Host "Hook canvas UI contract passed."
