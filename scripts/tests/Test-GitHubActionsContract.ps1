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
    'group: ci-${{ github.event.pull_request.number || github.ref }}',
    'cancel-in-progress: true',
    'permissions:',
    'contents: read',
    'runs-on: windows-latest',
    'runs-on: ubuntu-latest',
    'actions/checkout@v5',
    'actions/setup-node@v6',
    'actions/upload-artifact@v6',
    'node-version: "22"',
    'node --test .\scripts\tests\effective-code-lines.test.mjs',
    'node --test .\scripts\tests\dependabot-triage.test.cjs .\scripts\tests\github-release-automation.test.cjs',
    'node .\scripts\effective-code-lines.mjs --mode ratchet --json artifacts/effective-code-lines.json',
    'dtolnay/rust-toolchain@1.95.0',
    'Swatinem/rust-cache@v2',
    'cargo fmt --all -- --check',
    'cargo check --locked --workspace --all-targets',
    'cargo test --locked --workspace',
    'npm ci --no-audit --no-fund',
    'npm run typecheck',
    'npm run build',
    'cargo check --locked --all-targets --manifest-path .\apps\desktop\src-tauri\Cargo.toml',
    'cargo fmt --manifest-path .\apps\desktop\src-tauri\Cargo.toml -- --check',
    'cargo test --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml',
    'cargo fmt --manifest-path .\framework-packages\runtime-host\Cargo.toml -- --check',
    'cargo check --locked --all-targets --manifest-path .\framework-packages\runtime-host\Cargo.toml',
    'cargo test --locked --manifest-path .\framework-packages\runtime-host\Cargo.toml',
    '.\scripts\tests\Test-StandaloneLayout.ps1',
    '.\scripts\tests\Test-StandaloneReleaseContract.ps1',
    '.\scripts\tests\Test-FrameworkArtStoreHookSmokeModules.ps1',
    '.\scripts\tests\Test-SmokeReleaseModules.ps1',
    '.\scripts\tests\Test-ReleaseIntegrityTamper.ps1',
    '.\scripts\tests\Test-HookCanvasUiContract.ps1',
    '.\scripts\tests\Test-DevelopmentManualContract.ps1',
    '.\scripts\tests\Test-DependencySecurityContract.ps1',
    '.\scripts\tests\Test-GitHubActionsContract.ps1',
    '.\scripts\tests\Test-MaliciousPluginPackages.ps1',
    'Clean-host plugin SDK and schema validation',
    'cli_sign_trust_pack_install_conformance_and_revoke_e2e'
)

$ciPath = Join-Path $workflowRoot "ci.yml"
$ciRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $ciPath
$validationStep = '      - name: Validate standalone layout before generated output'
$tamperCommand = '          powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1'
$dependencyStep = '      - name: Install desktop dependencies'
$effectiveLineStep = '      - name: Enforce effective-code-line migration ratchet'
$validationIndex = $ciRaw.IndexOf($validationStep, [System.StringComparison]::Ordinal)
$tamperIndex = $ciRaw.IndexOf($tamperCommand, [System.StringComparison]::Ordinal)
$dependencyIndex = $ciRaw.IndexOf($dependencyStep, [System.StringComparison]::Ordinal)
$effectiveLineIndex = $ciRaw.IndexOf($effectiveLineStep, [System.StringComparison]::Ordinal)
Assert-True -Condition ($validationIndex -ge 0 -and $tamperIndex -gt $validationIndex -and $dependencyIndex -gt $tamperIndex) -Message "Integrity tamper contract must run in the pre-generated-output validation step."
Assert-True -Condition ($effectiveLineIndex -ge 0 -and $effectiveLineIndex -lt $dependencyIndex) -Message "Effective-line ratchet must run before dependency installation and generated output."
$validationBlock = $ciRaw.Substring($validationIndex, $dependencyIndex - $validationIndex)
Assert-True -Condition $validationBlock.Contains('        shell: powershell') -Message "Integrity tamper contract validation step must use PowerShell."
Assert-True -Condition ([regex]::IsMatch(
    $validationBlock,
    '(?m)^          powershell -NoProfile -ExecutionPolicy Bypass -File \.\\scripts\\tests\\Test-DevelopmentManualContract\.ps1\r?$'
)) -Message "Development manual contract must execute in the pre-generated-output PowerShell step."
Assert-True -Condition ([regex]::IsMatch(
    $validationBlock,
    '(?m)^          powershell -NoProfile -ExecutionPolicy Bypass -File \.\\scripts\\tests\\Test-DependencySecurityContract\.ps1\r?$'
)) -Message "Dependency security contract must execute in the pre-generated-output PowerShell step."

