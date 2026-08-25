[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$buildPath = Join-Path $repoRoot "scripts\build-release.ps1"
$verifyPath = Join-Path $repoRoot "scripts\verify-release.ps1"
$buildModuleRoot = Join-Path $repoRoot "scripts\build-release"
$buildModuleNames = @(
    "Common.ps1",
    "Catalog.ps1",
    "Plan.ps1",
    "Execution.ps1",
    "FrameworkPackages.ps1",
    "McpPackages.ps1",
    "ArtPackages.ps1",
    "Metadata.ps1",
    "Archives.ps1"
)
$buildModulePaths = @($buildModuleNames | ForEach-Object { Join-Path $buildModuleRoot $_ })
$buildContractPaths = @($buildPath) + $buildModulePaths
$verifyModuleRoot = Join-Path $repoRoot "scripts\verify-release"
$verifyModuleNames = @(
    "Common.ps1",
    "DesktopPayload.ps1",
    "CliSdkPayload.ps1",
    "FrameworkPackages.ps1",
    "McpPackages.ps1",
    "ArtPackages.ps1",
    "SupplyChain.ps1"
)
$verifyModulePaths = @($verifyModuleNames | ForEach-Object { Join-Path $verifyModuleRoot $_ })
$verifyContractPaths = @($verifyPath) + $verifyModulePaths
$smokePath = Join-Path $repoRoot "scripts\smoke-release.ps1"
$smokeModuleRoot = Join-Path $repoRoot "scripts\smoke-release"
$smokeModuleNames = @(
    "Assertions.ps1",
    "Image.ps1",
    "HttpStatus.ps1",
    "ProcessTree.ps1",
    "Evidence.ps1",
    "Process.ps1",
    "CloudFixture.ps1",
    "McpRegistryFixture.ps1",
    "ReleasePhases.ps1",
    "Release.ps1",
    "Focused.ps1"
)
$smokeModulePaths = @($smokeModuleNames | ForEach-Object { Join-Path $smokeModuleRoot $_ })
$smokePortHelperPath = Join-Path $repoRoot "scripts\LoomSmokePorts.ps1"
$focusedSmokePaths = @(
    (Join-Path $repoRoot "scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomRunPersistenceSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomDaemonConcurrencySmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomHookErrorPreviewSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomPluginBoundarySmoke.ps1"),
    (Join-Path $repoRoot "scripts\Invoke-LoomSurfacePrototypeSmoke.ps1")
)
$concurrencySmokeModuleRoot = Join-Path $repoRoot "scripts\daemon-concurrency-smoke"
$concurrencySmokeModuleNames = @("Common.ps1", "Process.ps1", "Http.ps1", "GatewayFixture.ps1")
$concurrencySmokeModulePaths = @($concurrencySmokeModuleNames | ForEach-Object {
    Join-Path $concurrencySmokeModuleRoot $_
})
$layoutPath = Join-Path $repoRoot "scripts\LoomReleaseLayout.ps1"
$tamperPath = Join-Path $repoRoot "scripts\tests\Test-ReleaseIntegrityTamper.ps1"
$pathSafetyPath = Join-Path $repoRoot "scripts\tests\Test-ReleasePathSafety.ps1"
$releaseContractHelperPath = Join-Path $repoRoot "scripts\tests\standalone-release\ReleaseContracts.ps1"
$hookErrorPreviewSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookErrorPreviewSmoke.ps1"
$frameworkArtStoreHookSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1"
$frameworkArtStoreHookSmokeModuleRoot = Join-Path $repoRoot "scripts\framework-art-store-hook-smoke"
$frameworkArtStoreHookSmokeContractPaths = @($frameworkArtStoreHookSmokePath)
$frameworkArtStoreHookSmokeContractPaths += @(@(
    "Assertions.ps1", "Paths.ps1", "Process.ps1", "Http.ps1", "HookBridge.ps1", "FixtureArchive.ps1",
    "ServiceFixtures.ps1", "ArtFixtures.ps1"
) | ForEach-Object { Join-Path $frameworkArtStoreHookSmokeModuleRoot $_ })
$surfacePrototypeSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomSurfacePrototypeSmoke.ps1"
$surfacePrototypeManifestPaths = @(
    (Join-Path $repoRoot "art-packages\surface-prototypes\stock-card\manifest.json"),
    (Join-Path $repoRoot "art-packages\surface-prototypes\dashboard\manifest.json"),
    (Join-Path $repoRoot "art-packages\surface-prototypes\form\manifest.json")
)

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
        [string[]]$Path,
        [string[]]$RequiredText,
        [string[]]$ForbiddenText
    )

    $rawParts = foreach ($scriptPath in @($Path)) {
        Assert-True -Condition (Test-Path -LiteralPath $scriptPath -PathType Leaf) -Message "Missing standalone release script: $scriptPath"
        $tokens = $null
        $parseErrors = $null
        [void][System.Management.Automation.Language.Parser]::ParseFile(
            $scriptPath,
            [ref]$tokens,
            [ref]$parseErrors
        )
        Assert-Equal -Expected 0 -Actual @($parseErrors).Count -Message "PowerShell parse errors in $scriptPath."
        Get-Content -Raw -Encoding UTF8 -LiteralPath $scriptPath
    }
    $raw = @($rawParts) -join [Environment]::NewLine
    $pathLabel = @($Path) -join ", "
    foreach ($needle in $RequiredText) {
        Assert-True -Condition $raw.Contains($needle) -Message "Missing required release contract text in ${pathLabel}: $needle"
    }
    foreach ($needle in $ForbiddenText) {
        Assert-True -Condition (-not $raw.Contains($needle)) -Message "Forbidden release contract text in ${pathLabel}: $needle"
    }
}

