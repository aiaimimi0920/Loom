# Release Provenance

A publishable Loom release must come from a clean Git worktree.

Before building, the exact release tag or manually requested ref must pass the
lockfile inventory contract and the configured OSV vulnerability gate:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-DependencySecurityScan.ps1
```

```powershell
.\scripts\build-release.ps1 `
  -VersionId Vx.y.z `
  -OutputRoot .\release\Loom `
  -RequireCleanSource

.\scripts\verify-release.ps1 `
  -PackageDir .\release\Loom\Vx.y.z `
  -RunSmoke `
  -RequireCleanSource
```

The clean-source gate runs before build output is created. Formal manifests
must record `gitDirty=false` and `sourceGitDirty=false`.

`.github/workflows/release-tag.yml` calls the reusable dependency security
workflow first and makes publication depend on that job. For manual dispatch it
passes the requested tag, not the workflow's default branch. The scan produces
an OSV SARIF artifact for the checked ref; a prior scan of another commit is not
release evidence. See `docs/DEPENDENCY_SECURITY.md` for inventory, triage, and
temporary exception rules.

Runs for the same effective tag share a non-cancelling concurrency group. Before
building, the workflow refuses any existing draft or published GitHub release
for that tag. This prevents two runs from uploading into or replacing the same
release. A failed deterministic build is fixed under a new version tag rather
than by moving a published tag.

## Public Release subjects and private evidence

The public GitHub Release contains the Windows desktop ZIP and sidecar plus the
Plugin SDK ZIP and sidecar. GitHub also shows its automatic `Source code (zip)` and
`Source code (tar.gz)` links for the tag.

The build still generates and verifies the CLI ZIP, CycloneDX/SPDX SBOMs,
`provenance/build-provenance.json`, `manifest.json`, and `checksums.sha256`. The CLI
and metadata remain local or workflow evidence and are not uploaded as public
Release assets.

The manifest records source commit, target, exact build commands, file sizes and
hashes, SDK protocol/schema metadata, SBOM records, provenance record, and ZIP
subjects. `checksums.sha256` covers every release file except itself.

## GitHub attestations

Tag releases use GitHub OIDC with `actions/attest-build-provenance@v2` and
`actions/attest-sbom@v2`. Docker builds use Buildx provenance/SBOM and a Trivy
high/critical vulnerability gate.

## Draft, verification, and publication

GitHub publication is draft-first. After all source, dependency, build, smoke,
and attestation gates pass, the commit-pinned `softprops/action-gh-release`
uploads the complete asset set into one draft. Trusted repository code then
obtains that exact draft by its release ID and verifies:

- the release is still a draft for the requested tag;
- no expected subject is missing and no unexpected asset is present;
- every remote asset byte count matches the local verified file;
- every GitHub-provided SHA-256 asset digest matches a streaming local digest.

Only that final verifier changes `draft` to `false`. If a later step fails, the
workflow deletes only a matching draft identified by the creating step; it never
deletes or rewrites a published release. A partial draft without a trusted
release ID is left for human inspection rather than deleted heuristically. This
sequence is compatible with repository-level immutable releases.

## Failure recovery

`.github/workflows/release-recovery.yml` observes completed Release Tag runs
from the trusted default branch. An unsuccessful run creates or updates one
issue containing the run, attempt, commit, ref, failed jobs, and failed steps.
A successful re-run closes the same issue.

Automatic recovery is bounded to one failed-jobs re-run and only when GitHub
reports failures exclusively in checkout, Node/Rust setup, Rust cache, or the
read-only publication preflight. Dependency security, compilation, tests, smoke,
attestations, draft upload, asset comparison, cancellation, timeout, and publish
failures never auto-retry. After review, an operator may retry an approved
transient boundary with:

```powershell
gh run rerun <run-id> --failed
```

Do not use a re-run to bypass a reproducible defect or security finding. Fix the
cause, rerun the affected local/CI gates, and create a new version tag when the
source commit must change.

## Evidence versus publication

A dirty candidate may be retained as runtime evidence, but it is not a formal
publication claim and must not replace an immutable clean release. Any change
to production source, resources, dependencies, packaging, or release tooling
requires a new release ID and regenerated checksums/SBOM/provenance.
