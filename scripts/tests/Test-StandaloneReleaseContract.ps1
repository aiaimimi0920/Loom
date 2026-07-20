[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$buildPath = Join-Path $repoRoot "scripts\build-release.ps1"
$verifyPath = Join-Path $repoRoot "scripts\verify-release.ps1"
$smokePath = Join-Path $repoRoot "scripts\smoke-release.ps1"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-ScriptContract {
    param(
        [string]$Path,
        [string[]]$RequiredText,
        [string[]]$ForbiddenText
    )

    Assert-True -Condition (Test-Path -LiteralPath $Path -PathType Leaf) -Message "Missing standalone release script: $Path"

    $tokens = $null
    $parseErrors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $Path,
        [ref]$tokens,
        [ref]$parseErrors
    )
    Assert-Equal -Expected 0 -Actual @($parseErrors).Count -Message "PowerShell parse errors in $Path."

    $raw = Get-Content -Raw -Encoding UTF8 -LiteralPath $Path
    foreach ($needle in $RequiredText) {
        Assert-True -Condition $raw.Contains($needle) -Message "Missing required release contract text in ${Path}: $needle"
    }
    foreach ($needle in $ForbiddenText) {
        Assert-True -Condition (-not $raw.Contains($needle)) -Message "Forbidden parent release dependency in ${Path}: $needle"
    }
}

$commonForbidden = @(
    '[string[]]$Apps',
    'scripts\build-release-exes.ps1',
    'scripts\verify-release.ps1',
    'scripts\smoke-release-local-apps.ps1',
    'Join-Path $repoRoot "Hook"',
    'Join-Path $repoRoot "Tea"',
    'Join-Path $repoRoot "Platform"',
    'Join-Path $repoRoot "Gateway"',
    'Join-Path $repoRoot "Talk"'
)

Assert-ScriptContract `
    -Path $buildPath `
    -RequiredText @(
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        '[string]$OutputRoot = ".\release\Loom"',
        '[switch]$DryRun',
        'loom.exe',
        'loom-daemon.exe',
        'loom-desktop.exe',
        'resources\ocr',
        'bin\python-embed',
        'sourcePaths = @(".")',
        'checksums.sha256',
        'manifest.json',
        '$previousErrorActionPreference = $ErrorActionPreference',
        '$ErrorActionPreference = "Continue"'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $verifyPath `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'loom.exe',
        'loom-daemon.exe',
        'loom-desktop.exe',
        'checksums.sha256',
        'manifest.json',
        '[switch]$RunSmoke'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $smokePath `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'Invoke-LoomGatewayBrainPlanSmoke.ps1',
        'Invoke-LoomRunPersistenceSmoke.ps1',
        'Invoke-LoomDaemonConcurrencySmoke.ps1',
        'loom-desktop.exe',
        'resources\ocr',
        'bin\python-embed',
        '/v1/mcp/servers',
        '/v1/workflows',
        '/v1/hook-bridge/status',
        'function Initialize-SmokeEvidenceRun',
        'function Write-SmokeJsonEvidence',
        '$EvidenceRoot'
    ) `
    -ForbiddenText $commonForbidden

$versionId = "standalone-contract"
$defaultOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $buildPath -VersionId $versionId -NoZip -DryRun 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Default standalone build dry-run failed: $($defaultOutput -join [Environment]::NewLine)"
$defaultPlan = ($defaultOutput -join [Environment]::NewLine) | ConvertFrom-Json
$expectedDefaultRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "release\Loom"))
Assert-Equal -Expected $expectedDefaultRoot -Actual ([string]$defaultPlan.outputRoot) -Message "Default release output must stay under the standalone repository."
Assert-Equal -Expected (Join-Path $expectedDefaultRoot $versionId) -Actual ([string]$defaultPlan.destination) -Message "Default candidate destination mismatch."
Assert-Equal -Expected "loom.exe,loom-daemon.exe,loom-desktop.exe" -Actual (@($defaultPlan.exes | ForEach-Object { [string]$_.name }) -join ",") -Message "Dry-run must catalog only Loom executables."
Assert-Equal -Expected "." -Actual (@($defaultPlan.sourcePaths) -join ",") -Message "Manifest source paths must be standalone-relative."

$explicitRoot = [System.IO.Path]::GetFullPath((Join-Path $env:TEMP "loom-parent-release-contract"))
$explicitOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $buildPath -VersionId $versionId -OutputRoot $explicitRoot -NoZip -DryRun 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Explicit output build dry-run failed: $($explicitOutput -join [Environment]::NewLine)"
$explicitPlan = ($explicitOutput -join [Environment]::NewLine) | ConvertFrom-Json
Assert-Equal -Expected $explicitRoot -Actual ([string]$explicitPlan.outputRoot) -Message "Explicit parent output root was not preserved."
Assert-Equal -Expected (Join-Path $explicitRoot $versionId) -Actual ([string]$explicitPlan.destination) -Message "Explicit candidate destination mismatch."
Assert-True -Condition (-not (Test-Path -LiteralPath $explicitRoot)) -Message "Dry-run must not create the explicit output root."

Write-Output "Loom standalone release contract passed."
