# Phase 67: Pluginized Art frameworks

## Status

Planned. No implementation changes have started after the baseline tag.

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

## Progress checklist

- [ ] Task 1: Add source-contract guards before runtime changes.
- [x] Task 2: Define framework package manifests and explicit installed state.
- [x] Task 3: Implement framework package install, disable, upgrade, and uninstall.
- [x] Task 4: Add the generic external framework execution protocol.
- [x] Task 5: Convert the six sample frameworks into independent packages.
- [ ] Task 6: Convert the six sample Arts into external Art packages.
- [ ] Task 7: Make Hook fully capability-driven for plugin Arts.
- [ ] Task 8: Add end-to-end plugin boundary smoke.
- [ ] Task 9: Update documentation and remove default-build/resource leakage.
- [ ] Task 10: Build final Loom and Hook releases.

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

- [ ] Default Loom release contains no optional Art framework runtime package.
- [ ] Fresh control-plane root starts with zero installed optional frameworks.
- [ ] Framework installation is package-backed rather than a built-in flag flip.
- [ ] Framework disable/enable/upgrade/uninstall are supported.
- [ ] Art disable/enable/upgrade/uninstall are supported.
- [ ] Six sample framework packages install and execute successfully.
- [ ] Six sample Art packages install and execute successfully.
- [ ] A temporary third-party framework package installs and executes.
- [ ] A temporary third-party Art package installs and executes.
- [ ] Hook has no production branch on sample Art IDs.
- [ ] Loom has no production branch on sample Art IDs outside fixtures/tests/docs.
- [ ] `verify-release.ps1 -RunSmoke` includes the plugin boundary smoke.
- [ ] Final Loom release exists under
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

## Notes

- Current formal framework list comes from
  `crates/loom_tool_registry/src/framework.rs`.
- Current README explicitly says `cli_wrapper`, `cloud_api`, `script`, and
  `workflow` are installed by default. This must be removed during this phase.
- Current Color Transfer is implemented as `python_art` with Hook-facing
  shader compatibility metadata. Treat "shader" as UI/capability behavior
  unless product requirements promote it into a seventh framework ID.
