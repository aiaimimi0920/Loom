# Loom Release Closure Design

Date: 2026-07-18
Status: Approved for implementation planning

## Context

Loom's migration and ArtLoom parity baseline is complete through Phase 38, and
the current source passes Rust workspace checks, Rust tests, desktop typechecking,
desktop web builds, Tauri checks, and an isolated packaged-runtime smoke test.
The remaining work is engineering closure rather than a new product feature.

Three inconsistencies remain:

1. `scripts/tests/test-loom-desktop-shell-contract.ps1` still asserts English
   UI copy that was intentionally replaced by the Phase 35 and Phase 38 Chinese
   desktop localization.
2. `Loom/docs/MIGRATION_MAP.md` still describes OCR and embedded Python as
   deferred even though later completed phases restored and packaged them.
3. Loom release manifests currently expose only repository-wide `gitDirty`.
   In the shared monorepo, unrelated work in Gateway, Hook, Talk, Tea, or other
   subprojects therefore prevents a formal Loom package from being recognized
   as clean even when Loom's actual release inputs are unchanged.

## Goals

- Make the desktop shell contract validate the current Chinese Loom UI.
- Align Loom migration documentation with the implementation that exists now.
- Preserve repository-wide dirty-state evidence in every release manifest.
- Add an auditable Loom-specific source dirty state for formal Loom releases.
- Allow unrelated subproject changes to coexist with a formally clean Loom
  source tree.
- Keep release output under `release/Loom/<versionId>`.
- Preserve existing behavior for every non-Loom release target.

## Non-Goals

- No Loom runtime, daemon API, workflow, OCR, Python, MCP, or desktop behavior
  changes.
- No changes to Hook, Talk, Tea, Gateway, or Platform release acceptance.
- No extraction or duplication of the complete root release framework into
  `Loom/scripts`.
- No formal release package will be generated while the selected Loom release
  inputs are uncommitted.
- No attempt will be made to reinterpret historical Loom manifests that do not
  contain the new scoped provenance fields.

## Considered Approaches

### 1. Documentation and contract-only repair

This would fix the stale Chinese UI assertions and migration map, but it would
leave formal Loom releases blocked whenever any unrelated monorepo path is
dirty. It does not satisfy the repository's independent-subproject workflow.

### 2. Add scoped Loom source provenance

Keep `gitDirty` as repository-wide evidence and add Loom-specific
`sourceGitDirty` plus `sourcePaths`. Formal Loom verification uses the scoped
field when present, while historical manifests and all other apps retain the
old rule. This is the selected approach because it solves the actual release
problem without changing the meaning of existing fields or duplicating the
release system.

### 3. Move all Loom release tooling under `Loom/scripts`

This provides maximal physical isolation, but it duplicates build, manifest,
hash, archive, verification, and smoke behavior already maintained at the
repository root. The added maintenance surface is not justified by the current
problem.

## Provenance Model

### Existing repository state

`gitDirty` remains unchanged and continues to mean: the Git worktree has at
least one staged, unstaged, or untracked change anywhere in the repository.

### Loom source state

New Loom manifests record:

```json
{
  "gitDirty": true,
  "sourceGitDirty": false,
  "sourcePaths": [
    "Loom",
    "scripts/build-release-exes.ps1"
  ]
}
```

The two Loom release source paths are:

- `Loom`: all Loom source, lockfiles, resources, desktop code, tests, and
  Loom-owned documentation.
- `scripts/build-release-exes.ps1`: the shared builder that selects commands,
  executables, support files, archive layout, and manifest contents for Loom.

The smoke and verifier scripts are validation tools rather than package inputs,
so their dirty state does not change the provenance of an already built Loom
archive. Their own contract tests still protect their behavior.

`sourceGitDirty` is computed with `git status --porcelain -- <sourcePaths>` and
therefore includes staged, unstaged, and untracked files. A Git command failure
produces an unknown value, not a clean value.

The formal verifier does not trust an arbitrary manifest-provided scope. For a
new scoped Loom manifest, `sourcePaths` must contain exactly `Loom` and
`scripts/build-release-exes.ps1`. Missing paths, additional paths, duplicates,
or a changed path set invalidate the scoped-clean claim.

## Manifest and Build Information

For Loom builds, `scripts/build-release-exes.ps1` will:

- declare optional `sourcePaths` in the Loom app catalog entry;
- compute repository-wide `gitDirty` once as before;
- compute `sourceGitDirty` from the Loom source paths;
- write `gitDirty`, `sourceGitDirty`, and `sourcePaths` to `manifest.json`;
- write both dirty states and the scoped paths to `BUILD_INFO.txt`;
- expose the scoped paths in dry-run output so contract tests can validate the
  intended release boundary.

Apps without `sourcePaths` will keep their current manifest and verification
behavior. The manifest schema version remains `1` because all new fields are
optional and backward-compatible.

