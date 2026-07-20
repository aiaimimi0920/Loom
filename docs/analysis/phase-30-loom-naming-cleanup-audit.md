# Phase 30 Loom Naming Cleanup Audit

## Scope

Phase 30 fixes a final-audit naming gap found while building the final
ArtLoom parity matrix.

The user-facing Loom product names must remain:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

and Loom release/configuration artifacts must not expose the old
`NeuroLoom`/`Neuro` product prefix.

## Gap found

After Phase 29, the shipped executable names were already correct, but the
final audit found two remaining Loom-specific naming leaks:

1. `scripts/build-release-exes.ps1` generated `BUILD_INFO.txt` with:

   ```text
   Neuro Windows release artifact
   ```

   This is a release artifact and therefore visible in the packaged Loom
   output.

2. `Loom/crates/loom_configuration/src/store.rs` stored the default managed
   configuration under:

   ```text
   %APPDATA%\Neuro\loom\configuration\apps
   .runtime\neuro\loom\configuration\apps
   ```

   This was still carrying the old project prefix in Loom's runtime storage
   contract.

Also cleaned package metadata:

- `Loom/Cargo.toml`
- `Loom/apps/desktop/src-tauri/Cargo.toml`

from:

```text
authors = ["Neuro contributors"]
```

to:

```text
authors = ["Loom contributors"]
```

## Contract-first RED

Updated:

```text
scripts/tests/test-build-release-exes-contract.ps1
scripts/tests/test-loom-desktop-shell-contract.ps1
Loom/crates/loom_configuration/src/store.rs
```

New contract expectations:

- release `BUILD_INFO.txt` template uses `Loom Windows release artifact`
- release `BUILD_INFO.txt` template no longer uses
  `Neuro Windows release artifact`
- desktop Cargo metadata uses `authors = ["Loom contributors"]`
- desktop Cargo metadata no longer uses `authors = ["Neuro contributors"]`
- default managed configuration root is:

  ```text
  %APPDATA%\Loom\configuration\apps
  .runtime\loom\configuration\apps
  ```

Observed RED:

```text
Release BUILD_INFO heading must use Loom naming.
```

and:

```text
left: "C:\\Users\\demo\\AppData\\Roaming\\Neuro\\loom\\configuration\\apps"
right: "C:\\Users\\demo\\AppData\\Roaming\\Loom\\configuration\\apps"

left: ".runtime\\neuro\\loom\\configuration\\apps"
right: ".runtime\\loom\\configuration\\apps"
```

The new desktop metadata assertion also failed before implementation:

```text
Desktop Cargo metadata must use Loom contributor naming.
```

## Implementation

Updated:

```text
scripts/build-release-exes.ps1
Loom/crates/loom_configuration/src/store.rs
Loom/Cargo.toml
Loom/apps/desktop/src-tauri/Cargo.toml
```

Behavior after implementation:

- `BUILD_INFO.txt` starts with:

  ```text
  Loom Windows release artifact
  ```

- default managed configuration root uses:

  ```text
  %APPDATA%\Loom\configuration\apps
  .runtime\loom\configuration\apps
  ```

- Loom package metadata uses:

  ```text
  authors = ["Loom contributors"]
  ```

The configuration tests now guard the process-wide `APPDATA` mutation with a
mutex, so the APPDATA and fallback tests remain stable under default parallel
`cargo test` execution.

## Validation evidence

Targeted tests:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
cargo test --manifest-path Loom\Cargo.toml -p loom_configuration default_root --offline -- --nocapture
```

Results:

```text
Release build dry-run contract passed.
Loom desktop shell contract passed.
2 passed; 0 failed
```

Compile/build checks:

```powershell
cargo fmt --manifest-path Loom\Cargo.toml --all
cargo check --manifest-path Loom\Cargo.toml -p loom-daemon --offline
npm --prefix Loom\apps\desktop run typecheck
cargo fmt --manifest-path Loom\Cargo.toml --all -- --check
git diff --check
```

Results:

```text
cargo check: finished successfully
typecheck: passed
cargo fmt --check: passed
git diff --check: passed
```

Targeted residual-prefix check:

```powershell
rg -n "Neuro Windows release artifact|Neuro contributors|Neuro\\loom|\.runtime\\neuro\\loom|AppData\\Roaming\\Neuro\\loom" scripts\build-release-exes.ps1 Loom\Cargo.toml Loom\apps\desktop\src-tauri\Cargo.toml Loom\crates\loom_configuration\src\store.rs scripts\tests\test-build-release-exes-contract.ps1 scripts\tests\test-loom-desktop-shell-contract.ps1
```

Only matched the new negative test assertions, not production code.

## Release evidence

Release version:

```text
loom-naming-cleanup-phase30
```

Build:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-naming-cleanup-phase30 -Force
```

Generated:

```text
release\Loom\loom-naming-cleanup-phase30\loom.exe
release\Loom\loom-naming-cleanup-phase30\loom-daemon.exe
release\Loom\loom-naming-cleanup-phase30\loom-desktop.exe
release\Loom\loom-naming-cleanup-phase30\packages\Loom-loom-naming-cleanup-phase30-windows-x64.zip
```

Package:

```text
size = 50101766 bytes
sha256 = 33087657aa8e2e1fc8f708b8f557ccea24218e165b7041e6b51796ef04688379
```

`BUILD_INFO.txt` begins with:

```text
Loom Windows release artifact
```

Formal release verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom
```

Result:

```text
status = passed
gitHead = b36a19dc13b78d4381c394b4fa66bc8a31ac4194
gitDirty = false
checksumEntries = 31
```

Release smoke:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom
```

Result:

```text
status = passed
summaryEvidencePath = output\smoke\runs\20260612-230235-Loom-91904-01bff690888c4180abe3a661590f4eff\release-local-apps-loom-naming-cleanup-phase30-Loom-summary.json
summaryLatestEvidencePath = output\smoke\latest\release-local-apps-loom-naming-cleanup-phase30-Loom-summary.json
```

Smoke evidence retained prior restored ArtLoom parity paths:

- `hookBridgeSettings.settingsTheme = "system"`
- `hookBridgeSettings.shortcutCount = 4`
- `mcpMarketplace.connectionTestSuccess = true`
- `managementCrud.workflowDeleted = true`
- `pythonArtSourceImport.scriptToolExecution = "source import saw release source helper"`
- `pythonArtToolExecution = "python art saw release installed python art"`
- `cloudMultipartArtNode.multipartSeen = true`
- `realOcrImage.fullTextLength = 63`
- `sharedImageAhrpProcess.outputType = "shared_memory"`
- `workflowArtNode.success = true`
- `workflowAhrpProcess.status = "Success"`

## Remaining final-audit work

Phase 30 only fixes a naming/configuration-root gap found during the final
audit. The final ArtLoom parity matrix is still required before claiming the
Loom migration is彻底 complete.
