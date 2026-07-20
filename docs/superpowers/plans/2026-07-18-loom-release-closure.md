# Loom Release Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Loom's stale desktop/documentation contracts and add an auditable Loom-scoped release provenance gate without changing runtime behavior or other subprojects' release rules.

**Architecture:** The shared release builder keeps repository-wide `gitDirty` and adds optional Loom-only `sourceGitDirty` plus an exact `sourcePaths` list. The formal verifier trusts scoped cleanliness only for Loom manifests whose path set exactly matches the approved Loom release inputs; legacy Loom manifests and every non-Loom app retain the existing global dirty gate.

**Tech Stack:** PowerShell 7/Windows PowerShell contract scripts, Git porcelain status, Rust/Cargo, React/TypeScript/Rsbuild, Tauri 2, Markdown release documentation.

---

## File Map

- Modify `scripts/tests/test-loom-desktop-shell-contract.ps1`: validate current Chinese desktop copy.
- Modify `scripts/build-release-exes.ps1`: compute and emit optional Loom-scoped provenance.
- Modify `scripts/tests/test-build-release-exes-contract.ps1`: lock the approved Loom source path set and dry-run schema.
- Modify `scripts/verify-release.ps1`: enforce scoped provenance for new Loom manifests and global provenance everywhere else.
- Modify `scripts/tests/test-verify-release-contract.ps1`: exercise scoped-clean, scoped-dirty, malformed-scope, legacy, and non-Loom cases.
- Modify `scripts/tests/test-loom-artloom-parity-contract.ps1`: prevent migration and release-evidence documentation from drifting again.
- Modify `Loom/docs/MIGRATION_MAP.md`: describe OCR, image, Python, MCP, workflow, and Hook compatibility as implemented.
- Modify `docs/loom/progress/MASTER.md`: preserve Phase 38 as historical evidence and require a regenerated next candidate.
- Modify `docs/loom/analysis/final-artloom-parity-matrix.md`: label the June 18 package as audit-time evidence, not the perpetual latest release.
- Modify `docs/architecture/neuro-release-artifact-standard.md`: document Loom's scoped source provenance and legacy fallback.

### Task 1: Align the Desktop Shell Contract With the Localized UI

**Files:**
- Modify: `scripts/tests/test-loom-desktop-shell-contract.ps1:90-107`
- Reference: `Loom/apps/desktop/src/App.tsx:123-133`
- Reference: `Loom/apps/desktop/src/App.tsx:3453-3460`