## Formal Verification

`scripts/verify-release.ps1` will use the following rule:

1. For a Loom manifest containing `sourceGitDirty`, formal source cleanliness
   requires `sourceGitDirty` to exist and equal `false`, and requires
   `sourcePaths` to match the approved Loom release source paths exactly.
2. For a historical Loom manifest without `sourceGitDirty`, verification falls
   back to the existing requirement that `gitDirty` equal `false`.
3. Every non-Loom manifest continues to require `gitDirty` equal `false`.
4. A partially present scoped pair, null or non-boolean scoped value, duplicate
   path, or altered path set is not clean.

Verifier output will continue to report `gitDirty` and will additionally report
Loom's scoped provenance fields when available. This makes it explicit when a
formal Loom archive was built in a repository that contained unrelated work.

## Desktop Contract Repair

The desktop shell contract will validate the current user-visible Chinese
surface instead of obsolete English copy. Required strings include:

- `本地工作台`
- `总览`
- `Art 注册表`
- `工作流管理`
- `截图同步`
- `工作流工作台`
- `启动截图同步`
- `停止截图同步`
- `智能体`
- `运行记录`
- `设置`
- `关于`

Existing structural assertions for Tauri configuration, daemon proxy commands,
loopback URLs, settings links, and visual CSS primitives remain in place.

## Documentation Repair

`Loom/docs/MIGRATION_MAP.md` will distinguish the original headless baseline
from the completed later parity phases. It will state that Loom now includes:

- OCR and packaged ONNX runtime resources;
- embedded Python and Python Art resources;
- native image and shared image handling;
- MCP, registry, workflow store, and Hook Bridge compatibility.

The remaining architectural exclusions are Gateway provider routing,
credentials, browser workers, relay APIs, and Platform account, quota, and
entitlement logic. Loom continues to implement those only through external
boundaries.

The release artifact standard will document that scoped Loom source cleanliness
does not hide repository-wide dirtiness and applies only to new Loom manifests.

`docs/loom/progress/MASTER.md` will stop presenting the unavailable Phase 38
artifact name as the current distributable release candidate. It will preserve
Phase 38 as completed historical evidence and state that the next candidate
must be regenerated after the scoped provenance work is committed.

`docs/loom/analysis/final-artloom-parity-matrix.md` will describe its recorded
June 18 package as the release used by that audit rather than the repository's
perpetual latest package. The parity conclusions remain historical evidence and
are not reopened.

## Testing Strategy

Implementation follows a test-first sequence:

1. Extend release contract fixtures with scoped Loom provenance cases and show
   that the current verifier cannot accept them correctly.
2. Update the stale desktop shell contract and reproduce its current failure
   against the Chinese source before changing the assertions.
3. Implement scoped dirty-state generation and verification.
4. Run targeted PowerShell contracts:
   - `test-loom-desktop-shell-contract.ps1`
   - `test-loom-artloom-parity-contract.ps1`
   - `test-build-release-exes-contract.ps1`
   - `test-verify-release-contract.ps1`
   - `test-build-release-contract.ps1`
5. Run Loom validation:
   - `cargo fmt --all -- --check`
   - `cargo check --workspace --all-targets --locked`
   - `cargo test --workspace --locked`
   - desktop `npm run typecheck`
   - desktop `npm run build`
   - desktop Tauri `cargo check --locked`
6. Re-run an isolated existing-package daemon, CLI, and desktop auto-start smoke
   if release-related code changes affect runtime smoke assumptions.

## Acceptance Criteria

- The desktop shell contract passes using current Chinese UI copy.
- The broad ArtLoom parity contract remains green.
- Loom dry-run output declares exactly the intended source paths.
- A synthetic new Loom manifest with `gitDirty=true` and
  `sourceGitDirty=false` passes the formal cleanliness gate.
- A Loom manifest with `sourceGitDirty=true` fails the formal gate.
- A Loom manifest with missing, additional, duplicate, or altered `sourcePaths`
  fails the formal gate.
- A historical dirty Loom manifest without `sourceGitDirty` still fails.
- Dirty manifests for non-Loom apps still fail.
- `MIGRATION_MAP.md` no longer claims implemented OCR or embedded Python work is
  deferred.
- Loom progress and parity documents no longer describe historical or missing
  package names as the current release candidate.
- No source file in another subproject is modified.
- No release is described as formally clean until the selected Loom source
  paths are committed and `sourceGitDirty=false` is freshly verified.

## Release Follow-Up

After implementation, tests, review, and source commit, the next Loom package
should be generated through the existing root release entrypoint and written to
`release/Loom/<versionId>`. Formal verification must report the repository-wide
and Loom-scoped states separately. Package generation is a follow-up operation,
not part of the uncommitted implementation batch.
