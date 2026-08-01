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

$frameworkRs = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "crates\loom_tool_registry\src\framework.rs")
Assert-True (-not $frameworkRs.Contains("BUILT_IN_FRAMEWORKS")) "Optional Art frameworks must not be installed by default."
Assert-True ($frameworkRs.Contains("framework.manifest.json")) "Framework package manifest support must be present."

$readme = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "README.md")
Assert-True (-not $readme.Contains("installed by default")) "README must describe explicit framework installation only."

$hookRoot = Resolve-Path (Join-Path $repoRoot "..\Hook")
$hookSource = Get-ChildItem -LiteralPath (Join-Path $hookRoot "src") -Recurse -File -Include *.ts,*.tsx |
    Where-Object { $_.FullName -notmatch "\\node_modules\\" }

$forbiddenArtIds = @(
    "custom-1770146354922",
    "custom-image-search",
    "custom-1770131241684",
    "custom-image-blend-script",
    "custom-image-blend-compress-workflow"
)

foreach ($file in $hookSource) {
    $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $file.FullName
    foreach ($id in $forbiddenArtIds) {
        Assert-True (-not $text.Contains($id)) "Hook production source must not branch on sample Art id '$id' in $($file.FullName)."
    }
}

Write-Host "Art plugin boundary contract passed."