- [ ] **Step 1: Reproduce the stale contract failure**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
```

Expected: FAIL with `Missing=[Layer 2 Workbench]`.

- [ ] **Step 2: Replace obsolete English copy assertions with current Chinese UI assertions**

Replace the user-visible assertion block with:

```powershell
$appSource = Get-Content -Raw -LiteralPath $appPath
Assert-Contains "本地工作台" $appSource "Desktop UI must identify Loom as the local workbench."
Assert-Contains "总览" $appSource "Desktop UI must include localized overview navigation."
Assert-Contains "MCP" $appSource "Desktop UI must include MCP navigation."
Assert-Contains "Art 注册表" $appSource "Desktop UI must include the localized Art registry."
Assert-Contains "工作流管理" $appSource "Desktop UI must include localized workflow management."
Assert-Contains "截图同步" $appSource "Desktop UI must include localized Hook screenshot sync."
Assert-Contains "工作流工作台" $appSource "Desktop UI must include the localized workflow workbench."
Assert-Contains "启动截图同步" $appSource "Desktop Hook panel must expose a localized start action."
Assert-Contains "停止截图同步" $appSource "Desktop Hook panel must expose a localized stop action."
Assert-Contains "智能体" $appSource "Desktop UI must include localized agent navigation."
Assert-Contains "运行记录" $appSource "Desktop UI must include localized run navigation."
Assert-Contains "设置" $appSource "Desktop UI must include localized settings navigation."
Assert-Contains "关于" $appSource "Desktop UI must include localized about navigation."
Assert-Contains "readLoomSnapshot" $appSource "Desktop UI must use the Loom API snapshot client."
Assert-Contains "startHookBridge" $appSource "Desktop UI must use the Loom API bridge start client."
Assert-Contains "stopHookBridge" $appSource "Desktop UI must use the Loom API bridge stop client."
Assert-True (-not $appSource.Contains("NeuroLoom")) "Desktop UI product surface must use Loom naming without the old Neuro prefix."
```

Do not add hidden English strings to `App.tsx`; the contract must follow the product UI.

- [ ] **Step 3: Verify the focused desktop and parity contracts**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

Expected: both commands exit `0` and print their passed messages.

- [ ] **Step 4: Commit the localized contract repair**

```powershell
git add -- scripts/tests/test-loom-desktop-shell-contract.ps1
git commit -m "test(loom): align desktop shell contract with localized UI"
```

### Task 2: Emit Loom-Scoped Provenance From the Release Builder

**Files:**
- Modify: `scripts/tests/test-build-release-exes-contract.ps1`
- Modify: `scripts/build-release-exes.ps1:57-70`
- Modify: `scripts/build-release-exes.ps1:198-242`
- Modify: `scripts/build-release-exes.ps1:494-541`
- Modify: `scripts/build-release-exes.ps1:620-679`
- Modify: `scripts/build-release-exes.ps1:700-875`
- Modify: `scripts/build-release-exes.ps1:892-907`

- [ ] **Step 1: Add failing dry-run and source-field contract assertions**

After reading `$scriptSource`, add:

```powershell
Assert-True ($scriptSource.Contains("sourceGitDirty")) "Loom release manifests must record scoped source dirty state."
Assert-True ($scriptSource.Contains("sourcePaths")) "Loom release manifests must record their approved source paths."
```

Inside the existing `if ($app -eq "Loom")` dry-run block, add:

```powershell
$loomSourcePaths = @($appPlan.sourcePaths | ForEach-Object { [string]$_ })
Assert-Equal "Loom,scripts/build-release-exes.ps1" ($loomSourcePaths -join ",") "Loom dry-run must expose the exact scoped release source paths."
```

After that block, add the non-Loom guard:

```powershell
if ($app -ne "Loom") {
    Assert-True ($null -eq $appPlan.PSObject.Properties["sourcePaths"]) "Only Loom may opt into scoped release source paths in this change."
}
```

- [ ] **Step 2: Run the builder contract and verify it fails for missing scoped provenance**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
```

Expected: FAIL because `sourceGitDirty`/`sourcePaths` and the Loom dry-run property do not exist yet.

- [ ] **Step 3: Add a path-scoped Git dirty helper**

Add after `Get-GitDirty`:

```powershell
function Get-GitDirtyForPaths {
    param([string[]]$Paths)

    if (@($Paths).Count -eq 0) {
        return $null
    }

    try {
        $output = & git -C $repoRoot status --porcelain -- @Paths 2>$null
        if ($LASTEXITCODE -ne 0) {
            return $null
        }

        $lines = @($output | Where-Object { -not [string]::IsNullOrWhiteSpace($_.ToString()) })
        return ($lines.Count -gt 0)
    } catch {
        return $null
    }
}
```

This deliberately returns `$null` when Git cannot prove the scope clean.

- [ ] **Step 4: Declare the approved Loom source paths in the app catalog**

Add to the `Loom` ordered dictionary immediately after `sourceProject`:

```powershell
sourcePaths = @(
    "Loom"
    "scripts/build-release-exes.ps1"
)
```

Do not add `sourcePaths` to any other app catalog entry.

- [ ] **Step 5: Expose optional source paths in dry-run output**

After `$appPlan` is created in `Build-DryRunPlan`, add:

```powershell
if ($spec.Contains("sourcePaths")) {
    $appPlan["sourcePaths"] = @($spec["sourcePaths"])
}
```

- [ ] **Step 6: Record scoped state in BUILD_INFO and manifest output**

Extend `New-BuildInfoText` parameters with:

```powershell
[object]$SourceGitDirty,
[string[]]$SourcePaths = @(),
```

Before the here-string, build the optional lines:

```powershell
$sourceStateLines = @()
if (@($SourcePaths).Count -gt 0) {
    $sourceStateLines += "Source git dirty: $SourceGitDirty"
    $sourceStateLines += "Source paths: $($SourcePaths -join ', ')"
}
```

