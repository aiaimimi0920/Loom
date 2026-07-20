# Loom Standalone Repository Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish Loom as a self-contained public repository and replace the Neuro parent copy with a pinned Loom Git submodule without changing Loom runtime behavior.

**Architecture:** Build and validate an isolated staging repository under Neuro/_temp, consolidate all Loom-owned docs/scripts/actions there, then initialize and publish that repository. Only after the published commit is reproducible is the parent Loom directory replaced by a 160000 gitlink; parent integration and standalone history are committed separately.

**Tech Stack:** Rust/Cargo, Tauri v2, React/TypeScript, PowerShell, Git submodules, GitHub Actions, Docker.

## Execution record

Publication and runtime validation closed on 2026-07-21. The public repository
is `https://github.com/aiaimimi0920/Loom`; the initial standalone commit is
`4749f116565a61fbaafa7188796a596d0ef542bb`, the clean release source is
`161b8aaa2dd8f31016eb1910850ac7fbf5bc65b0`, and the final runtime-test head is
`a3b081c869cae4a8b8115759276acb5ce6985acc`. The Neuro integration baseline is
`86105d555a01ad31b00e1328a011eb0f12828c18`.

The verified backup is
`C:\Users\Public\nas_home\AI\GameEditor\_temp\Loom-standalone-backup-20260720-195938-be4bbb7b`.
The formal package is
`C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260721-standalone-161b8aa`,
and its ZIP SHA-256 is
`962addde00416858138101722399f394ede011ae8a388d846fef58a5171b4ab4`.
Hosted CI, Build Windows, and Docker succeeded for `a3b081c`; no release tag was
created. The independent final review then verified parent commit
`724a26f5b2821c951f411bab60de4facb948aa0e`, a mode `160000` gitlink to
`3ebc74f5b713892e0418182cc60f88f6d9bed12b`, and the clean clone
`C:\Users\Public\nas_home\AI\GameEditor\Neuro\_temp\Neuro-loom-submodule-verification-724a26f`.
Task 8 Step 5 is complete.

---

### Task 1: Create the isolated standalone staging tree

**Files:**
- Source: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom`
- Source: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\docs\loom`
- Create: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\_temp\Loom-standalone-work`
- Create: `scripts/tests/Test-StandaloneLayout.ps1`

- [x] **Step 1: Verify the backup and source boundary**

Run:

~~~powershell
$backup = Get-Content -Raw -Encoding UTF8 .\_temp\Loom-standalone-backup-20260720-195938-be4bbb7b\backup-manifest.json | ConvertFrom-Json
if (-not $backup.verified) { throw "Loom backup is not verified." }
git status --porcelain --untracked-files=all -- Loom docs/loom
~~~

Expected: backup is verified and the only Loom change is this migration plan commit.

- [x] **Step 2: Copy the source into a new staging directory**

Use PowerShell/robocopy with these exact excluded paths:

~~~text
target
apps\desktop\node_modules
apps\desktop\dist
apps\desktop\src-tauri\target
~~~

Fail if `_temp\Loom-standalone-work` already exists. Do not delete an existing directory.

- [x] **Step 3: Consolidate parent-owned Loom docs**

Copy:

~~~text
docs\loom\analysis    -> _temp\Loom-standalone-work\docs\analysis
docs\loom\plan        -> _temp\Loom-standalone-work\docs\plan
docs\loom\progress    -> _temp\Loom-standalone-work\docs\progress
docs\loom\superpowers -> _temp\Loom-standalone-work\docs\superpowers
~~~

For any duplicate path, compare SHA-256 first. Identical files are accepted;
different files stop the task for an explicit merge.

- [x] **Step 4: Add the standalone layout contract**

Create `scripts/tests/Test-StandaloneLayout.ps1`. It must fail when:

- required root files/directories are absent;
- generated cache directories are tracked inputs;
- docs/progress/MASTER.md is absent;
- any current operator README command begins with `.\Loom\`;
- Cargo package repository metadata still points to the Neuro monorepo;
- desktop sibling-daemon code contains `C:\release\Loom`.

- [x] **Step 5: Run the layout contract and record the expected red state**

Run:

~~~powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneLayout.ps1
~~~

Expected: failure identifying missing standalone metadata/path changes.

### Task 2: Make metadata, docs, and runtime paths standalone-safe

**Files:**
- Create: `.gitignore`
- Create: `LICENSE`
- Create: `CONTRIBUTING.md`
- Create: `SOURCE_PROVENANCE.md`
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/progress/MASTER.md`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: `scripts/tests/Test-StandaloneLayout.ps1`

- [x] **Step 1: Add standalone repository metadata**

