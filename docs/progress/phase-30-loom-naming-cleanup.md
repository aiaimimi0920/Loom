# Phase 30: Loom Naming Cleanup

## Goal

Remove remaining Loom-specific `Neuro` product-prefix leaks from release
metadata, default managed configuration storage, and desktop package metadata;
then regenerate and smoke-test the Loom release.

## Tasks

- [x] P30.1 Contract-first naming gaps
  - Acceptance: tests fail before implementation for release metadata, managed
    configuration path, and desktop Cargo metadata.
  - Evidence:
    - `scripts/tests/test-build-release-exes-contract.ps1` now requires
      `Loom Windows release artifact` and rejects
      `Neuro Windows release artifact`.
    - `Loom/crates/loom_configuration/src/store.rs` tests now require:
      - `%APPDATA%\Loom\configuration\apps`
      - `.runtime\loom\configuration\apps`
    - `scripts/tests/test-loom-desktop-shell-contract.ps1` now requires
      desktop Cargo metadata `authors = ["Loom contributors"]`.
    - RED evidence:
      - `Release BUILD_INFO heading must use Loom naming.`
      - `left: "...\\Neuro\\loom\\configuration\\apps"`,
        `right: "...\\Loom\\configuration\\apps"`
      - `Desktop Cargo metadata must use Loom contributor naming.`

- [x] P30.2 Release metadata and configuration root cleanup
  - Acceptance: Loom-generated release and default configuration paths use
    Loom naming.
  - Evidence:
    - `scripts/build-release-exes.ps1` now writes
      `Loom Windows release artifact`.
    - `Loom/crates/loom_configuration/src/store.rs` now defaults to:
      - `%APPDATA%\Loom\configuration\apps`
      - `.runtime\loom\configuration\apps`
    - `Loom/Cargo.toml` and
      `Loom/apps/desktop/src-tauri/Cargo.toml` now use
      `authors = ["Loom contributors"]`.
    - The configuration tests use an `APPDATA` env mutex to avoid parallel test
      races.

- [x] P30.3 Verification and release
  - Acceptance: targeted contracts, Rust compile, desktop typecheck, formal
    release verification, and packaged release smoke pass.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1`
      passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1`
      passed.
    - `cargo test --manifest-path Loom\Cargo.toml -p loom_configuration default_root --offline -- --nocapture`
      passed with 2 tests.
    - `cargo check --manifest-path Loom\Cargo.toml -p loom-daemon --offline`
      passed.
    - `npm --prefix Loom\apps\desktop run typecheck` passed.
    - `cargo fmt --manifest-path Loom\Cargo.toml --all -- --check` passed.
    - `git diff --check` passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-naming-cleanup-phase30 -Force`
      generated `release\Loom\loom-naming-cleanup-phase30` with `loom.exe`,
      `loom-daemon.exe`, and `loom-desktop.exe`.
    - Package:
      `release\Loom\loom-naming-cleanup-phase30\packages\Loom-loom-naming-cleanup-phase30-windows-x64.zip`,
      size `50101766` bytes, sha256
      `33087657aa8e2e1fc8f708b8f557ccea24218e165b7041e6b51796ef04688379`.
    - `release\Loom\loom-naming-cleanup-phase30\BUILD_INFO.txt` starts with
      `Loom Windows release artifact`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom`
      passed formal verification with
      `gitHead = b36a19dc13b78d4381c394b4fa66bc8a31ac4194`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-230235-Loom-91904-01bff690888c4180abe3a661590f4eff\release-local-apps-loom-naming-cleanup-phase30-Loom-summary.json`.
    - Smoke evidence includes:
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

## Notes

- `Neuro Gateway` remains an external gateway/client concept and was not
  renamed in this phase.
- Root repository URLs that point at the current monorepo are not product
  surface names.
- The final ArtLoom parity matrix is still required before claiming the Loom
  migration is彻底 complete.
