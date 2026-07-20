[CmdletBinding()]
param(
    [string]$RepoRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
}

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Contains {
    param(
        [string]$Needle,
        [string]$Haystack,
        [string]$Message
    )

    Assert-True -Condition $Haystack.Contains($Needle) -Message $Message
}

$requiredPaths = @(
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "README.md",
    ".gitignore",
    "LICENSE",
    "CONTRIBUTING.md",
    "SOURCE_PROVENANCE.md",
    "apps\daemon",
    "apps\daemon\tests\fixtures\local-capability\loom-manifest.json",
    "apps\daemon\tests\fixtures\local-capability\loom-invoke-request.json",
    "apps\daemon\tests\fixtures\tea-brain-provider\decompose-request.example.json",
    "apps\cli",
    "apps\desktop",
    "crates",
    "docs\progress\MASTER.md",
    "examples",
    "resources",
    "scripts"
)

foreach ($relativePath in $requiredPaths) {
    $path = Join-Path $RepoRoot $relativePath
    Assert-True -Condition (Test-Path -LiteralPath $path) -Message "Missing standalone Loom path: $relativePath"
}

$forbiddenGeneratedPaths = @(
    [ordered]@{ path = "target"; ignore = "/target/" },
    [ordered]@{ path = "apps\desktop\node_modules"; ignore = "/apps/desktop/node_modules/" },
    [ordered]@{ path = "apps\desktop\dist"; ignore = "/apps/desktop/dist/" },
    [ordered]@{ path = "apps\desktop\src-tauri\target"; ignore = "/apps/desktop/src-tauri/target/" }
)

$gitIgnore = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot ".gitignore")
$hasGitRepository = Test-Path -LiteralPath (Join-Path $RepoRoot ".git")
foreach ($generatedPath in $forbiddenGeneratedPaths) {
    $relativePath = [string]$generatedPath.path
    $ignoreRule = [string]$generatedPath.ignore
    Assert-Contains `
        -Needle $ignoreRule `
        -Haystack $gitIgnore `
        -Message "Generated path must be ignored by the standalone repository: $relativePath"

    if ($hasGitRepository) {
        $tracked = @(& git -C $RepoRoot ls-files -- $relativePath.Replace("\", "/"))
        Assert-True -Condition ($LASTEXITCODE -eq 0) -Message "Unable to inspect tracked generated path: $relativePath"
        Assert-True -Condition ($tracked.Count -eq 0) -Message "Generated path must not be tracked: $relativePath"
    }
}

$cargoToml = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "Cargo.toml")
Assert-Contains `
    -Needle 'repository = "https://github.com/aiaimimi0920/Loom"' `
    -Haystack $cargoToml `
    -Message "Cargo repository metadata must point to the standalone Loom repository."

$readme = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "README.md")
Assert-True `
    -Condition ($readme -notmatch '(?m)^\s*\.\\Loom\\') `
    -Message "README operator commands must run from the standalone repository root."
Assert-Contains `
    -Needle "https://github.com/aiaimimi0920/Loom" `
    -Haystack $readme `
    -Message "README must link to the standalone Loom repository."

$desktopLib = Get-Content -Raw -Encoding UTF8 (Join-Path $RepoRoot "apps\desktop\src-tauri\src\lib.rs")
Assert-True `
    -Condition (-not $desktopLib.Contains('C:\release\Loom')) `
    -Message "Desktop daemon discovery must not contain the hard-coded C:\release\Loom fallback."

Write-Output "Loom standalone layout contract passed."
