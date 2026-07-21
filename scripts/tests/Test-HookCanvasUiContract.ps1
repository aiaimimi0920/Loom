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

Write-Host "Hook canvas UI contract passed."