function Get-ScriptFunctionDefinition {
    param(
        [System.Management.Automation.Language.ScriptBlockAst]$Ast,
        [string]$Name
    )

    $definition = $Ast.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name
    }, $true)
    Assert-True -Condition ($null -ne $definition) -Message "Missing script function for runtime contract: $Name"
    return [scriptblock]::Create($definition.Extent.Text)
}

. $releaseContractHelperPath

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
    -Path $buildContractPaths `
    -RequiredText @(
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        '[string]$OutputRoot = ".\release\Loom"',
        '[switch]$DryRun',
        '[switch]$RequireCleanSource',
        'New-ExeSpec -Name "Loom.exe"',
        '-DestinationRelativePath "runtime\loom-daemon.exe"',
        'Loom-CLI-',
        'cliArtifact',
        'pluginSdkArtifact',
        'Loom-Plugin-SDK-',
        'Build-LoomArtFrameworkPackages.ps1',
        'Build-LoomMcpServerPackages.ps1',
        'Build-LoomSampleArtPackages.ps1',
        'frameworkPackageCatalog',
        'frameworkPackages',
        'frameworkCatalog',
        'mcpServerPackageCatalog',
        'mcpServerPackages',
        'mcpServerCatalog',
        'sampleArtPackageCatalog',
        'sampleArtPackages',
        'sampleArtCatalog',
        'New-LoomSbom.ps1',
        'build-provenance.json',
        'runtime\resources\ocr',
        'expectedIds = @(',
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
    -Path $verifyContractPaths `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'Loom.exe',
        'runtime\loom-daemon.exe',
        'cliArtifact',
        'Loom-CLI-',
        'pluginSdkArtifact',
        'Loom-Plugin-SDK-',
        'function Assert-FrameworkPackages',
        'function Assert-McpServerPackages',
        'function Assert-SampleArtPackages',
        'frameworkPackages',
        'frameworkCatalog',
        'framework-package-zip-sha256',
        'mcpServerPackages',
        'mcpServerCatalog',
        'mcp-server-package-zip-sha256',
        'sampleArtPackages',
        'sampleArtCatalog',
        'sample-art-package-zip-sha256',
        'Assert-SupplyChainMetadata',
        'Get-LoomReleaseLayout',
        'Assert-ZipChecksumSidecar',
        'ZIP checksum sidecar content mismatch',
        'CLI artifact ZIP byte count mismatch',
        'Desktop ZIP name does not match the manifest version.',
        'CLI ZIP name does not match the manifest version.',
        'Plugin SDK ZIP path does not match its name.',
        'function Get-Sha256HexForBytes',
        'Plugin SDK protocol README does not match the release source.',
        '$sourceReadmePath = Join-Path $repoRoot "protocol\README.md"',
        '[System.StringComparison]::Ordinal',
        '$expectedLine = "$actualZipHash  $zipName"',
        'checksums.sha256',
        'manifest.json',
        '[switch]$RunSmoke',
        '[switch]$RequireCleanSource',
        'function Invoke-CapturedPowerShell',
        'Invoke-LoomHookErrorPreviewSmoke.ps1',
        'hookErrorPreviewSmoke',
        'Invoke-LoomFrameworkArtStoreHookSmoke.ps1',
        'frameworkArtStoreHookSmoke',
        'Invoke-LoomPluginBoundarySmoke.ps1',
        'pluginBoundarySmoke',
        'Invoke-LoomSurfacePrototypeSmoke.ps1',
        'surfacePrototypeSmoke',
        'runtime/python/Arts/',
        '-PackageDir',
        '$previousErrorActionPreference = $ErrorActionPreference',
        '$ErrorActionPreference = "Continue"'
    ) `
    -ForbiddenText $commonForbidden

Assert-ReleaseModuleContracts -ReleaseModules @(
    [pscustomobject]@{ root = $buildModuleRoot; names = $buildModuleNames; entry = $buildPath },
    [pscustomobject]@{ root = $verifyModuleRoot; names = $verifyModuleNames; entry = $verifyPath }
) -CommonForbidden $commonForbidden

$verifyCommonPath = Join-Path $verifyModuleRoot "Common.ps1"
Assert-CapturedPowerShellContract -VerifyCommonPath $verifyCommonPath
Assert-ReleaseSecurityScriptContracts `
    -LayoutPath $layoutPath `
    -TamperPath $tamperPath `
    -PathSafetyPath $pathSafetyPath `
    -CommonForbidden $commonForbidden

Assert-ScriptContract `
    -Path $hookErrorPreviewSmokePath `
    -RequiredText @(
        'Write-HookFixture',
        'failed-art',
        'status = "error"',
        'minified = $true',
        'savedRect',
        'cropOffset',
        'reference = "upstream"',
        'fromPortId = "output"',
        'toPortId = "input"',
        'previewMatchesFailedArtSource',
        'previewDiffersFromUpstream'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $frameworkArtStoreHookSmokeContractPaths `
    -RequiredText @(
        'raw-image-alt.png',
        '"image.candidates"',
        '$mcpCandidates.items',
        '$mcpCandidates.selectedIndex',
        'mcpCandidateCount',
        'result_index',
        'Get-FileHash -LiteralPath $temporaryZipPath -Algorithm SHA256',
        'Write-Utf8NoBomFile -Path $temporarySidecarPath',
        '[System.IO.Compression.ZipArchive]::new',
        'ConvertTo-SmokeZipRelativePath',
        'Get-SmokeDescendantProcessIds',
        'Assert-SmokeLoopbackHttpUri',
        '[int]$MaxMessageBytes = 1MB',
        'method = "loom.hook.art.execute"',
        'protocolVersion = "loom.hook.v1"',
        'outputTransports = @("shared_memory", "websocket")'
    ) `
    -ForbiddenText $commonForbidden

Assert-ScriptContract `
    -Path $surfacePrototypeSmokePath `
    -RequiredText @(
        'build-surface-prototypes.ps1',
        '/v1/surfaces/attach',
        '/v1/surfaces/actions/cancel',
        'instanceReused',
        'isolatedControlPlane',
        'shared resource leases must be attachment-scoped',
        'loom.surface.snapshot'
    ) `
    -ForbiddenText @($commonForbidden) + @(
        '"surface/snapshot"'
    )