Insert this directly after `Git dirty: $GitDirty` in the here-string:

```powershell
$($sourceStateLines -join [Environment]::NewLine)
```

Extend `Invoke-AppReleaseBuild` with `[object]$SourceGitDirty`, pass it and
`@($Spec["sourcePaths"])` into `New-BuildInfoText`, then add optional manifest
fields after the base manifest is created:

```powershell
if ($Spec.Contains("sourcePaths")) {
    $manifest["sourceGitDirty"] = $SourceGitDirty
    $manifest["sourcePaths"] = @($Spec["sourcePaths"])
}
```

- [ ] **Step 7: Compute scoped state independently for each selected app**

Inside the final app loop, before `Invoke-AppReleaseBuild`, add:

```powershell
$sourceGitDirty = if ($catalog[$appName].Contains("sourcePaths")) {
    Get-GitDirtyForPaths -Paths @($catalog[$appName]["sourcePaths"])
} else {
    $null
}
```

Pass `-SourceGitDirty $sourceGitDirty` to `Invoke-AppReleaseBuild`.

- [ ] **Step 8: Run the builder contracts and inspect Loom dry-run output**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
$plan = powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -DryRun -VersionId contract-scoped-provenance -NoZip -Apps Loom | ConvertFrom-Json
$plan.apps[0].sourcePaths -join ","
```

Expected:

```text
Release build dry-run contract passed.
Loom,scripts/build-release-exes.ps1
```

- [ ] **Step 9: Commit the builder provenance change**

```powershell
git add -- scripts/build-release-exes.ps1 scripts/tests/test-build-release-exes-contract.ps1
git commit -m "feat(release): record Loom scoped source provenance"
```

### Task 3: Enforce Scoped Loom Provenance in the Formal Verifier

**Files:**
- Modify: `scripts/tests/test-verify-release-contract.ps1`
- Modify: `scripts/verify-release.ps1:360-378`
- Modify: `scripts/verify-release.ps1:472-513`

- [ ] **Step 1: Parameterize the executable package fixture**

Rename `Write-HookPackage` to `Write-ExePackageFixture` and use this signature:

```powershell
function Write-ExePackageFixture {
    param(
        [string]$PackageDir,
        [string]$VersionId,
        [string]$AppName,
        [string]$ExeName,
        [bool]$GitDirty,
        [switch]$IncludeScopedProvenance,
        [object]$SourceGitDirty = $null,
        [string[]]$SourcePaths = @()
    )
```

Within the helper, replace hard-coded Hook values as follows:

```powershell
Write-Utf8NoBomFile -Path (Join-Path $PackageDir $ExeName) -Content "fake $AppName exe"
$zipRelative = "packages\$AppName-$VersionId-windows-x64.zip"
Write-Utf8NoBomFile -Path "$zipPath.sha256" -Content "$zipHash  $AppName-$VersionId-windows-x64.zip`r`n"

$manifest = [ordered]@{
    schemaVersion = 1
    app = $AppName
    sourceProject = $AppName
    versionId = $VersionId
    builtAt = "2026-06-09T00:00:00.0000000+08:00"
    gitHead = "0123456789abcdef0123456789abcdef01234567"
    gitShortSha = "01234567"
    gitDirty = $GitDirty
    profile = "release"
    target = "windows-x64"
    repoRoot = $repoRoot
    releaseRoot = Split-Path -Parent (Split-Path -Parent $PackageDir)
    destination = $PackageDir
    commands = @(
        [ordered]@{
            display = "fake $AppName release command"
            workingDirectory = $repoRoot
        }
    )
    exes = @(
        Get-FileRecord -BasePath $PackageDir -RelativePath $ExeName -Kind "exe" -Name $ExeName
    )
    supportFiles = @()
    buildInfo = Get-FileRecord -BasePath $PackageDir -RelativePath "BUILD_INFO.txt" -Kind "build-info" -Name "BUILD_INFO.txt"
    buildLogs = @(
        Get-FileRecord -BasePath $PackageDir -RelativePath "logs\hook-build.log" -Kind "build-log" -Name "hook-build.log"
    )
    artifacts = @(
        Get-FileRecord -BasePath $PackageDir -RelativePath $zipRelative -Kind "zip" -Name "$AppName-$VersionId-windows-x64.zip"
        Get-FileRecord -BasePath $PackageDir -RelativePath "$zipRelative.sha256" -Kind "zip-sha256" -Name "$AppName-$VersionId-windows-x64.zip.sha256"
    )
    checksums = "checksums.sha256"
}

if ($IncludeScopedProvenance) {
    $manifest["sourceGitDirty"] = $SourceGitDirty
    $manifest["sourcePaths"] = @($SourcePaths)
}
```

Update existing Hook fixture calls to pass `-AppName Hook -ExeName hook.exe`.

- [ ] **Step 2: Add failing scoped provenance cases**

Use the approved scope in the test:

```powershell
$approvedLoomSourcePaths = @("Loom", "scripts/build-release-exes.ps1")
```

Add these cases inside the existing temporary release root:

```powershell
$scopedCleanVersion = "contract-loom-scoped-clean"
$scopedCleanPackage = Join-Path $releaseRoot "Loom\$scopedCleanVersion"
Write-ExePackageFixture -PackageDir $scopedCleanPackage -VersionId $scopedCleanVersion -AppName Loom -ExeName loom.exe -GitDirty $true -IncludeScopedProvenance -SourceGitDirty $false -SourcePaths $approvedLoomSourcePaths
$scopedCleanOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $scriptPath -VersionId $scopedCleanVersion -Apps Loom -ReleaseRoot $releaseRoot 2>&1
Assert-Equal 0 $LASTEXITCODE "Formal verifier should accept a scoped-clean Loom package in a globally dirty repository. Output: $($scopedCleanOutput -join [Environment]::NewLine)"
$scopedCleanJson = ($scopedCleanOutput -join [Environment]::NewLine) | ConvertFrom-Json
Assert-Equal $true ([bool]$scopedCleanJson.apps[0].gitDirty) "Verifier must preserve repository-wide dirty evidence."
Assert-Equal $false ([bool]$scopedCleanJson.apps[0].sourceGitDirty) "Verifier must report scoped Loom cleanliness."
Assert-Equal ($approvedLoomSourcePaths -join ",") (@($scopedCleanJson.apps[0].sourcePaths) -join ",") "Verifier must report the approved Loom source paths."
```

Add the failure-case table and execution loop:

```powershell
@(
    [ordered]@{ name = "scoped-dirty"; sourceDirty = $true; paths = $approvedLoomSourcePaths; expected = "Manifest sourceGitDirty must be false for a formal Loom release package." },
    [ordered]@{ name = "missing-path"; sourceDirty = $false; paths = @("Loom"); expected = "Manifest sourcePaths must exactly match the approved Loom release source paths." },
    [ordered]@{ name = "extra-path"; sourceDirty = $false; paths = @("Loom", "scripts/build-release-exes.ps1", "scripts/verify-release.ps1"); expected = "Manifest sourcePaths must exactly match the approved Loom release source paths." },
    [ordered]@{ name = "duplicate-path"; sourceDirty = $false; paths = @("Loom", "Loom"); expected = "Manifest sourcePaths must exactly match the approved Loom release source paths." },
    [ordered]@{ name = "altered-path"; sourceDirty = $false; paths = @("Loom", "scripts/verify-release.ps1"); expected = "Manifest sourcePaths must exactly match the approved Loom release source paths." }
)
```

Store that array in `$scopedFailureCases`, then execute it with:

```powershell
foreach ($case in $scopedFailureCases) {
    $caseVersion = "contract-loom-$($case.name)"
    $casePackage = Join-Path $releaseRoot "Loom\$caseVersion"
    Write-ExePackageFixture -PackageDir $casePackage -VersionId $caseVersion -AppName Loom -ExeName loom.exe -GitDirty $true -IncludeScopedProvenance -SourceGitDirty $case.sourceDirty -SourcePaths @($case.paths)

    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $caseOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $scriptPath -VersionId $caseVersion -Apps Loom -ReleaseRoot $releaseRoot 2>&1
        $caseExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    Assert-True ($caseExitCode -ne 0) "Formal verifier must reject Loom case '$($case.name)'."
    Assert-Contains ([string]$case.expected) ($caseOutput -join [Environment]::NewLine) "Formal verifier rejection for '$($case.name)' must explain the scoped provenance failure."
}
```

Also add:

- a legacy Loom fixture with `gitDirty=true` and no scoped fields, which must
  fail with the existing `Manifest gitDirty must be false...` message;
- a Hook fixture with `gitDirty=true`, `sourceGitDirty=false`, and the approved
  Loom paths, which must still fail the global dirty gate.

- [ ] **Step 3: Run the verifier contract and verify the new scoped-clean case fails**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-verify-release-contract.ps1
```

Expected: FAIL because `Assert-CleanManifest` still rejects the globally dirty
Loom fixture before considering scoped provenance.

- [ ] **Step 4: Implement the exact Loom scope gate**

Add near `Assert-CleanManifest`:

```powershell
$approvedLoomSourcePaths = @("Loom", "scripts/build-release-exes.ps1")
```

Replace the final dirty assertion in `Assert-CleanManifest` with:

```powershell
$sourceDirtyProperty = $Manifest.PSObject.Properties["sourceGitDirty"]
$sourcePathsProperty = $Manifest.PSObject.Properties["sourcePaths"]
$hasSourceDirty = $null -ne $sourceDirtyProperty
$hasSourcePaths = $null -ne $sourcePathsProperty

if ($AppName -eq "Loom" -and ($hasSourceDirty -or $hasSourcePaths)) {
    Assert-True ($hasSourceDirty -and $hasSourcePaths) "Manifest scoped Loom provenance must include both sourceGitDirty and sourcePaths."
    Assert-True ($sourceDirtyProperty.Value -is [bool]) "Manifest sourceGitDirty must be a boolean for a formal Loom release package."

    $actualSourcePaths = @($sourcePathsProperty.Value | ForEach-Object { [string]$_ })
    Assert-Equal ($approvedLoomSourcePaths -join "`n") ($actualSourcePaths -join "`n") "Manifest sourcePaths must exactly match the approved Loom release source paths."
    Assert-Equal $false $sourceDirtyProperty.Value "Manifest sourceGitDirty must be false for a formal Loom release package."
} else {
    Assert-Equal $false ([bool]$Manifest.gitDirty) "Manifest gitDirty must be false for a formal release package."
}
```

The exact ordered comparison rejects missing, additional, duplicate, reordered,
or altered paths. Non-Loom apps always enter the global `gitDirty` branch.

- [ ] **Step 5: Return scoped evidence in formal verifier output**

Build the `Test-ExePackage` result in a variable:

```powershell
$result = [ordered]@{
    app = $AppName
    component = "exe"
    target = [string]$manifest.target
    packageDir = $packageDir
    versionId = [string]$manifest.versionId
    gitHead = [string]$manifest.gitHead
    gitDirty = [bool]$manifest.gitDirty
    exes = @($manifest.exes | ForEach-Object { [string]$_.name })
    zipArtifacts = @($zipArtifacts)
    checksumEntries = $checksumEntries
    smoke = $false
}

if ($AppName -eq "Loom" -and $null -ne $manifest.PSObject.Properties["sourceGitDirty"]) {
    $result["sourceGitDirty"] = [bool]$manifest.sourceGitDirty
    $result["sourcePaths"] = @($manifest.sourcePaths | ForEach-Object { [string]$_ })
}

return $result
```

- [ ] **Step 6: Run formal verifier contracts**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-verify-release-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-contract.ps1
```

Expected: both commands exit `0`. The first prints
`Formal release verifier contract passed.`

- [ ] **Step 7: Commit the formal verifier change**

```powershell
git add -- scripts/verify-release.ps1 scripts/tests/test-verify-release-contract.ps1
git commit -m "feat(release): verify Loom scoped source provenance"
```

### Task 4: Align Migration, Progress, Parity, and Release Documentation

**Files:**
- Modify: `scripts/tests/test-loom-artloom-parity-contract.ps1`
- Modify: `scripts/tests/test-verify-release-contract.ps1`
- Modify: `Loom/docs/MIGRATION_MAP.md:37-54`
- Modify: `docs/loom/progress/MASTER.md:151-156`
- Modify: `docs/loom/analysis/final-artloom-parity-matrix.md:17-36`
- Modify: `docs/architecture/neuro-release-artifact-standard.md:255-270`
- Modify: `docs/architecture/neuro-release-artifact-standard.md:470-485`

- [ ] **Step 1: Add failing documentation contract assertions**

In `test-loom-artloom-parity-contract.ps1`, add paths and reads for the migration
map, progress master, and final parity matrix, then add:

```powershell
Assert-True (-not $migrationMapSource.Contains("- OCR/image capture.")) "Loom migration map must not describe implemented OCR as deferred."
Assert-True (-not $migrationMapSource.Contains("- Embedded Python runtime.")) "Loom migration map must not describe packaged Python as deferred."
Assert-Contains "OCR and packaged ONNX runtime" $migrationMapSource "Loom migration map must record restored OCR packaging."
Assert-Contains "Embedded Python and Python Art" $migrationMapSource "Loom migration map must record restored Python packaging."
Assert-Contains "regenerated after the scoped provenance" $progressMasterSource "Loom progress must require a newly generated release candidate."
Assert-True (-not $progressMasterSource.Contains("Use `loom-desktop-cn-polish-phase38` as the current Loom release candidate.")) "Loom progress must not name an unavailable historical package as current."
Assert-Contains "Release used by this audit" $parityMatrixSource "Final parity matrix must label its package as historical audit evidence."
```

In `test-verify-release-contract.ps1`, add:

```powershell
Assert-Contains "sourceGitDirty" $standard "Release standard must document Loom scoped source cleanliness."
Assert-Contains "sourcePaths" $standard "Release standard must document the exact Loom source scope."
Assert-Contains "repository-wide" $standard "Release standard must preserve repository-wide dirty evidence."
```

- [ ] **Step 2: Run the two documentation-bearing contracts and verify they fail**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-verify-release-contract.ps1
```

Expected: FAIL on the new documentation assertions.

- [ ] **Step 3: Rewrite the migration map completion boundary**

Replace the stale deferred section in `Loom/docs/MIGRATION_MAP.md` with text
that explicitly records:

```markdown
## Completed later parity work

The original headless baseline deferred several ArtLoom runtime surfaces. Later
completed phases restored them through Loom-owned boundaries:

- OCR and packaged ONNX runtime resources.
- Embedded Python and Python Art resources.
- Native image, image conversion, and shared image handling.
- MCP, registry, workflow store, desktop control plane, and Hook Bridge
  compatibility.

## Remaining architectural non-goals

- Gateway provider routing, credentials, browser workers, and relay APIs.
- Platform account, quota, and entitlement logic.

Loom reaches those capabilities through external Gateway and Platform
boundaries. The desktop remains a thin Tauri/React shell over the Loom daemon;
the migration did not copy ArtLoom's monolithic desktop-local backend.
```

Keep the existing completion checklist below this replacement.

- [ ] **Step 4: Correct current-candidate and audit-time wording**

In `docs/loom/progress/MASTER.md`, replace the first next step with:

```markdown
1. Preserve Phase 38 and its package evidence as the completed migration
   baseline; the next Loom release candidate must be regenerated after the
   scoped provenance work is committed and verified.
```

In `docs/loom/analysis/final-artloom-parity-matrix.md`, change:

```markdown
- Latest generated Loom release:
```

to:

```markdown
- Release used by this audit:
```

Change `Latest packaged executables`, `Latest package`, `Latest package sha256`,
`Latest formal verify`, and `Latest release smoke` to the corresponding
`Audit packaged executables`, `Audit package`, `Audit package sha256`,
`Audit formal verify`, and `Audit release smoke` labels. Do not change the
recorded historical paths or hashes.

- [ ] **Step 5: Document scoped Loom provenance in the release standard**

After the formal clean mixed-release paragraph, add:

```markdown
New Loom executable manifests may also contain `sourceGitDirty` and
`sourcePaths`. For Loom only, the formal verifier accepts repository-wide
`gitDirty: true` when `sourceGitDirty: false` and `sourcePaths` exactly equals
`Loom` plus `scripts/build-release-exes.ps1`. The repository-wide value remains
visible and is never rewritten. Historical Loom manifests without the scoped
fields, and all non-Loom manifests, continue to require `gitDirty: false`.
```

In the manifest field discussion, add:

```markdown
`sourceGitDirty` is not a general dirty-manifest bypass. It is an optional Loom
source-provenance field whose approved `sourcePaths` are validated exactly by
the formal verifier. Missing, additional, duplicate, reordered, or altered
paths invalidate the scoped-clean claim.
```

- [ ] **Step 6: Run documentation and release contracts**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-verify-release-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
```

Expected: all three commands exit `0`.

- [ ] **Step 7: Commit the documentation alignment**

```powershell
git add -- Loom/docs/MIGRATION_MAP.md docs/loom/progress/MASTER.md docs/loom/analysis/final-artloom-parity-matrix.md docs/architecture/neuro-release-artifact-standard.md scripts/tests/test-loom-artloom-parity-contract.ps1 scripts/tests/test-verify-release-contract.ps1
git commit -m "docs(loom): align migration and release provenance evidence"
```

### Task 5: Run the Full Loom and Release Regression Suite

**Files:**
- Verify only; no planned source edits.

- [ ] **Step 1: Run all affected root PowerShell contracts**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-verify-release-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-contract.ps1
```

Expected: every command exits `0` with no failed assertion.

- [ ] **Step 2: Run the complete Loom Rust validation**

From `Loom`:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
```

Expected: formatting and compilation succeed; all workspace tests pass with
zero failures.

- [ ] **Step 3: Run desktop TypeScript, web build, and Tauri validation**

```powershell
Push-Location .\Loom\apps\desktop
npm run typecheck
npm run build
Pop-Location
cargo check --manifest-path .\Loom\apps\desktop\src-tauri\Cargo.toml --locked
```

Expected: TypeScript emits no errors, Rsbuild completes, and Tauri Cargo check
exits `0`.

- [ ] **Step 4: Verify scoped dirty reporting without generating a release**

Run:

```powershell
git status --short -- Loom scripts/build-release-exes.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -DryRun -VersionId loom-scoped-provenance-check -NoZip -Apps Loom
```

Expected: dry-run reports only `Loom` and `scripts/build-release-exes.ps1` as
the Loom source scope. Because implementation files are committed task by task,
the final selected paths should be clean even if unrelated subprojects remain
dirty.

- [ ] **Step 5: Re-run isolated existing-package daemon, CLI, and desktop auto-start smoke**

Use `release\Loom\blur-brush` on a dynamically allocated loopback port. Set
temporary `APPDATA`, `LOCALAPPDATA`, and `LOOM_DAEMON_URL` under
`Loom\target\runtime-smoke`; start processes hidden, assert:

```text
/health status = ok
/status status = ready
initialized modules = 8/8
capability count = 4
loom.exe status exit code = 0
loom-desktop.exe remains alive and auto-starts one sibling loom-daemon.exe
```

Stop only the PIDs created by this smoke and verify neither remains.

- [ ] **Step 6: Inspect final diff and repository boundaries**

```powershell
git diff --check
git status --short -- Loom scripts/build-release-exes.ps1 scripts/verify-release.ps1 scripts/tests/test-loom-desktop-shell-contract.ps1 scripts/tests/test-build-release-exes-contract.ps1 scripts/tests/test-verify-release-contract.ps1 scripts/tests/test-loom-artloom-parity-contract.ps1 docs/loom docs/architecture/neuro-release-artifact-standard.md
git log -6 --oneline
```

Expected: no whitespace errors; only intended Loom and Loom-supporting files
appear; no Gateway, Hook, Talk, Tea, or Platform source was changed by this
implementation.

- [ ] **Step 7: Do not generate a formal package in the uncommitted state**

Record in the completion report that the next package must be generated under
`release\Loom` only after the scoped source paths are clean. Do not rewrite or
delete existing release artifacts.