Create ignore rules for Cargo, npm, Tauri, release output, evidence, editor files,
and temporary directories. Add MIT and Apache-2.0 license notices consistent
with Cargo metadata. Add contributor commands for Rust, desktop, Tauri, package,
and Docker validation.

- [x] **Step 2: Add source provenance**

Record:

~~~text
Parent repository: https://github.com/aiaimimi0920/Neuro
Parent baseline commit: 96c38f7119b2ea67b71edc124c19fea6c3c13584
Verified backup manifest:
C:\Users\Public\nas_home\AI\GameEditor\Neuro\_temp\Loom-standalone-backup-20260720-195938-be4bbb7b\backup-manifest.json
~~~

Do not copy credentials or parent dirty-worktree details.

- [x] **Step 3: Update package/repository links**

Set Cargo repository metadata and README badges/links to
`https://github.com/aiaimimi0920/Loom`. Rewrite operator commands to
run from the standalone root. Document an explicit `-OutputRoot` option
for Neuro parent releases.

- [x] **Step 4: Remove the hard-coded desktop release fallback**

Update sibling-daemon resolution to check, in order:

1. explicit environment/configuration override;
2. sibling `loom-daemon.exe` beside the running desktop executable;
3. development target path derived from the repository root.

Do not retain `C:\release\Loom`.

- [x] **Step 5: Run the layout contract**

Expected: all standalone layout/path assertions pass.

### Task 3: Add self-contained release and verification tooling

**Files:**
- Create: `scripts/build-release.ps1`
- Create: `scripts/verify-release.ps1`
- Create: `scripts/smoke-release.ps1`
- Create: `scripts/tests/Test-StandaloneReleaseContract.ps1`
- Adapt: existing focused smoke and contract scripts

- [x] **Step 1: Write a failing standalone release contract**

The contract must parse the build, verify, and smoke scripts and assert:

- repo root comes from PSScriptRoot;
- output root is parameterized;
- only Loom binaries/resources are cataloged;
- manifest source paths are standalone-relative;
- no parent Hook/Tea/Platform release branches are present;
- default output stays under the standalone repository;
- an explicit parent output path is accepted.

- [x] **Step 2: Run the contract and verify it fails**

Expected: missing standalone build/verify/smoke scripts.

- [x] **Step 3: Extract the Loom-only build script**

Implement this interface:

~~~powershell
param(
  [string]$VersionId,
  [string]$OutputRoot = ".\release\Loom",
  [switch]$NoZip,
  [switch]$DryRun
)
~~~

Build `loom-cli`, `loom-daemon`, and
`loom-desktop`; copy embedded Python/OCR payloads; write UTF-8 no-BOM
manifest, ASCII checksums, logs, ZIP, and SHA-256 sidecar.

- [x] **Step 4: Extract the Loom-only verifier**

Implement:

~~~powershell
param(
  [Parameter(Mandatory = $true)][string]$PackageDir,
  [switch]$RunSmoke
)
~~~

Verify all files except checksums.sha256 have exactly one checksum entry,
manifest provenance is clean, the payload ZIP matches executable/support files,
and the three executables exist.

- [x] **Step 5: Extract the Loom-only unified smoke**

Use only standalone Loom files and existing focused smokes. Preserve local
planner, bearer, desktop sibling-daemon, OCR, Python, MCP, workflow,
persistence, Gateway, and bounded concurrency coverage.

- [x] **Step 6: Run the release contract and builder dry run**

Run:

~~~powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -VersionId standalone-dry-run -OutputRoot .\release\Loom -DryRun
~~~

Expected: contract passes and dry-run JSON lists only standalone Loom inputs.

