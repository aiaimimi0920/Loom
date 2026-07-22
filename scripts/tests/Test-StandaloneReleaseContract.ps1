[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$buildPath = Join-Path $repoRoot "scripts\build-release.ps1"
$verifyPath = Join-Path $repoRoot "scripts\verify-release.ps1"
$smokePath = Join-Path $repoRoot "scripts\smoke-release.ps1"
$smokePortHelperPath = Join-Path $repoRoot "scripts\LoomSmokePorts.ps1"
$focusedSmokePaths = @(
    (Join-Path $repoRoot "scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomRunPersistenceSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomDaemonConcurrencySmoke.ps1")
)
$layoutPath = Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1"
$tamperPath = Join-Path $repoRoot "scripts\tests\Test-ReleaseIntegrityTamper.ps1"

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

$powerShellScripts = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "scripts") -Recurse -File -Filter "*.ps1")
foreach ($powerShellScript in $powerShellScripts) {
    $powerShellSource = [System.IO.File]::ReadAllText($powerShellScript.FullName, [System.Text.UTF8Encoding]::new($false, $true))
    Assert-True `
        -Condition (-not [regex]::IsMatch($powerShellSource, '[^\u0000-\u007F]')) `
        -Message "Windows PowerShell 5.1 script source must be ASCII-safe: $($powerShellScript.FullName)"
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
    'Join-Path $repoRoot "Talk"',
    'Split-Path -Parent $loomDaemonExe'
)

