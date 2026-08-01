# Phase 67: Pluginized Art frameworks

## Status

Phase complete after a post-completion release audit. The Loom host,
package-backed framework registry, generic framework process protocol, sample
packages, Hook capability protocol, independently authored third-party
no-source-change smoke, and release verification are implemented and
validated. The audit found and removed release-only leakage and false-positive
test conditions that the original `final2` evidence did not cover.

## Why this phase exists

Phases 45 through 66 proved that Loom can install and execute representative
Arts for the six current framework IDs:

- `cli_wrapper`
- `cloud_api`
- `script`
- `python_art`
- `mcp`
- `workflow`

That proof is not enough for the final product boundary. The current code still
documents and implements several frameworks as built-in/default-installed
capabilities, and parts of the execution behavior remain inside the Loom host.
The target is a true plugin model:

- frameworks are optional packages;
- Art nodes are optional packages;
- a third-party author can build, package, install, and run an Art without
  changing Loom source;
- the same author does not need Hook source access;
- Hook renders dynamic Art capabilities generically.

## Baseline restore point

Before this phase, local annotated tags were created:

| Repository | Tag | Commit |
| --- | --- | --- |
| Loom | `框架修改前的最后一个版本` | `a8e3df0712bcaa4ba640d8848cf82d2271582054` |
| Hook | `框架修改前的最后一个版本` | `a86272a5b06e3b3f5a92d01dda6be138ab6e087f` |

Use these tags as the rollback boundary if the pluginization work needs to be
abandoned or restarted.

## Plan

Detailed implementation plan:

```text
docs/superpowers/plans/2026-08-01-loom-pluginized-art-frameworks.md
```

## Scope

In scope:

- package-backed framework install state;
- framework package manifest and ZIP format;
- generic external framework process execution protocol;
- six repo-owned framework packages built outside the default release payload;
- six repo-owned sample Art packages built outside the default release payload;
- Hook generic capability rendering for plugin Arts;
- release and smoke guards proving a third party can install without host
  source changes.

Out of scope for this phase:

- hosted public marketplace operations;
- code signing and trust policy beyond local checksum/manifest validation;
- remote payment/licensing;
- cloud provider credential UI beyond existing framework/Art manifest
  declarations.

Those boundaries describe Phase 67 only. Phase 68 subsequently adds the public
Plugin SDK and Schemas, publisher-qualified identity, Ed25519 trust/revocation,
credential management, immutable rollback/recovery hardening, diagnostics,
malicious-package CI, SBOM/provenance assets, and the Desktop security surface.
See [Phase 68](phase-68-art-plugin-platform-hardening.md). Hosted marketplace
operations, payment/licensing, and a complete AppContainer/namespace sandbox
remain outside both phases.

## Progress checklist

- [x] Task 1: Add source-contract guards before runtime changes.
- [x] Task 2: Define framework package manifests and explicit installed state.
- [x] Task 3: Implement framework package install, disable, upgrade, and uninstall.
- [x] Task 4: Add the generic external framework execution protocol.
- [x] Task 5: Convert the six sample frameworks into independent packages.
- [x] Task 6: Convert the six sample Arts into external Art packages.
- [x] Task 7: Make Hook fully capability-driven for plugin Arts.
- [x] Task 8: Add end-to-end plugin boundary smoke.
- [x] Task 9: Update documentation and remove default-build/resource leakage.
- [x] Task 10: Build final Loom and Hook releases.

## Task 1 RED evidence

Command:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtPluginBoundaryContract.ps1
```

Observed exit code: `1`

Observed failure:

```text
Optional Art frameworks must not be installed by default.
```

This is the expected RED result: the current `framework.rs` still declares
`BUILT_IN_FRAMEWORKS`, so the source-contract guard correctly rejects the
pre-pluginized host implementation.

## Task 2 RED evidence

Command:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1
```

Observed exit code: `1`

Observed failure:

```text
framework-packages directory is required.
```

This is the expected RED result before the six external framework package
manifests are added.

## Task 5 RED evidence

