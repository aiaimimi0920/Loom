[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}
function Read-RepoText {
    param([string]$RelativePath)
    $path = Join-Path $repoRoot $RelativePath
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Missing dependency security file: $RelativePath"
    return [System.IO.File]::ReadAllText($path, [System.Text.Encoding]::UTF8)
}

$policy = Read-RepoText "security\dependency-security-policy.json" | ConvertFrom-Json
Assert-True ([int]$policy.schemaVersion -eq 1) "Unsupported dependency security policy schema."
Assert-True ($policy.scanner.name -eq "osv-scanner") "Dependency scanner identity changed."
Assert-True ($policy.scanner.version -eq "2.5.0") "Dependency scanner version is not pinned."
Assert-True ($policy.scanner.reusableWorkflow -eq "google/osv-scanner-action/.github/workflows/osv-scanner-reusable.yml@0c58c542420dfd23fcac08dd9c8ca3cca9c36f1a") "OSV reusable workflow pin changed without contract review."
Assert-True ($policy.scanner.actionCommit -eq "06b2ab4348248b456ee06c9e953637f55e03504f") "OSV scanner Action pin changed without contract review."
Assert-True ($policy.scanner.windowsX64Url -eq "https://github.com/google/osv-scanner/releases/download/v2.5.0/osv-scanner_windows_amd64.exe") "OSV Windows download URL changed without contract review."
Assert-True ($policy.scanner.windowsX64Sha256 -eq "4342285bd8be36b9f113468f3eea86e7900befbcd19ca8dc6ac4f0f6cbe7c362") "OSV Windows binary hash changed without contract review."
Assert-True ([int]$policy.maximumExceptionDays -gt 0 -and [int]$policy.maximumExceptionDays -le 90) "Exception lifetime must be at most 90 days."

$expectedLockfiles = @(
    "Cargo.lock",
    "apps/desktop/src-tauri/Cargo.lock",
    "framework-packages/runtime-host/Cargo.lock",
    "apps/desktop/package-lock.json"
)
Assert-True ($policy.lockfiles.Count -eq $expectedLockfiles.Count) "Dependency lockfile inventory changed without review."
foreach ($lockfile in $expectedLockfiles) {
    Assert-True ($policy.lockfiles -contains $lockfile) "Policy does not scan lockfile: $lockfile"
    Assert-True (Test-Path -LiteralPath (Join-Path $repoRoot $lockfile) -PathType Leaf) "Configured lockfile is missing: $lockfile"
}

$config = Read-RepoText ([string]$policy.config).Replace('/', '\')
Assert-True (-not $config.Contains("[[PackageOverrides]]")) "Broad package overrides are forbidden."
$blocks = [regex]::Matches($config, '(?ms)^\[\[IgnoredVulns\]\]\s*(.*?)(?=^\[\[|\z)')
Assert-True ($blocks.Count -gt 0) "No reviewed vulnerability exceptions were found."
$seen = @{}
$today = [DateTime]::UtcNow.Date
foreach ($match in $blocks) {
    $block = $match.Groups[1].Value
    $idMatch = [regex]::Match($block, '(?m)^id\s*=\s*"([A-Z0-9-]+)"\s*$')
    $dateMatch = [regex]::Match($block, '(?m)^ignoreUntil\s*=\s*(\d{4}-\d{2}-\d{2})\s*$')
    $reasonMatch = [regex]::Match($block, '(?m)^reason\s*=\s*"([^"\r\n]+)"\s*$')
    Assert-True $idMatch.Success "Every exception requires one vulnerability ID."
    Assert-True $dateMatch.Success "Exception $($idMatch.Groups[1].Value) requires ignoreUntil."
    Assert-True ($reasonMatch.Success -and $reasonMatch.Groups[1].Value.Length -ge 40) "Exception $($idMatch.Groups[1].Value) requires a concrete reason."
    $id = $idMatch.Groups[1].Value
    Assert-True (-not $seen.ContainsKey($id)) "Duplicate vulnerability exception: $id"
    $seen[$id] = $true
    $expiry = [DateTime]::ParseExact($dateMatch.Groups[1].Value, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
    Assert-True ($expiry -gt $today) "Vulnerability exception expired: $id"
    Assert-True ($expiry -le $today.AddDays([int]$policy.maximumExceptionDays)) "Vulnerability exception exceeds maximum lifetime: $id"
}

$workflow = Read-RepoText ".github\workflows\dependency-security.yml"
foreach ($required in @($policy.scanner.reusableWorkflow, "fail-on-vuln: true", "upload-sarif: false", "checkout-ref", "security/osv-scanner.toml") + $expectedLockfiles) {
    Assert-True $workflow.Contains($required) "Dependency security workflow lost required contract: $required"
}
$dependabot = Read-RepoText ".github\dependabot.yml"
foreach ($required in @("version: 2", "package-ecosystem: cargo", "package-ecosystem: npm", "package-ecosystem: github-actions", "package-ecosystem: docker")) {
    Assert-True $dependabot.Contains($required) "Dependabot configuration lost required contract: $required"
}
$release = Read-RepoText ".github\workflows\release-tag.yml"
Assert-True $release.Contains("uses: ./.github/workflows/dependency-security.yml") "Tag release does not call the dependency security gate."
Assert-True $release.Contains("needs: dependency-security") "Tag publication is not blocked on dependency security."

$installer = Read-RepoText "scripts\Install-OsvScanner.ps1"
foreach ($required in @("Get-FileHash", "windowsX64Sha256", "Invoke-WebRequest", "Assert-ScannerHash")) {
    Assert-True $installer.Contains($required) "OSV installer lost integrity control: $required"
}
$localScan = Read-RepoText "scripts\Invoke-DependencySecurityScan.ps1"
foreach ($required in @("Get-FileHash", "windowsX64Sha256", "--version", "--config=", "--lockfile=")) {
    Assert-True $localScan.Contains($required) "Local dependency scan lost required control: $required"
}

$guidance = @(
    (Read-RepoText "AGENTS.md"),
    (Read-RepoText "CONTRIBUTING.md"),
    (Read-RepoText "README.md"),
    (Read-RepoText "docs\release-provenance.md")
)
foreach ($text in $guidance) {
    Assert-True $text.Contains("docs/DEPENDENCY_SECURITY.md") "Repository guidance does not link dependency security policy."
}
$manual = Read-RepoText "docs\DEPENDENCY_SECURITY.md"
foreach ($required in @("# Loom Dependency Security", "Machine-authoritative inventory", "Temporary exceptions", "Suspected malicious dependency incident", "maximumExceptionDays")) {
    Assert-True $manual.Contains($required) "Dependency security manual lost required guidance: $required"
}

Write-Output "Loom dependency security contract passed: locks=4 exceptions=$($blocks.Count) max-days=$($policy.maximumExceptionDays)"
