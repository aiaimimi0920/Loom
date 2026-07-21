[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$workflowRoot = Join-Path $repoRoot ".github\workflows"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Workflow {
    param(
        [string]$Name,
        [string[]]$RequiredText
    )

    $path = Join-Path $workflowRoot $Name
    Assert-True -Condition (Test-Path -LiteralPath $path -PathType Leaf) -Message "Missing GitHub Actions workflow: $Name"
    $raw = Get-Content -Raw -Encoding UTF8 -LiteralPath $path
    foreach ($needle in $RequiredText) {
        Assert-True -Condition $raw.Contains($needle) -Message "Missing required workflow contract in ${Name}: $needle"
    }
    Assert-True -Condition ($raw -notmatch 'github_pat_|ghp_[A-Za-z0-9]+') -Message "Workflow contains a GitHub credential literal: $Name"
    Assert-True -Condition ($raw -notmatch '(?m)^\s*pull_request_target\s*:') -Message "Workflow must not use pull_request_target: $Name"
    Assert-True -Condition ($raw -notmatch 'secrets\.') -Message "Workflow must not depend on repository secrets: $Name"
}

Assert-Workflow -Name "ci.yml" -RequiredText @(
    'name: CI',
    'pull_request:',
    'push:',
    'branches:',
    '- main',
    'workflow_dispatch:',
    'permissions:',
    'contents: read',
    'runs-on: windows-latest',
    'runs-on: ubuntu-latest',
    'actions/checkout@v5',
    'actions/setup-node@v6',
    'node-version: "22"',
    'dtolnay/rust-toolchain@1.95.0',
    'Swatinem/rust-cache@v2',
    'cargo fmt --all -- --check',
    'cargo check --locked --workspace --all-targets',
    'cargo test --locked --workspace',
    'npm ci --no-audit --no-fund',
    'npm run typecheck',
    'npm run build',
    'cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml',
    'cargo fmt --manifest-path .\apps\desktop\src-tauri\Cargo.toml -- --check',
    '.\scripts\tests\Test-StandaloneLayout.ps1',
    '.\scripts\tests\Test-StandaloneReleaseContract.ps1',
    '.\scripts\tests\Test-ReleaseIntegrityTamper.ps1',
    '.\scripts\tests\Test-HookCanvasUiContract.ps1',
    '.\scripts\tests\Test-GitHubActionsContract.ps1'
)

$ciPath = Join-Path $workflowRoot "ci.yml"
$ciRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $ciPath
$validationStep = '      - name: Validate standalone layout before generated output'
$tamperCommand = '          powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1'
$dependencyStep = '      - name: Install desktop dependencies'
$validationIndex = $ciRaw.IndexOf($validationStep, [System.StringComparison]::Ordinal)
$tamperIndex = $ciRaw.IndexOf($tamperCommand, [System.StringComparison]::Ordinal)
$dependencyIndex = $ciRaw.IndexOf($dependencyStep, [System.StringComparison]::Ordinal)
Assert-True -Condition ($validationIndex -ge 0 -and $tamperIndex -gt $validationIndex -and $dependencyIndex -gt $tamperIndex) -Message "Integrity tamper contract must run in the pre-generated-output validation step."
$validationBlock = $ciRaw.Substring($validationIndex, $dependencyIndex - $validationIndex)
Assert-True -Condition $validationBlock.Contains('        shell: powershell') -Message "Integrity tamper contract validation step must use PowerShell."

Assert-Workflow -Name "build-windows.yml" -RequiredText @(
    'name: Build Windows',
    'branches:',
    '- main',
    'workflow_dispatch:',
    'permissions:',
    'contents: read',
    'runs-on: windows-latest',
    'actions/checkout@v5',
    'actions/setup-node@v6',
    'dtolnay/rust-toolchain@1.95.0',
    'Swatinem/rust-cache@v2',
    '.\scripts\build-release.ps1',
    '.\scripts\verify-release.ps1',
    'actions/upload-artifact@v6',
    'if-no-files-found: error'
)

Assert-Workflow -Name "release-tag.yml" -RequiredText @(
    'name: Release Tag',
    "tags:",
    "- 'V*.*.*'",
    'workflow_dispatch:',
    'contents: write',
    'actions/checkout@v5',
    'actions/setup-node@v6',
    'dtolnay/rust-toolchain@1.95.0',
    "'^V\d+\.\d+\.\d+$'",
    '.\scripts\build-release.ps1',
    '.\scripts\verify-release.ps1',
    '-RunSmoke',
    'softprops/action-gh-release@v3',
    'generate_release_notes: true',
    'fail_on_unmatched_files: true',
    '.zip.sha256',
    'Loom-CLI-',
    'Loom-CLI-${{ env.LOOM_TAG }}-windows-x64.zip'
)

Assert-Workflow -Name "docker.yml" -RequiredText @(
    'name: Docker',
    'workflow_dispatch:',
    'permissions:',
    'contents: read',
    'runs-on: ubuntu-latest',
    'actions/checkout@v5',
    'docker build -t loom-ci .'
)

$dockerfile = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "Dockerfile")
Assert-True -Condition $dockerfile.Contains('FROM rust:1.95.0-slim-bookworm AS builder') -Message "Dockerfile must use the pinned Rust 1.95.0 builder."
Assert-True -Condition (-not $dockerfile.Contains('rust:1.91.1')) -Message "Dockerfile must not use the obsolete Rust 1.91.1 builder."

$dockerIgnore = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot ".dockerignore")
foreach ($requiredIgnore in @('**/target/', '**/node_modules/', '**/dist/', 'apps/desktop/', 'release/', 'output/')) {
    Assert-True -Condition $dockerIgnore.Contains($requiredIgnore) -Message "Docker context must ignore generated or unused path: $requiredIgnore"
}

Write-Output "Loom GitHub Actions contract passed."