Command:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1
```

Observed exit code: `1`

Observed failure:

```text
Independent framework package build script is required.
```

The source manifests alone are intentionally insufficient; the runtime host
and independent ZIP build path must exist before Task 5 can pass.

## Task 5 verification

Implemented and verified:

- Added a standalone `framework-packages/runtime-host` Cargo package. It is
  outside the Loom workspace and is not part of the default Loom build.
- The runtime host implements the external process boundary generically: it
  validates the requested framework ID, loads an Art package's
  `art.runtime.json`, invokes that Art-owned runtime entry, and normalizes
  text/JSON/error output into `loom.framework.v1` responses.
- Added `scripts/Build-LoomArtFrameworkPackages.ps1`, which explicitly builds
  the host, stages all six manifests and runtime entries, emits six ZIPs,
  SHA-256 sidecars, and a JSON build summary.
- Extended the package contract test to validate ZIP contents and hashes, not
  just source manifests.

Verification commands and results:

```text
powershell -NoProfile -ExecutionPolicy Bypass \
  -File .\scripts\Build-LoomArtFrameworkPackages.ps1 \
  -OutputRoot .\.loom-art-store-data\frameworks \
  -Configuration Release
                                                -> 6 ZIPs built
powershell -NoProfile -ExecutionPolicy Bypass \
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1 \
  -ArtifactRoot .\.loom-art-store-data\frameworks
                                                -> passed for 6 manifests and ZIPs
```

## Task 2 verification

Implemented and verified:

- Added `FrameworkPackageManifest`, runtime-entry, and execution-contract
  types using `loom.framework.v1`.
- Removed the built-in framework default set. A fresh control plane now reports
  all six frameworks as `installed=false`, `enabled=false`, and `ready=false`.
- Framework status now exposes `enabled`, `version`, and `runtimeDir` and only
  reports a framework as installed when its persisted state and package
  manifest are both present.
- Added six source package manifests under `framework-packages/`.

Verification commands and results:

```text
cargo fmt --all -- --check                       -> passed
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry framework -- --nocapture
                                                -> 11 passed, 0 failed
powershell -NoProfile -ExecutionPolicy Bypass \
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1
                                                -> passed for 6 manifests
```

## Task 3 verification

Implemented and verified:

- Framework packages are installed atomically under
  `<control-plane>/frameworks/<id>/` after manifest, platform, protocol, entry,
  and Art execution schema validation.
- Package replacement supports upgrades without leaving a partially extracted
  directory; unsafe ZIP paths and mismatched manifest IDs are rejected.
- Persisted framework state now records `version` and `enabled`. Enable,
  disable, uninstall, and upgrade operations update the same package-backed
  state used by readiness checks.
- Added daemon routes for direct package install, enable, disable, upgrade, and
  uninstall while retaining the old per-ID install route as a store-backed
  package install.
- The local Art Store now names and serves framework ZIPs as framework packages.

Verification commands and results:

```text
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry -- --nocapture
                                                -> 67 passed, 0 failed
cargo test --manifest-path .\Cargo.toml -p loom-daemon \
  framework_package_routes_cover_install_upgrade_disable_enable_uninstall -- --nocapture
                                                -> passed
cargo test --manifest-path .\Cargo.toml -p loom-daemon --lib -- --nocapture
                                                -> 172 passed, 0 failed
npm run typecheck --prefix .\apps\desktop       -> passed
npm test --prefix .\apps\desktop               -> 56 passed, 0 failed
```

## Task 4 verification

Implemented and verified:

- Added `loom.framework.v1` stdin/stdout request and response contracts in
  `crates/loom_tool_registry/src/framework_process.rs`.
- Added the generic `ToolExecution::FrameworkArt { framework }` execution kind;
  no third-party framework-specific execution enum variants are required.
- Installed Art packages now persist `metadata.artPackage.dir`, allowing the
  host to pass the package resource directory without embedding Art-specific
  branches in the framework broker.
- The broker resolves the framework package manifest, launches its declared
  process, sends one JSON request, enforces the 120-second production timeout,
  preserves structured framework errors, and returns output/candidates/cache
  through the existing tool result channel.
- Daemon error mapping now preserves package-not-found, protocol, timeout, and
  framework-provided failure details for Hook-facing callers.

Verification commands and results:

```text
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry framework_process -- --nocapture
                                                -> 5 passed, 0 failed
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry -- --nocapture
                                                -> 73 passed, 0 failed
cargo test --manifest-path .\Cargo.toml -p loom-daemon --lib -- --nocapture
                                                -> 172 passed, 0 failed
