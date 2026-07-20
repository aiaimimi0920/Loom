# Loom Release Integrity Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Loom Windows release verifier reject internally consistent but tampered desktop/CLI candidates.

**Architecture:** A shared PowerShell layout module owns archive-entry and package-root validation. The standalone verifier composes that module with manifest, artifact, sidecar, and all-file checksum validation. A synthetic fixture contract creates tiny candidates so negative cases are deterministic and do not require compiling Loom.

**Tech Stack:** PowerShell 5.1, .NET `System.IO.Compression`, SHA-256, GitHub Actions, existing Loom release scripts.

---

### Task 1: Add synthetic tamper fixtures and prove current gaps

**Files:**
- Create: `scripts/tests/Test-ReleaseIntegrityTamper.ps1`
- Modify: `.github/workflows/ci.yml`
- Modify: `scripts/tests/Test-GitHubActionsContract.ps1`

- [ ] **Step 1: Create fixture helpers**

Implement helpers that write UTF-8 without BOM, ASCII sidecars, file records,
ZIPs, manifest JSON, and `checksums.sha256`. The fixture must contain:

```text
Loom.exe
runtime/loom-daemon.exe
BUILD_INFO.txt
packages/Loom-integrity-fixture-windows-x64.zip
packages/Loom-integrity-fixture-windows-x64.zip.sha256
packages/Loom-CLI-integrity-fixture-windows-x64.zip
packages/Loom-CLI-integrity-fixture-windows-x64.zip.sha256
manifest.json
checksums.sha256
```

The manifest must use the real schema fields consumed by
`scripts/verify-release.ps1`, with two command provenance records and no
support files.

- [ ] **Step 2: Add explicit pass/fail invocation helpers**

Add:

```powershell
function Invoke-VerifierSuccess {
    param([string]$PackageDir)
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass `
        -File $verifyPath -PackageDir $PackageDir 2>&1
    Assert-Equal 0 $LASTEXITCODE "Valid integrity fixture must pass: $($output -join [Environment]::NewLine)"
}

function Invoke-VerifierFailure {
    param([string]$PackageDir, [string]$ExpectedMessage)
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass `
        -File $verifyPath -PackageDir $PackageDir 2>&1
    Assert-True ($LASTEXITCODE -ne 0) "Tampered fixture unexpectedly passed."
    Assert-True (($output -join [Environment]::NewLine).Contains($ExpectedMessage)) `
        "Expected failure text was not reported: $ExpectedMessage"
}
```

- [ ] **Step 3: Add five behavioral cases**

Generate independent fixtures for:

1. valid candidate;
2. extra root `loom-desktop.exe`, with regenerated generic checksums;
3. CLI ZIP containing `loom.exe` plus `extra.txt`, with matching manifest,
   artifact records, sidecars, and generic checksums;
4. `cliArtifact.zipName` not matching its artifact/path;
5. desktop and CLI sidecars whose contents record the wrong ZIP hash while the
   sidecar files themselves are correctly represented in manifest/checksums.

Also dot-source `LoomReleaseLayout.ps1` and assert that the malformed CLI ZIP
fails before extraction.

- [ ] **Step 4: Run the tamper contract and confirm RED**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1
```

Expected: FAIL because at least the extra-root executable, sidecar-content, and
shared-layout CLI payload cases are not rejected by the current implementation.

- [ ] **Step 5: Wire the contract into Windows CI**

Add the tamper contract to the pre-generated-output validation block in
`.github/workflows/ci.yml`. Add matching static assertions to
`Test-GitHubActionsContract.ps1`.

### Task 2: Harden the shared release layout module

**Files:**
- Modify: `scripts/LoomReleaseLayout.ps1`
- Test: `scripts/tests/Test-ReleaseIntegrityTamper.ps1`
- Test: `scripts/tests/Test-StandaloneReleaseContract.ps1`

- [ ] **Step 1: Add archive entry enumeration**

Add `Get-LoomArchiveFileEntries` using
`System.IO.Compression.ZipFile::OpenRead`. Return normalized archive file
names and reject directory-only or traversal entries.

- [ ] **Step 2: Add desktop-root executable validation**

Add `Assert-LoomDesktopRootExecutableBoundary`. Enumerate only files directly
under the package root with extension `.exe` and require one case-sensitive
name, `Loom.exe`. Throw:

```text
Loom desktop package root must contain exactly one executable named Loom.exe.
```

- [ ] **Step 3: Validate CLI artifact relationships**

In `Get-LoomReleaseLayout` require:

- exactly one manifest artifact with `kind = cli-zip`;
- `cliArtifact.zipName`, `cliArtifact.path`, `bytes`, and `sha256` equal
  the CLI artifact record;
- ZIP leaf name equals `cliArtifact.zipName`;
- actual ZIP byte count and SHA-256 equal manifest values;
- archive entries equal exactly `loom.exe`.