Assert-Workflow -Name "dependency-security.yml" -RequiredText @(
    'name: Dependency Security',
    'pull_request:',
    'push:',
    'schedule:',
    'workflow_dispatch:',
    'workflow_call:',
    'group: dependency-security-${{ github.event.pull_request.number || inputs.checkout-ref || github.ref }}',
    'cancel-in-progress: ${{ github.event_name == ''pull_request'' }}',
    'actions: read',
    'contents: read',
    'security-events: write',
    'google/osv-scanner-action/.github/workflows/osv-scanner-reusable.yml@0c58c542420dfd23fcac08dd9c8ca3cca9c36f1a',
    'ref: ${{ inputs.checkout-ref || github.ref }}',
    '--config=./security/osv-scanner.toml',
    '--lockfile=./Cargo.lock',
    '--lockfile=./apps/desktop/src-tauri/Cargo.lock',
    '--lockfile=./framework-packages/runtime-host/Cargo.lock',
    '--lockfile=./apps/desktop/package-lock.json',
    'upload-sarif: true',
    'fail-on-vuln: true'
)

Assert-Workflow -Name "codeql.yml" -RequiredText @(
    'name: CodeQL',
    'pull_request:',
    'push:',
    'schedule:',
    'workflow_dispatch:',
    'group: codeql-${{ github.event.pull_request.number || github.ref }}',
    'cancel-in-progress: ${{ github.event_name == ''pull_request'' }}',
    'actions: read',
    'contents: read',
    'packages: read',
    'security-events: write',
    'runs-on: ubuntu-latest',
    'actions/checkout@v5',
    'persist-credentials: false',
    'language: javascript-typescript',
    'language: rust',
    'language: actions',
    'build-mode: none',
    'github/codeql-action/init@v4',
    'queries: security-extended',
    'github/codeql-action/analyze@v4'
)

Assert-Workflow -Name "build-windows.yml" -RequiredText @(
    'name: Build Windows',
    'branches:',
    '- main',
    'workflow_dispatch:',
    'group: build-windows-${{ github.ref }}',
    'cancel-in-progress: true',
    'permissions:',
    'contents: read',
    'runs-on: windows-latest',
    'actions/checkout@v5',
    'persist-credentials: false',
    'actions/setup-node@v6',
    'dtolnay/rust-toolchain@1.95.0',
    'Swatinem/rust-cache@v2',
    '.\scripts\build-release.ps1',
    '.\scripts\verify-release.ps1',
    '-RunSmoke',
    'actions/upload-artifact@v6',
    'if-no-files-found: error'
)