```

## Acceptance checklist

- [x] Default Loom release contains no optional Art framework runtime package.
- [x] Fresh control-plane root starts with zero installed optional frameworks.
- [x] Framework installation is package-backed rather than a built-in flag flip.
- [x] Framework disable/enable/upgrade/uninstall are supported.
- [x] Art disable/enable/upgrade/uninstall are supported.
- [x] Six sample framework packages install and execute successfully.
- [x] Six sample Art packages install and execute successfully.
- [x] A temporary third-party framework package installs and executes.
- [x] A temporary third-party Art package installs and executes.
- [x] Hook has no production branch on sample Art IDs.
- [x] Loom has no production branch on sample Art IDs outside fixtures/tests/docs.
- [x] `verify-release.ps1 -RunSmoke` includes the plugin boundary smoke.
- [x] Final Loom release exists under
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`.

## Task 6 RED evidence

Added the sample Art package contract before implementation:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-LoomSampleArtPackageContract.ps1
```

Expected RED result:

```text
Sample Art package source directory is required:
...\Loom\art-packages\samples
EXIT_CODE=1
```

The failure is intentional: the six independent Art package source
directories and their builder do not exist yet. The contract requires each
package to declare `framework_art`, an explicit framework dependency, an
`art.runtime.json` entry, and a bundled runtime entry before it will pass.

## Task 6 implementation and verification

Implemented the six sample Arts as independent package sources under
`art-packages/samples/`:

| Package source | Art id | Framework |
| --- | --- | --- |
| `image-compress` | `custom-1770146354922` | `cli_wrapper` |
| `remove-bg` | `custom-remove-bg-cloud` | `cloud_api` |
| `image-search` | `custom-image-search` | `mcp` |
| `color-transfer` | `custom-1770131241684` | `python_art` |
| `image-blend` | `custom-image-blend-script` | `script` |
| `image-blend-compress` | `custom-image-blend-compress-workflow` | `workflow` |

Each package now owns `manifest.json`, `art.runtime.json`, and its runtime
adapter. The workflow package also owns its child-Art dependency declaration
and `workflow.yaml`. The generic `runtime-host` remains the only framework
broker; it reads the Art package runtime manifest and does not branch on these
sample IDs.

Added:

- `scripts/Build-LoomSampleArtPackages.ps1`
- `scripts/Install-LoomSampleArtPackage.ps1`
- `scripts/tests/Test-LoomSampleArtPackageContract.ps1`
- `scripts/tests/Test-LoomSampleArtRuntime.ps1`
- `scripts/tests/Test-LoomSampleArtInstallExecution.ps1`

The legacy per-Art installers are now thin wrappers over the generic package
installer. They no longer generate legacy `cli_wrapper`, `cloud_api`,
`script`, `python_art`, `mcp`, or `workflow` definitions in Loom's registry.

Task 6 was committed as `86a1c51 feat(loom): package sample arts outside host`.

Fresh verification:

```text
Test-LoomSampleArtPackageContract.ps1
  -> source contract passed for 6 packages
  -> ZIP/hash contract passed for 6 packages

Test-LoomSampleArtRuntime.ps1
  -> all 6 package-local adapters returned image output
  -> image-search returned 3 image candidates

Test-LoomSampleArtInstallExecution.ps1
  -> installed all 6 framework ZIPs into a fresh control plane
  -> installed all 6 Art ZIPs through /v1/arts/install
  -> executed all 6 through /v1/tools/{id}/execute

Test-ImageBlendCompressWorkflowArtContract.ps1
  -> pluginized workflow contract passed

Test-ArtPluginBoundaryContract.ps1
  -> passed; README and Hook production source contain no stale default-install
     claim or sample-ID branch