Assert-ScriptContract `
    -Path $buildPath `
    -RequiredText @(
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        '[string]$OutputRoot = ".\release\Loom"',
        '[switch]$DryRun',
        'New-ExeSpec -Name "Loom.exe"',
        '-DestinationRelativePath "runtime\loom-daemon.exe"',
        'Loom-CLI-',
        'cliArtifact',
        'runtime\resources\ocr',
        'runtime\bin\python-embed',
        'sourcePaths = @(".")',
        'checksums.sha256',
        'manifest.json',
        '$previousErrorActionPreference = $ErrorActionPreference',
        '$ErrorActionPreference = "Continue"'
    ) `
    -ForbiddenText @(
        $commonForbidden
        'New-ExeSpec -Name "loom.exe"'
        'New-ExeSpec -Name "loom-desktop.exe"'
    )

Assert-ScriptContract `
    -Path $verifyPath `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'Loom.exe',
        'runtime\loom-daemon.exe',
        'cliArtifact',
        'Loom-CLI-',
        'Get-LoomReleaseLayout',
        'Assert-ZipChecksumSidecar',
        'ZIP checksum sidecar content mismatch',
        'CLI artifact ZIP byte count mismatch',
        'Desktop ZIP name does not match the manifest version.',
        'CLI ZIP name does not match the manifest version.',
        '[System.StringComparison]::Ordinal',
        '$expectedLine = "$actualZipHash  $zipName"',
        'checksums.sha256',
        'manifest.json',
        '[switch]$RunSmoke'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $layoutPath `
    -RequiredText @(
        'function Get-LoomReleaseLayout',
        'function Get-LoomArchiveFileEntries',
        'function Assert-LoomDesktopRootExecutableBoundary',
        'function Test-LoomArtifactKind',
        'Loom.exe',
        '$runtimeRoot = Join-Path $packageFullPath "runtime"',
        '$daemonExe = Join-Path $runtimeRoot "loom-daemon.exe"',
        'Loom-CLI-',
        'Loom CLI artifact metadata mismatch.',
        'Loom CLI ZIP must contain exactly one loom.exe entry.',
        'Loom CLI extraction destination must be empty:',
        'Invalid Loom archive entry:',
        '$entry.Name.Length -eq 0',
        '[System.StringComparison]::Ordinal',
        'manifest.json',
        'Expand-Archive'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $tamperPath `
    -RequiredText @(
        'function New-IntegrityFixture',
        'function New-TraversalZip',
        'function New-WhitespaceEntryZip',
        'ExtraRootExecutable',
        'ExtraCliEntry',
        'CliMetadataMismatch',
        'CliEntryCaseMismatch',
        'CliKindCaseMismatch',
        'ForwardSlashPaths',
        'no-newline',
        'extra-line',
        'Traversal archive unexpectedly passed shared entry validation.',
        'Non-empty CLI extraction destination unexpectedly passed validation.',
        'ArtifactNamingMismatch',
        'desktop-wrong',
        'cli-wrong',
        'Loom release integrity tamper contract passed.'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $smokePath `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'LoomReleaseLayout.ps1',
        'Get-LoomReleaseLayout',
        'Invoke-LoomGatewayBrainPlanSmoke.ps1',
        'Invoke-LoomRunPersistenceSmoke.ps1',
        'Invoke-LoomDaemonConcurrencySmoke.ps1',
        'runtime\resources\ocr',
        'runtime\bin\python-embed',
        'LoomSmokePorts.ps1',
        'Get-LoomSmokePort',
        '/v1/mcp/servers',
        '/v1/workflows',
        '/v1/hook-bridge/status',
        'function Initialize-SmokeEvidenceRun',
        'function Write-SmokeJsonEvidence',
        'function Assert-SameExistingPath',
        'Assert-SameExistingPath -Expected $sourcePath -Actual ([string]$read.filePath)',
        '$EvidenceRoot'
    ) `
    -ForbiddenText $commonForbidden

Assert-True -Condition (Test-Path -LiteralPath $smokePortHelperPath -PathType Leaf) -Message "Missing shared smoke port allocator: $smokePortHelperPath"
$smokePortHelperRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokePortHelperPath
foreach ($needle in @(
    '$script:SmokePortMinimum = 30000',
    '$script:SmokePortMaximum = 45000',
    '$script:AllocatedSmokePorts = [System.Collections.Generic.HashSet[int]]::new()',
    'for ($attempt = 0; $attempt -lt 64; $attempt++)',
    'Get-Random -Minimum $script:SmokePortMinimum -Maximum ($script:SmokePortMaximum + 1)',
    '$listener.ExclusiveAddressUse = $true',
    '$script:AllocatedSmokePorts.Add([int]$port)'
)) {
    Assert-True -Condition $smokePortHelperRaw.Contains($needle) -Message "Missing shared smoke port allocator contract text: $needle"
}
Assert-True `
    -Condition (-not [regex]::IsMatch($smokePortHelperRaw, 'TcpListener\]::new\([\s\S]*?,\s*0\s*\)', [System.Text.RegularExpressions.RegexOptions]::CultureInvariant)) `
    -Message "Shared smoke port allocator must not request a Windows dynamic client port with TcpListener port 0."

. $smokePortHelperPath
$allocatedSmokePorts = @(for ($index = 0; $index -lt 64; $index++) { Get-LoomSmokePort })
Assert-Equal -Expected 64 -Actual @($allocatedSmokePorts | Select-Object -Unique).Count -Message "Shared smoke port allocator returned a duplicate port."
Assert-True -Condition (@($allocatedSmokePorts | Where-Object { $_ -lt 30000 -or $_ -gt 45000 }).Count -eq 0) -Message "Shared smoke port allocator returned a port outside 30000-45000."

foreach ($focusedSmokePath in @($smokePath) + $focusedSmokePaths) {
    $focusedSmokeRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $focusedSmokePath
    Assert-True -Condition $focusedSmokeRaw.Contains('LoomSmokePorts.ps1') -Message "Smoke must import the shared port allocator: $focusedSmokePath"
    Assert-True -Condition $focusedSmokeRaw.Contains('Get-LoomSmokePort') -Message "Smoke must use the shared port allocator: $focusedSmokePath"
    Assert-True -Condition (-not $focusedSmokeRaw.Contains('function Get-FreePort')) -Message "Smoke must not retain a local release port allocator: $focusedSmokePath"
    Assert-True -Condition (-not $focusedSmokeRaw.Contains('function Get-FreeTcpPort')) -Message "Smoke must not retain a local TCP port allocator: $focusedSmokePath"
}

$versionId = "standalone-contract"
$defaultOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $buildPath -VersionId $versionId -NoZip -DryRun 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Default standalone build dry-run failed: $($defaultOutput -join [Environment]::NewLine)"
$defaultPlan = ($defaultOutput -join [Environment]::NewLine) | ConvertFrom-Json
$expectedDefaultRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "release\Loom"))
Assert-Equal -Expected $expectedDefaultRoot -Actual ([string]$defaultPlan.outputRoot) -Message "Default release output must stay under the standalone repository."
Assert-Equal -Expected (Join-Path $expectedDefaultRoot $versionId) -Actual ([string]$defaultPlan.destination) -Message "Default candidate destination mismatch."
Assert-Equal -Expected "Loom.exe,loom-daemon.exe" -Actual (@($defaultPlan.exes | ForEach-Object { [string]$_.name }) -join ",") -Message "Dry-run must catalog the desktop entry and internal daemon only."
Assert-Equal -Expected "Loom.exe,runtime\loom-daemon.exe" -Actual (@($defaultPlan.exes | ForEach-Object { [string]$_.destinationRelativePath }) -join ",") -Message "Dry-run executable paths must expose one root entry and one runtime sidecar."
Assert-Equal -Expected "loom.exe" -Actual ([string]$defaultPlan.cliArtifact.entryName) -Message "Dry-run must catalog the separate CLI entry."
Assert-True -Condition ([string]$defaultPlan.cliArtifact.zipNamePattern -eq "Loom-CLI-{versionId}-windows-x64.zip") -Message "Dry-run CLI ZIP naming contract mismatch."
Assert-True -Condition (@($defaultPlan.supportFiles | Where-Object { -not ([string]$_.destinationRelativePath).StartsWith("runtime\") }).Count -eq 0) -Message "All daemon-owned support files must live under runtime."
Assert-Equal -Expected "." -Actual (@($defaultPlan.sourcePaths) -join ",") -Message "Manifest source paths must be standalone-relative."

$explicitRoot = [System.IO.Path]::GetFullPath((Join-Path $env:TEMP "loom-parent-release-contract"))
$explicitOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $buildPath -VersionId $versionId -OutputRoot $explicitRoot -NoZip -DryRun 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Explicit output build dry-run failed: $($explicitOutput -join [Environment]::NewLine)"
$explicitPlan = ($explicitOutput -join [Environment]::NewLine) | ConvertFrom-Json
Assert-Equal -Expected $explicitRoot -Actual ([string]$explicitPlan.outputRoot) -Message "Explicit parent output root was not preserved."
Assert-Equal -Expected (Join-Path $explicitRoot $versionId) -Actual ([string]$explicitPlan.destination) -Message "Explicit candidate destination mismatch."
Assert-True -Condition (-not (Test-Path -LiteralPath $explicitRoot)) -Message "Dry-run must not create the explicit output root."

Write-Output "Loom standalone release contract passed."
