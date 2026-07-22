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

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
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
$visualWorkflowLabel = ConvertFrom-UnicodeCodePoints @(0x6253, 0x5F00, 0x53EF, 0x89C6, 0x5316, 0x5DE5, 0x4F5C, 0x6D41)
$saveWorkflowLabel = ConvertFrom-UnicodeCodePoints @(0x4FDD, 0x5B58, 0x5DE5, 0x4F5C, 0x6D41)
$openWorkflowLabel = ConvertFrom-UnicodeCodePoints @(0x6253, 0x5F00, 0x5DE5, 0x4F5C, 0x6D41)
$yamlStorageLabel = "YAML " + (ConvertFrom-UnicodeCodePoints @(0x5B58, 0x50A8))
$loadYamlLabel = ">" + (ConvertFrom-UnicodeCodePoints @(0x52A0, 0x8F7D)) + " YAML<"

Assert-Contains 'data-testid=' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'nav-hook-bridge' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'data-testid="hook-canvas-thumbnail"' $thumbnail "Screenshot Sync must render a real Hook canvas thumbnail."
Assert-Contains 'data-testid="hook-canvas-node"' $node "Hook canvas nodes need stable smoke targets."
Assert-Contains 'data-testid="hook-canvas-view"' $view "Hook workflow must render a full visual canvas."
Assert-Contains $visualWorkflowLabel $thumbnail "Thumbnail must expose the visual workflow entry."
Assert-Contains 'data-testid="advanced-technical-information"' $app "Technical workflow formats must be in an explicit disclosure."
Assert-Contains $saveWorkflowLabel $app "Normal save action must not require YAML wording."
Assert-Contains $openWorkflowLabel $app "Normal load action must not require YAML wording."
Assert-NotContains ("eyebrow: `"$yamlStorageLabel`"") $app "Navigation must not advertise YAML to normal users."
Assert-NotContains $loadYamlLabel $app "Saved workflow action must use visual language."
Assert-Contains 'const nextSnapshot = await waitForLoomOnline(refreshSnapshot);' $app "Daemon start must always poll the complete snapshot until online."
Assert-NotContains '? await waitForLoomOnline' $app "Daemon readiness polling must not depend on whether a new process was spawned."

Assert-PathExists $smokePath
Assert-PathExists $inspectorPath
$smoke = Get-Content -Raw -Encoding UTF8 $smokePath
$inspector = Get-Content -Raw -Encoding UTF8 $inspectorPath
Assert-Contains 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' $smoke "Smoke must use an isolated CDP port."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $smoke "Smoke must isolate Loom data."
Assert-Contains 'APPDATA' $smoke "Smoke must isolate the Hook session."
Assert-Contains 'ExpectedExecutablePath' $smoke "Smoke cleanup must validate exact process paths."
Assert-Contains 'SmokePortMinimum = 30000' $smoke "Smoke listeners must stay below the Windows dynamic client-port range."
Assert-Contains 'SmokePortMaximum = 45000' $smoke "Smoke listeners must stay below the Windows dynamic client-port range."
Assert-Contains 'Wait-ForHookCanvasUi' $smoke "Smoke must wait on Hook canvas DOM conditions."
Assert-NotContains 'Start-Sleep -Seconds 2' $smoke "Smoke must not use a fixed Hook canvas refresh delay."
Assert-Contains 'min-nodes' $inspector "Inspector must wait for the expected Hook canvas node count."
Assert-Contains 'data-revision' $inspector "Inspector must wait for a non-empty Hook canvas revision."

Write-Host "Hook canvas UI contract passed."
