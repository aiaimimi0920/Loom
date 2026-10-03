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

Loom is an independent repository, including when checked out as a Neuro
submodule. The manifest's `sourcePaths` must be exactly `["."]`, relative to
that repository. The retired monorepo scope of `Loom` plus a parent build script
is not accepted. Parent or sibling changes do not redefine Loom's source identity.

## Package layout and integrity

The desktop package has exactly one root executable, `Loom.exe`, and the daemon
at `runtime/loom-daemon.exe`. Daemon-owned support files remain under `runtime`;
optional OCR/package payloads follow the current catalog rather than an old
design's bundled-resource list. Docker remains daemon-first, without the desktop.
The separate CLI ZIP contains exactly one `loom.exe`. CLI artifacts remain local
or workflow evidence under the public-asset policy below.

The verifier binds artifact names, package-relative paths, byte counts and
SHA-256 values to the manifest. ZIP contents must match their declared payload;
extra root executables are rejected even when generic checksums are consistent.
Each ZIP sidecar must match the expected ASCII line containing its lowercase
SHA-256 and filename. CLI extraction refuses stale nonempty destinations.

`scripts/tests/Test-ReleaseIntegrityTamper.ps1` exercises valid synthetic packages
and targeted corruptions; `Test-StandaloneReleaseContract.ps1` and
`Test-StandaloneLayout.ps1` protect the repository and package boundaries. Keep
these contracts passing when changing packaging. A synthetic tamper test does
not replace verification and smoke of the actual release package.

## Plugin boundary failure diagnostics

The Windows candidate failure artifact includes the existing bounded MCP JSON
and `target/runtime-smoke/plugin-boundary/plugin-boundary-diagnostic.json`.
The latter is capped at 4 KiB and records the active WebSocket phase, fixed smoke
request identity, operation budget, phase elapsed time, socket state, received
fragment/message counts, completion flag, exception types, and source line.
It excludes response payloads, exception messages, full paths, and daemon logs.
The original smoke failure is rethrown even if diagnostic writing fails.

The six phases are connect, subscribe send/receive, instantiation receive, and
execute send/receive. The existing 10-second cancellation budget applies to each
operation or received fragment, not to the entire phase or smoke run. Execution
events may therefore span multiple operations. These diagnostics do not retry
Art execution, increase timeouts, or establish the cause of an older failure.

Run `scripts/tests/Test-PluginBoundaryDiagnostics.ps1` in Windows PowerShell 5.1
for real loopback cancellation, fragmentation, close, malformed JSON, unrelated
event, and successful-response coverage. `Test-PluginBoundarySmokeContract.ps1`
checks module wiring and the exact bounded upload paths. The dedicated Plugin
Boundary Diagnostics workflow runs these tests without building, signing, or
publishing a release. They do not replace a packaged-daemon smoke run.

## Packaged QR smoke

`-RunSmoke` includes a QR projection check against the packaged daemon, using
three temporary Ed25519 device identities and an isolated loopback listener.
It verifies invitation confirmation, two image updates, retries, process-loss
recovery, persistent unlink, and source/receiver revocation. The script checks
the executable against the package manifest before starting it, removes its
temporary credentials and state, and retains a bounded JSON result. It requires
Node.js 22.18 or newer; the release workflow pins Node.js 22.22.2.

The same check can run independently:

```powershell
.\scripts\Invoke-LoomQrProjectionSmoke.ps1 -PackageDir .\release\Loom\Vx.y.z
```

This is packaged-daemon protocol evidence. Native Hook windows and cross-machine
HTTPS still require separate acceptance on the actual devices.

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

