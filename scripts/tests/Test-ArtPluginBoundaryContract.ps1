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

$releaseBuilder = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "scripts\build-release.ps1")
Assert-True (-not $releaseBuilder.Contains("Arts\Art_LoomEcho")) "Default release must not package the legacy Python sample Art."
$releaseVerifier = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "scripts\verify-release.ps1")
Assert-True ($releaseVerifier.Contains("runtime/python/Arts/")) "Release verification must reject packaged optional Python Arts."
$pluginSmoke = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "scripts\Invoke-LoomPluginBoundarySmoke.ps1")
foreach ($requiredText in @(
    "Build-ExternalFrameworkRuntime",
    "rustc.exe",
    "/upgrade",
    "frameworkLockRefreshRequired",
    "art_package_integrity_failed",
    "art_hook/instantiate",
    "art_loom/execute_art_node",
    "hookBridgeExecuted"
)) {
    Assert-True ($pluginSmoke.Contains($requiredText)) "Plugin boundary smoke is missing required proof: $requiredText"
}
Assert-True (-not $pluginSmoke.Contains("Copy-ZipEntry")) "Third-party smoke must not reuse a repo-owned framework runtime."
Assert-True (-not $pluginSmoke.Contains("FrameworkArtifactRoot")) "Third-party smoke must not depend on repo-owned framework artifacts."

$hookRoot = Resolve-Path (Join-Path $repoRoot "..\Hook")
$hookSource = Get-ChildItem -LiteralPath (Join-Path $hookRoot "src") -Recurse -File -Include *.ts,*.tsx |
    Where-Object { $_.FullName -notmatch "\\node_modules\\" }

$forbiddenArtIds = @(
    "custom-1770146354922",
    "custom-remove-bg-cloud",
    "custom-image-search",
    "custom-1770131241684"
)

foreach ($file in $hookSource) {
    $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $file.FullName
    foreach ($id in $forbiddenArtIds) {
        Assert-True (-not $text.Contains($id)) "Hook production source must not branch on sample Art id '$id' in $($file.FullName)."
    }
}

Write-Host "Art plugin boundary contract passed."