### Task 4: Add GitHub Actions and workflow contracts

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/build-windows.yml`
- Create: `.github/workflows/release-tag.yml`
- Create: `.github/workflows/docker.yml`
- Create: `scripts/tests/Test-GitHubActionsContract.ps1`

- [x] **Step 1: Write and run the failing workflow contract**

Assert workflow names/triggers, explicit permissions, checkout/setup versions,
locked Cargo commands, npm ci, Tauri check, artifact checks, V tag validation,
GitHub Release usage, and absence of PAT/token literals.

Expected: failure because workflow files do not yet exist.

- [x] **Step 2: Add ci.yml**

Use Windows full validation and Linux Rust/Docker-compatible validation. Use
Rust 1.95.0, Node 22, actions/checkout@v5, actions/setup-node@v6,
Swatinem/rust-cache@v2, and no secrets.

- [x] **Step 3: Add build-windows.yml**

Build with `scripts/build-release.ps1`, verify the candidate, and upload
the candidate directory with actions/upload-artifact@v6.

- [x] **Step 4: Add release-tag.yml**

Accept `V\d+\.\d+\.\d+`, build/verify/smoke the package, and publish
ZIP plus SHA-256 through softprops/action-gh-release@v3 using only GITHUB_TOKEN.

- [x] **Step 5: Add docker.yml**

Run `docker build -t loom-ci .` on Dockerfile or Rust source changes
and manual dispatch. Use contents read.

- [x] **Step 6: Run the workflow contract**

Expected: pass with zero credential literals.

### Task 5: Validate and initialize the standalone repository

**Files:**
- All files under `_temp\Loom-standalone-work`
- Create: standalone `.git`

- [x] **Step 1: Run source validation**

Run format, workspace all-target check, workspace tests, desktop npm ci,
typecheck/build, Tauri check, and all PowerShell contracts.

Expected: the same 269-test baseline or a larger intentional count, with zero
failures.

- [x] **Step 2: Build and smoke a standalone candidate**

Run build-release.ps1, verify-release.ps1, smoke-release.ps1, persistence,
Gateway, and concurrency smokes against `release\Loom\$versionId`.
Confirm zero candidate processes and token leakage.

- [x] **Step 3: Initialize Git and commit the baseline**

Use main as the initial branch. Stage only standalone-owned files. Verify no
generated release/target/node_modules/evidence files are staged.

Commit:

~~~text
feat: establish standalone Loom repository
~~~

- [x] **Step 4: Validate a second clean clone**

Clone to `_temp\Loom-standalone-clean-clone` and run layout/workflow
contracts, Cargo check/tests, desktop validation, and release dry run.

### Task 6: Create and publish the GitHub repository

**Files:**
- Standalone Git config only; no credential files

- [x] **Step 1: Verify safe authentication**

Use an existing Git credential helper or a newly rotated process-only token.
Do not use the PAT pasted in the conversation. Stop before remote creation when
safe authentication is unavailable.

- [x] **Step 2: Recheck repository absence**

Run an unauthenticated read for
`https://github.com/aiaimimi0920/Loom.git`. Continue only when it does
not exist.

- [x] **Step 3: Create and push the public repository**

Create `aiaimimi0920/Loom` with main as default branch and no
server-generated files. Add origin, push main, and compare local/remote commit
IDs.

- [x] **Step 4: Verify Actions registration**

Confirm ci, build-windows, release-tag, and docker workflows are registered. Do
not create a release tag unless separately requested.

### Task 7: Replace the parent Loom directory with the submodule

**Files:**
- Modify: `.gitmodules`
- Replace: parent `Loom` ordinary tree with a 160000 gitlink
- Modify/create: parent Loom pointer docs or wrapper only as required

- [x] **Step 1: Verify the published commit and backup**

Record the remote commit ID. Recheck the backup and ensure no process runs from
the live Loom path.

- [x] **Step 2: Preserve the live directory without deleting it**

Move the current live Loom directory to a unique
`_temp\Loom-parent-pre-submodule-*` path on the same volume. Do not
remove it during this migration.

- [x] **Step 3: Replace the parent index entry**

Remove Loom from the parent index only, add the .gitmodules entry, and clone the
published repository into Loom as a submodule pinned to the exact commit.

- [x] **Step 4: Consolidate parent docs and wrapper**

After confirming standalone docs are published, remove duplicate docs/loom or
replace them with one pointer document. Add a parent release wrapper only if
needed. Do not edit already-dirty shared scripts from parallel work.

- [x] **Step 5: Commit parent integration**

Stage only .gitmodules, the Loom gitlink, and approved Loom pointer
docs/wrapper.

Commit:

~~~text
chore(loom): track standalone repository
~~~

### Task 8: Final reproducibility and handoff

**Files:**
- Standalone README/docs/progress
- Parent submodule pointer/integration docs
- GitHub repository metadata

- [x] **Step 1: Verify the parent gitlink and clean clone**

Require `git ls-tree HEAD Loom` mode 160000 at the published commit.
Create an isolated parent clone, initialize the Loom submodule, and verify Loom
HEAD.

- [x] **Step 2: Re-run standalone CI-equivalent gates**

Run all commands from the submodule root. Verify the release can target the
parent release/Loom directory by explicit output parameter.

- [x] **Step 3: Verify GitHub Actions**

Confirm main-branch CI/build runs complete. Record workflow URLs and artifact
names. Report external runner limitations separately from local correctness.

- [x] **Step 4: Update migration/progress documentation**

Record standalone URL, baseline commit, parent gitlink commit, backup path,
release path, Actions, evidence, and remaining pre-existing Clippy debt.

- [x] **Step 5: Request final review**

Review standalone and parent commits independently, then run final status checks
showing that Loom submodule and parent Loom integration paths are clean.