Throw stable messages containing:

```text
Loom CLI artifact metadata mismatch.
Loom CLI ZIP must contain exactly one loom.exe entry.
```

- [ ] **Step 4: Protect extraction destinations**

When `CliExtractRoot` is supplied, reject an existing non-empty directory
before `Expand-Archive`. After extraction, require exactly one file named
`loom.exe`.

- [ ] **Step 5: Run focused contracts to GREEN**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
```

Expected: layout-specific tamper cases pass; sidecar cases may remain RED until
Task 3.

### Task 3: Harden verifier artifact and sidecar validation

**Files:**
- Modify: `scripts/verify-release.ps1`
- Test: `scripts/tests/Test-ReleaseIntegrityTamper.ps1`
- Test: `scripts/tests/Test-StandaloneReleaseContract.ps1`

- [ ] **Step 1: Dot-source the shared layout module**

Load `scripts/LoomReleaseLayout.ps1` near verifier startup and call
`Get-LoomReleaseLayout -PackageDir $packageFullPath` before payload
verification.

- [ ] **Step 2: Add ZIP artifact lookup and naming checks**

Require exactly one `desktop-zip`, one `zip-sha256`, one `cli-zip`, and one
`cli-zip-sha256`. Require names and paths derived from
`manifest.versionId`:

```text
packages/Loom-<versionId>-windows-x64.zip
packages/Loom-CLI-<versionId>-windows-x64.zip
```

- [ ] **Step 3: Parse checksum sidecars**

Add `Assert-ZipChecksumSidecar` that:

1. resolves the ZIP and sidecar records;
2. reads the entire sidecar as ASCII;
3. allows one trailing CRLF but no extra lines;
4. requires lowercase `<sha256>  <zip filename>`;
5. compares the recorded hash with both the actual ZIP hash and the ZIP artifact
   record.

Use the stable failure message:

```text
ZIP checksum sidecar content mismatch for <zip name>.
```

- [ ] **Step 4: Strengthen CLI manifest checks**

Extend `Assert-CliZipPayload` to compare `bytes` and `sha256` in addition
to name/path and reuse `Get-LoomArchiveFileEntries`.

- [ ] **Step 5: Run all release contracts to GREEN**

Run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-GitHubActionsContract.ps1
```

Expected: all pass, including the valid fixture and every tamper case.

### Task 4: Refresh operator and progress documentation

**Files:**
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Modify: `docs/progress/MASTER.md`

- [ ] **Step 1: Document integrity validation**

State that release verification checks exact root executables, ZIP payloads,
manifest/artifact metadata, sidecar content, and all-file checksums.

- [ ] **Step 2: Record Phase 42 final evidence**

Replace stale future wording with:

```text
Candidate: 20260721-single-entry-3d378db
Standalone SHA: 3d378db3a33fd3b5b819eda9dd17d10e6f5c977d
Verifier: 32 files, full smoke passed
Hosted CI/Build Windows/Docker: success
Parent gitlink commit: b1116ef70a437a84615b6343986c6afb9082d20c
```

- [ ] **Step 3: Add Phase 43 status**

Describe the hardening scope and keep main merge, tag, signing, installer, and
runtime cancellation as explicit non-goals.

### Task 5: Validate, package, and publish the hardening

**Files:**
- Generated only under:
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`

- [ ] **Step 1: Run source validation**

Run:

```powershell
git diff --check
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked --manifest-path apps\desktop\src-tauri\Cargo.toml --lib
npm --prefix apps\desktop run typecheck
npm --prefix apps\desktop run build
cargo check --locked --manifest-path apps\desktop\src-tauri\Cargo.toml
```

- [ ] **Step 2: Run all PowerShell contracts**

Run the standalone layout, standalone release, integrity tamper, Actions,
persistence, and concurrency contract scripts.

- [ ] **Step 3: Re-verify the Phase 42 candidate**

Run the hardened verifier against:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260721-single-entry-3d378db
```

Expected: PASS, proving backward readability for the current single-entry
candidate.

- [ ] **Step 4: Commit implementation and build a new clean candidate**

Commit the Phase 43 source. Use a new version id:

```text
20260721-release-integrity-<shortsha>
```

Build under the mandated release root and run
`verify-release.ps1 -RunSmoke`.

- [ ] **Step 5: Push and validate hosted workflows**

Push `feat/single-entry-release`. Dispatch CI and Build Windows for the exact
SHA; let Docker run when its path filters match, otherwise dispatch Docker
manually. Require successful conclusions for all three workflows.

- [ ] **Step 6: Advance only the parent Loom gitlink**

Create one local Neuro commit containing only mode `160000` path `Loom`.
Do not push Neuro. Create a fresh isolated parent clone under
`C:\Users\Public\nas_home\AI\GameEditor\_temp`, initialize the public
Loom submodule, and verify the exact SHA and a clean clone.