Automatic publication accepts only canonical uppercase numeric tags such as
`V0.2.1`. The push filter uses GitHub's `[0-9]+` syntax for each version component,
matching the existing publication validator. Suffix tags such as
`V0.2.0-public`, `V0.2.0-sdk`, and prerelease labels do not start this workflow;
they are not separate channels supported by the release pipeline. Manual
dispatch remains available, but its tag must pass the same strict validation.
See the [GitHub filter pattern syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#filter-pattern-cheat-sheet).

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

Windows candidate builds also run for pull requests touching the MCP smoke,
transport/process owners, or their build workflow. A failed Loom smoke exports
only a bounded `mcp-smoke-diagnostic.json` artifact: fixed phase names, UTC
timestamps, process IDs and truncation/rejection counters. At most eight fixture
files, 16 KiB per file and 32 records per file are accepted. Unknown fields,
payloads and reparse-point files are rejected. The broader local failure bundle,
raw logs, command lines and control-plane files are not uploaded by this step.
Phase metadata is diagnostic evidence, not a passing package verification; the
existing protocol assertions and timeouts remain authoritative.

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

The historical `V0.2.0-public` and `V0.2.0-sdk` failures (#40 and #41) stopped at
tag validation because the old broad trigger also matched suffix tags. Do not
rerun or move those tags: a corrected trigger on `main` does not alter their
historical commits or certify any assets uploaded separately. A future automated
release must use a new canonical tag on reviewed source and pass every gate.

## Joint release acceptance

The former Phase 77-79 records mixed implementation history with joint acceptance.
Their cleanup does not complete the remaining exact-package Hook/Loom release
gate. Old shared-dirty-worktree descriptions and reserved candidate IDs are not
current instructions; inspect both repositories and choose unused output paths.

For a joint release, retain evidence for all of the following:

- Reviewed coherent commits in both repositories, with each untracked path
  assigned or preserved deliberately. Formal builds require clean source;
  never bulk-stage or remove unrelated work to manufacture that condition.
- Tested effective-line tooling and strict enforcement: no handwritten file
  above 700 effective lines, no hard-cap waiver, and exact current justification
  for each 501-700 exception. Use the registries rather than old phase counts.
- Composition boundaries, meaningful purpose/invariant comments and coverage of
  public protocol, HTTP, IPC, events, serialization, package and release contracts.
- Security, resource lifetime and performance review of changed modules, regression
  evidence for fixed high/critical issues and leaks, and measurements for sensitive
  paths without unexplained regressions.
- Loom's full Rust, desktop, smoke and release gates from the reviewed source,
  followed by `verify-release.ps1 -RunSmoke -RequireCleanSource` on the exact package.
- Hook's full frontend, browser, Rust, native/runtime and release gates from its
  reviewed source, with the executable and ZIP digests matching provenance.
- New verified artifacts under the required release roots, final commit identity,
  `gitDirty=false` and `sourceGitDirty=false`, plus recorded commands, results,
  current exceptions and actual package paths.

The old Phase 79 RC1 package failed its exact-package verifier even though the
build completed; it remains diagnostic evidence, not an accepted release. A
resolver fix or a later package build does not retroactively validate RC1.
Package existence, headless self-check and an independently passing QR smoke do
not replace full verification or [wall joint acceptance](TILE_WALL.md).

Historical phase records and detailed candidate evidence can be recovered from
Git tag `cleanup-base-20260928`. Keep future verification results with the exact
release artifacts rather than extending a completed implementation diary.

For Art Surface native acceptance, bind the exact Hook/daemon paths and SHA-256
values to startup, pairing/approval, attachment, action/resource/formal result,
600-second resource sampling, same-instance restart recovery and final teardown.
Historical passing pairs cannot validate later source or package bytes. Record
whether `HOOK_NATIVE_ACCEPTANCE=1` isolates native global Delete input; such
test-mode evidence must not be presented as an ungated production-input run.
Do not stop unknown user processes or weaken protocol dispose/cancellation to
make an acceptance run pass.

## Evidence versus publication

A dirty candidate may be retained as runtime evidence, but it is not a formal
publication claim and must not replace an immutable clean release. Any change
to production source, resources, dependencies, packaging, or release tooling
requires a new release ID and regenerated checksums/SBOM/provenance.


## Smoke-process teardown

Framework/Art Store/Hook smoke pins the root handle at launch and retains verified
child process handles before requesting termination. A child must still report the pinned parent's PID and must not predate
that parent. Termination is requested once per retained identity, and all parent
and child exit waits consume one monotonic five-second budget. A real timeout,
failed identity check, enumeration error, or termination/access error remains a
failure; fixed sleeps and PID disappearance do not establish successful teardown.
The existing synchronous CIM/WMI provider calls can outlast that wait budget, so
this is not a hard five-second wall-clock limit on a stalled provider.

`Test-FrameworkSmokeCleanup.ps1` covers delayed exit signaling, genuine timeout,
identity mismatch, access/enumeration failure even during concurrent exit, handle
disposal, and the shared
remaining budget with synthetic process boundaries. The Windows module contract
also retains independent observation handles for its real parent/child fixture,
checks their exit signals and cleans those exact processes even when an assertion
fails. These checks concern temporary smoke processes; they do not establish the
cause of an unrelated MCP connection timeout.