Assert-ScriptContract `
    -Path $smokePath `
    -RequiredText @(
        '[Parameter(Mandatory = $true)][string]$PackageDir',
        '$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))',
        'LoomReleaseLayout.ps1',
        '$smokeModuleRoot = Join-Path $PSScriptRoot "smoke-release"',
        'Assertions.ps1',
        'Image.ps1',
        'HttpStatus.ps1',
        'ProcessTree.ps1',
        'Evidence.ps1',
        'Process.ps1',
        'CloudFixture.ps1',
        'McpRegistryFixture.ps1',
        'ReleasePhases.ps1',
        'Release.ps1',
        'Focused.ps1',
        'Invoke-LoomGatewayBrainPlanSmoke.ps1',
        'Invoke-LoomRunPersistenceSmoke.ps1',
        'Invoke-LoomDaemonConcurrencySmoke.ps1',
        'LoomSmokePorts.ps1',
        '$EvidenceRoot'
    ) `
    -ForbiddenText $commonForbidden

$actualSmokeModuleNames = @(Get-ChildItem -LiteralPath $smokeModuleRoot -File -Filter "*.ps1" | Sort-Object Name | ForEach-Object Name)
Assert-Equal `
    -Expected (@($smokeModuleNames | Sort-Object) -join ",") `
    -Actual ($actualSmokeModuleNames -join ",") `
    -Message "Smoke helper module set drifted."

$smokeContractRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokePath
$previousSmokeModuleIndex = -1
foreach ($smokeModuleName in $smokeModuleNames) {
    $smokeModuleIndex = $smokeContractRaw.IndexOf($smokeModuleName, [System.StringComparison]::Ordinal)
    Assert-True `
        -Condition ($smokeModuleIndex -gt $previousSmokeModuleIndex) `
        -Message "Smoke helper module load order drifted at $smokeModuleName."
    $previousSmokeModuleIndex = $smokeModuleIndex
}
foreach ($smokeModulePath in $smokeModulePaths) {
    Assert-ScriptContract `
        -Path $smokeModulePath `
        -RequiredText @('<# Owns') `
        -ForbiddenText $commonForbidden
    $smokeModuleRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $smokeModulePath
    Assert-True `
        -Condition (-not $smokeModuleRaw.Contains('$smokeModuleRoot')) `
        -Message "Smoke helper modules must not recursively import the module directory: $smokeModulePath"
    $smokeContractRaw += [Environment]::NewLine + $smokeModuleRaw
}

foreach ($needle in @(
    'Get-LoomReleaseLayout',
    'Get-LoomSmokePort',
    '/v1/mcp/servers',
    '/v1/workflows',
    '/v1/hook-bridge/status',
    'function Initialize-SmokeEvidenceRun',
    'function Write-SmokeJsonEvidence'
)) {
    Assert-True -Condition $smokeContractRaw.Contains($needle) -Message "Missing aggregate smoke contract text: $needle"
}

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

Assert-True -Condition $smokeContractRaw.Contains('Get-LoomSmokePort') -Message "Modular release smoke must use the shared port allocator."
Assert-True -Condition (-not $smokeContractRaw.Contains('function Get-FreePort')) -Message "Modular release smoke must not retain a local release port allocator."
Assert-True -Condition (-not $smokeContractRaw.Contains('function Get-FreeTcpPort')) -Message "Modular release smoke must not retain a local TCP port allocator."
foreach ($focusedSmokePath in $focusedSmokePaths) {
    $focusedSmokeRaw = Get-Content -Raw -Encoding UTF8 -LiteralPath $focusedSmokePath
    Assert-True -Condition $focusedSmokeRaw.Contains('LoomSmokePorts.ps1') -Message "Smoke must import the shared port allocator: $focusedSmokePath"
    Assert-True -Condition $focusedSmokeRaw.Contains('Get-LoomSmokePort') -Message "Smoke must use the shared port allocator: $focusedSmokePath"
    Assert-True -Condition (-not $focusedSmokeRaw.Contains('function Get-FreePort')) -Message "Smoke must not retain a local release port allocator: $focusedSmokePath"
    Assert-True -Condition (-not $focusedSmokeRaw.Contains('function Get-FreeTcpPort')) -Message "Smoke must not retain a local TCP port allocator: $focusedSmokePath"
}

$actualConcurrencySmokeModuleNames = @(
    Get-ChildItem -LiteralPath $concurrencySmokeModuleRoot -File -Filter "*.ps1" |
        Sort-Object Name |
        ForEach-Object Name
)
Assert-Equal `
    -Expected (@($concurrencySmokeModuleNames | Sort-Object) -join ",") `
    -Actual ($actualConcurrencySmokeModuleNames -join ",") `
    -Message "Daemon concurrency smoke helper module set drifted."
foreach ($concurrencySmokeModulePath in $concurrencySmokeModulePaths) {
    Assert-ScriptContract `
        -Path $concurrencySmokeModulePath `
        -RequiredText @('<# Owns') `
        -ForbiddenText $commonForbidden
}

foreach ($surfacePrototypeManifestPath in $surfacePrototypeManifestPaths) {
    $surfacePrototypeManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $surfacePrototypeManifestPath | ConvertFrom-Json
    $surfaceActions = @($surfacePrototypeManifest.metadata.capabilities.surface.actions)
    Assert-True -Condition ($surfaceActions.Count -gt 0) -Message "Surface prototype must declare actions: $surfacePrototypeManifestPath"
    $runtimeManifestPath = Join-Path (Split-Path -Parent $surfacePrototypeManifestPath) "art.runtime.json"
    Assert-True -Condition (Test-Path -LiteralPath $runtimeManifestPath -PathType Leaf) -Message "Surface prototype runtime manifest is missing: $runtimeManifestPath"
    $runtimeManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimeManifestPath | ConvertFrom-Json
    $maxActionTimeout = [int64](@($surfaceActions | ForEach-Object { [int64]$_.timeoutMs } | Measure-Object -Maximum).Maximum)
    Assert-True `
        -Condition ([int64]$runtimeManifest.limits.timeoutMs -ge $maxActionTimeout) `
        -Message "Surface runtime timeout must cover every declared action: $surfacePrototypeManifestPath runtime=$($runtimeManifest.limits.timeoutMs) action=$maxActionTimeout"
    foreach ($surfaceAction in $surfaceActions) {
        Assert-True `
            -Condition ([int64]$surfaceAction.timeoutMs -ge 10000) `
            -Message "Surface process action timeout must cover framework and Art runtime startup: $surfacePrototypeManifestPath action=$($surfaceAction.id) timeoutMs=$($surfaceAction.timeoutMs)"
    }
}

$versionId = "standalone-contract"
$pathSafetyOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $pathSafetyPath 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Release path safety contract failed: $($pathSafetyOutput -join [Environment]::NewLine)"
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
Assert-Equal -Expected "loom-plugin.exe" -Actual ([string]$defaultPlan.pluginSdkArtifact.pluginCliEntryName) -Message "Dry-run must catalog the plugin developer CLI."
Assert-True -Condition ([string]$defaultPlan.pluginSdkArtifact.zipNamePattern -eq "Loom-Plugin-SDK-{versionId}-windows-x64.zip") -Message "Dry-run plugin SDK ZIP naming contract mismatch."
Assert-Equal -Expected 20 -Actual @($defaultPlan.pluginSdkArtifact.files).Count -Message "Dry-run plugin SDK must contain protocol schemas, Surface SDK, and developer documentation."
Assert-Equal -Expected 1 -Actual @($defaultPlan.pluginSdkArtifact.files | Where-Object { [string]$_.destinationRelativePath -eq "protocol\schemas\surface-stream.v1.schema.json" }).Count -Message "Dry-run plugin SDK must include the Surface stream protocol schema."
Assert-Equal -Expected "process,cloud_api,mcp,workflow" -Actual (@($defaultPlan.frameworkPackageCatalog.expectedIds) -join ",") -Message "Dry-run must catalog all four independent framework packages."
Assert-Equal -Expected (Join-Path $defaultPlan.destination "packages\frameworks") -Actual ([string]$defaultPlan.frameworkPackageCatalog.outputRoot) -Message "Dry-run framework catalog output must stay inside the candidate."
Assert-Equal -Expected "neuro-image-search,stock-api" -Actual (@($defaultPlan.mcpServerPackageCatalog.expectedIds) -join ",") -Message "Dry-run must catalog both independent MCP server packages."
Assert-Equal -Expected (Join-Path $defaultPlan.destination "packages\mcp-servers") -Actual ([string]$defaultPlan.mcpServerPackageCatalog.outputRoot) -Message "Dry-run MCP server catalog output must stay inside the candidate."
Assert-True -Condition (@($defaultPlan.supportFiles | Where-Object { -not ([string]$_.destinationRelativePath).StartsWith("runtime\") }).Count -eq 0) -Message "All daemon-owned support files must live under runtime."
Assert-True -Condition (@($defaultPlan.supportFiles | Where-Object { ([string]$_.destinationRelativePath).Replace('\', '/').StartsWith("runtime/python/Arts/", [System.StringComparison]::OrdinalIgnoreCase) }).Count -eq 0) -Message "Default release must not package optional Python Arts."
Assert-Equal -Expected "." -Actual (@($defaultPlan.sourcePaths) -join ",") -Message "Manifest source paths must be standalone-relative."

$explicitRoot = [System.IO.Path]::GetFullPath((Join-Path $env:TEMP "loom-parent-release-contract"))
$explicitOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $buildPath -VersionId $versionId -OutputRoot $explicitRoot -NoZip -DryRun 2>&1
Assert-Equal -Expected 0 -Actual $LASTEXITCODE -Message "Explicit output build dry-run failed: $($explicitOutput -join [Environment]::NewLine)"
$explicitPlan = ($explicitOutput -join [Environment]::NewLine) | ConvertFrom-Json
Assert-Equal -Expected $explicitRoot -Actual ([string]$explicitPlan.outputRoot) -Message "Explicit parent output root was not preserved."
Assert-Equal -Expected (Join-Path $explicitRoot $versionId) -Actual ([string]$explicitPlan.destination) -Message "Explicit candidate destination mismatch."
Assert-True -Condition (-not (Test-Path -LiteralPath $explicitRoot)) -Message "Dry-run must not create the explicit output root."

Write-Output "Loom standalone release contract passed."
