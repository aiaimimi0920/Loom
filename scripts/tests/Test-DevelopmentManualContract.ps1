[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Read-RequiredText {
    param([string]$RelativePath)
    $path = Join-Path $repoRoot $RelativePath
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Missing development guidance: $RelativePath"
    return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
}

$agentInstructions = Read-RequiredText "AGENTS.md"
$manual = Read-RequiredText "docs\DEVELOPMENT.md"
$contributing = Read-RequiredText "CONTRIBUTING.md"
$readme = Read-RequiredText "README.md"
$policy = Read-RequiredText "scripts\effective-code-lines-policy.json" | ConvertFrom-Json

Assert-True ([int]$policy.thresholds.target -eq 150) "Effective-line target threshold changed."
Assert-True ([int]$policy.thresholds.acceptable -eq 500) "Effective-line acceptable threshold changed."
Assert-True ([int]$policy.thresholds.soft -eq 700) "Effective-line soft threshold changed."
Assert-True ([int]$policy.thresholds.hard -eq 1500) "Effective-line hard threshold changed."

foreach ($requiredText in @(
    "docs/DEVELOPMENT.md",
    "scripts/effective-code-lines-policy.json",
    "scripts/effective-code-lines-exceptions.json",
    "Target about 150 effective lines",
    "501-700 is a reviewed soft exception",
    "701-1500 requires a split",
    "More than 1500 is a hard-cap violation",
    "--mode strict",
    "An agent must not invent approval",
    "security, vulnerability, resource lifetime"
)) {
    Assert-True $agentInstructions.Contains($requiredText) "AGENTS.md lost a mandatory development rule: $requiredText"
}

foreach ($requiredText in @(
    "# Loom Development Manual",
    "## 1. Effective code lines",
    "## 2. Size thresholds",
    "## 4. The 501-700 exception process",
    "## 6. Post-split security review",
    "## 7. Memory and resource lifetime review",
    "## 8. Performance review",
    "scripts/effective-code-lines-exceptions.json",
    "--mode strict",
    "git diff --check"
)) {
    Assert-True $manual.Contains($requiredText) "Development manual lost required guidance: $requiredText"
}

Assert-True $contributing.Contains("docs/DEVELOPMENT.md") "CONTRIBUTING.md does not link the development manual."
Assert-True $contributing.Contains("--mode strict") "CONTRIBUTING.md does not require the strict size gate."
Assert-True $readme.Contains("docs/DEVELOPMENT.md") "README.md does not index the development manual."

Write-Output "Loom development manual contract passed: auto-agent-entry=1 thresholds=150/500/700/1500 strict-gate=required"
