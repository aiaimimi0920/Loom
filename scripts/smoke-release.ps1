[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")
$PackageDir = [System.IO.Path]::GetFullPath($PackageDir)
if (-not (Test-Path -LiteralPath $PackageDir -PathType Container)) {
    throw "Loom package directory is missing: $PackageDir"
}
$VersionId = Split-Path -Leaf $PackageDir
$resolvedApps = @("Loom")
$ExpectedLoomCapabilityIds = "brain.plan,tea.ticket.decompose.v1,tea.ticket.execute.v1,tea.ticket.review.v1"
$script:SmokeEvidenceRunId = ""
$script:SmokeEvidenceRunDir = ""
$script:DaemonAuthHeaders = @{}
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    $EvidenceRoot = Join-Path $repoRoot "target\runtime-smoke"
}
$EvidenceRoot = [System.IO.Path]::GetFullPath($EvidenceRoot)

$smokeModuleRoot = Join-Path $PSScriptRoot "smoke-release"
@(
    "Assertions.ps1"
    "Image.ps1"
    "HttpStatus.ps1"
    "ProcessTree.ps1"
    "Evidence.ps1"
    "Process.ps1"
    "CloudFixture.ps1"
    "McpRegistryFixture.ps1"
    "ReleasePhases.ps1"
    "Release.ps1"
    "Focused.ps1"
) | ForEach-Object {
    . (Join-Path $smokeModuleRoot $_)
}

Initialize-SmokeEvidenceRun
$localResult = Test-LoomRelease
$focusedResults = @(
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomGatewayBrainPlanSmoke.ps1" `
        -EvidenceSubdirectory "gateway"
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomRunPersistenceSmoke.ps1" `
        -EvidenceSubdirectory "persistence"
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomDaemonConcurrencySmoke.ps1" `
        -EvidenceSubdirectory "concurrency"
)

$safeVersion = $VersionId -replace "[^A-Za-z0-9._-]", "_"
$summaryFileName = "loom-release-$safeVersion-summary.json"
$summary = [ordered]@{
    schemaVersion = 1
    mode = "loom-release-smoke"
    status = "passed"
    versionId = $VersionId
    packageDir = $PackageDir
    evidenceRunId = $script:SmokeEvidenceRunId
    evidenceRunDir = $script:SmokeEvidenceRunDir
    summaryEvidencePath = $null
    summaryLatestEvidencePath = $null
    local = $localResult
    focused = $focusedResults
}
$summary.summaryEvidencePath = Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary
$summary.summaryLatestEvidencePath = Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary -Latest
$summary | ConvertTo-Json -Depth 40