Assert-Workflow -Name "release-tag.yml" -RequiredText @(
    'name: Release Tag',
    "tags:",
    "- 'V*.*.*'",
    'workflow_dispatch:',
    'group: release-tag-${{ github.event_name == ''workflow_dispatch'' && inputs.tag || github.ref_name }}',
    'cancel-in-progress: false',
    'contents: write',
    'id-token: write',
    'attestations: write',
    'uses: ./.github/workflows/dependency-security.yml',
    'checkout-ref: ${{ github.event_name == ''workflow_dispatch'' && inputs.tag || github.ref }}',
    'needs: dependency-security',
    'security-events: write',
    'actions/checkout@v5',
    'persist-credentials: false',
    'actions/setup-node@v6',
    'dtolnay/rust-toolchain@1.95.0',
    "'^V\d+\.\d+\.\d+$'",
    '.\scripts\build-release.ps1',
    '.\scripts\verify-release.ps1',
    '-RunSmoke',
    '-RequireCleanSource',
    'actions/attest-build-provenance@v2',
    'actions/attest-sbom@v2',
    'softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228',
    'actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3',
    'Check release publication state',
    '.github/scripts/release-publication.cjs',
    'id: draft-release',
    'draft: true',
    'steps.draft-release.outputs.id',
    'Verify assets and publish draft release',
    'Remove failed draft release',
    'generate_release_notes: true',
    'fail_on_unmatched_files: true',
    '.zip.sha256',
    'Loom-CLI-',
    'Loom-CLI-${{ env.LOOM_TAG }}-windows-x64.zip',
    'Loom-Plugin-SDK-${{ env.LOOM_TAG }}-windows-x64.zip',
    'sbom/*.json',
    'provenance/*.json'
)

Assert-Workflow -Name "release-recovery.yml" -RequiredText @(
    'name: Release Recovery',
    'workflow_run:',
    '- Release Tag',
    '- completed',
    'actions: write',
    'contents: read',
    'issues: write',
    'group: release-recovery-${{ github.event.workflow_run.id }}',
    'cancel-in-progress: false',
    'ref: ${{ github.event.repository.default_branch }}',
    'persist-credentials: false',
    'actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3',
    '.github/scripts/release-recovery.cjs'
)

Assert-Workflow -Name "dependabot-triage.yml" -RequiredText @(
    'name: Dependabot Triage',
    'workflow_run:',
    '- CI',
    'workflow_dispatch:',
    'actions: read',
    'contents: read',
    'issues: write',
    'pull-requests: read',
    'Classify without merging',
    'ref: ${{ github.event.repository.default_branch }}',
    'persist-credentials: false',
    'actions/github-script@3a2844b7e9c422d3c10d287c895573f7108da1b3',
    '.github/scripts/dependabot-triage.cjs'
)

Assert-Workflow -Name "docker.yml" -RequiredText @(
    'name: Docker',
    'workflow_dispatch:',
    'permissions:',
    'contents: read',
    'security-events: write',
    'runs-on: ubuntu-latest',
    'actions/checkout@v5',
    'docker/setup-buildx-action@v3',
    'docker/build-push-action@v6',
    '.github/workflows/docker.yml',
    'examples/**',
    'provenance: false',
    'sbom: false',
    'aquasecurity/trivy-action@ed142fd0673e97e23eac54620cfb913e5ce36c25',
    'format: sarif',
    'output: trivy-results.sarif',
    'limit-severities-for-sarif: true',
    'github/codeql-action/upload-sarif@v4',
    "if: always() && steps.trivy.outcome != 'skipped'",
    "if: always() && steps.trivy.outcome == 'failure'"
)

$dockerfile = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "Dockerfile")
Assert-True -Condition $dockerfile.Contains('FROM rust:1.95.0-slim-bookworm AS builder') -Message "Dockerfile must use the pinned Rust 1.95.0 builder."
Assert-True -Condition (-not $dockerfile.Contains('rust:1.91.1')) -Message "Dockerfile must not use the obsolete Rust 1.91.1 builder."

$dockerIgnore = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot ".dockerignore")
foreach ($requiredIgnore in @('**/target/', '**/node_modules/', '**/dist/', 'apps/desktop/', 'release/', 'output/')) {
    Assert-True -Condition $dockerIgnore.Contains($requiredIgnore) -Message "Docker context must ignore generated or unused path: $requiredIgnore"
}

Write-Output "Loom GitHub Actions contract passed."