```

## Task 7 implementation and verification

Implemented in the Hook repository and committed as
01beb7c feat(hook): render plugin arts by capability.

- Candidate/result rendering consumes generic loomMetadata.candidates data.
- The legacy imageSearch field remains only as a compatibility fallback.
- Candidate thumbnails and previews use the generic result candidate contract.
- Shader/live-preview behavior is selected from capability metadata.
- Production Hook source contains no sample Art ID branch.

Verification: npm run typecheck passed; npm test passed with 208 test files
and 800 tests.

## Task 8 implementation and verification

Added scripts/Invoke-LoomPluginBoundarySmoke.ps1 and wired it into
scripts/verify-release.ps1 -RunSmoke.

The smoke creates an isolated control plane and third-party framework/Art
package outside the repository, verifies default-empty discovery, installs and
executes the package, exercises framework and Art enable/disable/uninstall and
reinstall flows, restarts the daemon, and compares Loom/Hook source
fingerprints before and after the run.

The corresponding product invariant is stronger than the smoke snapshot:
framework/Art installation, execution, upgrade, rollback, disable, uninstall,
and recovery may write only to Loom control-plane package/state/evidence roots.
No package lifecycle operation is allowed to modify Loom or Hook source, and
canonical third-party identity is `publisher/id` rather than a privileged
catalog-only ID.

Evidence: target/plugin-boundary-smoke/plugin-boundary-evidence.json records
thirdPartyFrameworkInstalled=true, thirdPartyArtInstalled=true,
thirdPartyArtExecuted=true, restarted=true, loomSourceChanged=false, and
hookSourceChanged=false.

The dynamic framework registry accepts safe third-party framework IDs while
retaining the six host catalog IDs. FrameworkArt execution is blocked when the
Art is disabled or its framework is not ready.

Verification: loom_tool_registry 74 tests passed, loom-daemon 172 library
tests passed, and the plugin boundary smoke passed.

## Task 9 implementation and verification

- README describes all six frameworks as optional packages in a fresh control
  plane and documents independent framework/Art package builds.
- The release verifier rejects optional framework and sample Art payload
  directories in the default Loom package.
- The root Cargo workspace does not include the external framework runtime host.
- The boundary contract and package contract are included in the release checks.

## Current verification matrix

| Area | Result |
| --- | --- |
| Framework package source/ZIP contract | passed |
| Sample Art package source/ZIP contract | passed |
| Sample Art runtime contract | passed for all six Arts |
| Sample Art install/execute contract | passed for all six Arts |
| Framework registry lifecycle | 79 passed |
| Loom daemon library tests | 173 passed |
| Hook typecheck/tests | 800 tests passed |
| Third-party plugin boundary smoke | passed, including outside-repository compile and Hook Bridge execution |
| Current audited Loom release candidate | passed: `20260801-art-plugin-boundary-hardening-r3` |
| Final Hook release | passed: final2 |

## Task 10 implementation and verification

Framework and sample Art ZIPs were rebuilt in Release configuration. The final
Loom package was built with the Loom-owned scripts/build-release.ps1 so its
desktop root, runtime sidecar, CLI ZIP, and manifest match the Loom release
layout contract.

Loom release:
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-pluginized-art-frameworks-final2

Loom desktop ZIP SHA-256:
57805c5b174d8a59ee9fc320ccb247323ca3285ec8a225a5f11ce6c7f5582c06

Hook release:
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260801-pluginized-art-frameworks-final2

Hook executable SHA-256:
8b99ae5badc1483ea076039027e6ae24bbf08f38698beff231438cc0f5492241
Hook ZIP SHA-256:
8772581f50a0632d480c470c1492b67e9899c594ddf9fa917d35b4931b90f39f

Final release verification:
- 32 Loom package checksum entries validated.
- default payload contains no optional framework or sample Art package directories.
- standalone, Hook canvas, failed-preview, all-framework art-store/Hook, and third-party plugin boundary smokes passed.
- Hook typecheck passed.
- Hook test suite passed: 208 test files and 800 tests.

## Post-completion audit and hardening

The original Phase 67 implementation proved the package architecture, but a
fresh release-level audit found that the completion claim was incomplete under
real installation conditions:

- `scripts/build-release.ps1` still copied the legacy
  `runtime/python/Arts/Art_LoomEcho` sample into the default package;
- the release daemon searched the startup directory and compile-time source
  tree for Python Arts, so a developer checkout appeared to have a default Art
  even when a clean installed package would not;
- the official `python_art.zip` package exceeded the daemon's global 1 MiB
  request limit and therefore could not traverse the real install API;
- generic installed Arts were not marked as Loom-local capabilities, so Hook's
  enabled-Art discovery omitted independently authored packages;
- framework IDs and downloaded dependency paths needed stronger directory
  containment checks;
- framework process error paths leaked temporary directories, and waiting for
  child exit before draining stdout/stderr could deadlock on large output;
- the original third-party smoke renamed a repo-owned runtime instead of
  compiling an independently authored framework outside Loom and Hook.

The audit hardened the boundary as follows:

- the default release no longer copies any optional Python Art, and release
  verification rejects `runtime/python/Arts/`;
- production discovery only uses executable-relative package paths; source-tree
  and current-directory fallbacks are debug-only, while
  `LOOM_PYTHON_ARTS_DIR` provides an explicit isolated compatibility fixture;
- ordinary daemon APIs remain bounded to 1 MiB, while framework/Art install and
  framework-upgrade package routes have a separate bounded 32 MiB limit;
- package-installed generic Arts are forced to `artloomCompat.source =
  "loom-local"`, making them visible through the same capability list Hook
  consumes and preventing a package from claiming sync-owned provenance;
- `FrameworkArt` definitions and execution validate safe package IDs, resolve
  canonical framework package paths, and enforce containment under the package
  root;
- downloaded Art dependency paths reject absolute, rooted, drive-prefixed,
  colon-containing, and parent-directory paths;
- framework execution now uses RAII temporary-directory cleanup, terminates the
  child on early I/O failure, and drains stdout/stderr concurrently;
- the third-party smoke writes and compiles a Rust framework runtime in the OS
  temporary directory, packages a separate PowerShell Art, installs both into
  a fresh control plane, upgrades framework v1 to v2, verifies changed runtime
  behavior, discovers the Art through Hook capability metadata, instantiates a
  dynamic Hook node, executes it through Hook Bridge, restarts the daemon, and
  exercises enable/disable/uninstall/reinstall lifecycles without changing
  Loom or Hook source.

Current evidence:

- audited release candidate:
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-art-plugin-boundary-hardening-r3`;
- the candidate contains 30 checksum entries; its manifest records desktop ZIP SHA-256
  `af617e0798468b9a09a84f3bb74fa36178fa8b3d77350e31a4aedec15b115d78`,
  and packaged `loom-daemon.exe` SHA-256
  `cc47760db3a3f57d812b2de26d1e4b3a49bd1ab2118b029d3d449371c36ed808`;
- the candidate was intentionally built from the audit worktree and its
  manifest records `gitDirty=true`; it is runtime evidence, not a clean-source
  publication claim;
- standalone release smoke evidence:
  `target/runtime-smoke/art-plugin-boundary-hardening-r3-retry/runs/20260801-153352-Loom-68452-aee1788942a94da68da0ac90dbb94c4a`;
- that smoke records `pythonArtCatalog.count=0` and `defaultEmpty=true` before
  explicitly provisioning the isolated `loom_echo` compatibility fixture, then
  records `installedCount=1` and successful legacy catalog/direct execution;
- independent third-party evidence:
  `target/plugin-boundary-smoke-current/plugin-boundary-evidence.json`, with
  outside-repository compilation, v1-to-v2 upgrade, Hook capability discovery,
  Hook node instantiation, Hook Bridge execution, restart and lifecycle flags
  all passing, and `loomSourceChanged=false` / `hookSourceChanged=false`;
- all-six Hook evidence:
  `target/framework-art-store-hook-smoke-current/20260801-145606-framework-store-50748-90e9039de6434184abd5a35aa98fd61a/summary.json`, with six framework installs,
  six dynamic nodes, six successful executions, and preserved MCP selection;
- `loom_tool_registry`: 79 tests passed;
- `loom-daemon --lib`: 173 tests passed;
- Hook: typecheck passed, 208 test files and 800 tests passed;
- all six official framework packages and all six official Art packages passed
  source/ZIP, direct runtime, install, and execution contracts.
- a fresh formal `verify-release.ps1 -RunSmoke` against R3 checked 30 files and
  returned `smoke=passed`, `hookCanvasSmoke=passed`,
  `hookErrorPreviewSmoke=passed`, `frameworkArtStoreHookSmoke=passed`, and
  `pluginBoundarySmoke=passed`.

## Notes

- Current formal framework list comes from
  `crates/loom_tool_registry/src/framework.rs`; it is a repo-owned catalog, not
  a closed allowlist for dynamically installed framework IDs.
- Loom desktop may expose these six IDs as Art-authoring presets. Those entries
  are UI/manifest metadata and do not compile or bundle their framework
  runtimes into the default host.
- The default Loom release contains the host, registry, installer, and broker
  only. Framework and Art ZIPs are built and installed separately.
- Current Color Transfer is implemented as `python_art` with Hook-facing
  shader compatibility metadata. Treat "shader" as UI/capability behavior
  unless product requirements promote it into a seventh framework ID.
- Phase 67's R3 `gitDirty=true` candidate remains immutable historical runtime
  evidence. It must not be relabeled or overwritten as the later clean-source
  Phase 68 publication artifact.
- Phase 68 implementation and final validation are tracked in
  [phase-68-art-plugin-platform-hardening.md](phase-68-art-plugin-platform-hardening.md).
