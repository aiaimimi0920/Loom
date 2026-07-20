# Phase 16: Cloud API Runtime

## Goal

Restore old ArtLoom's `cloud_api` execution layer in Loom so explicitly
registered cloud API Arts can execute through direct tool execution, Hook Bridge
`art_loom/execute_art_node`, and AHRP `art/process`.

## Tasks

- [x] P16.1 Cloud API registry execution
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry cloud -- --nocapture`
    proves `ToolExecution::CloudApi` calls a local HTTP fixture, sends JSON
    arguments, parses JSON/image output, and reports HTTP errors.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry cloud -- --nocapture`
      failed before implementation with unsupported `cloud_api` execution.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry cloud -- --nocapture`
      passed: 3 tests.
    - Regression: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry`
      passed: 12 tests.
    - Format: `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
      passed.
- [x] P16.2 Hook Bridge/AHRP cloud image routing
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon cloud -- --nocapture`
    proves registered cloud API Arts execute through direct `/v1/tools/*/execute`,
    Hook Bridge `art_loom/execute_art_node`, and AHRP `art/process`.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon cloud -- --nocapture`
      initially failed before daemon cloud error mapping.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon cloud -- --nocapture`
      passed: 3 tests.
    - Regression: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed: 43 lib tests and 2 binary contract tests.
    - Compile: `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed.
    - Format: `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
      passed.
- [x] P16.3 Release cloud API smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` executes a cloud API fixture and records
    `cloudToolExecution`, `cloudArtNode`, and `cloudAhrpProcess` evidence while
    keeping Loom-only executable names.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      failed with missing `Test-LoomHookBridgeCloudArtNode`.
    - GREEN: same command passed after adding the local cloud API fixture and
      release smoke checks.
    - Parse: `[System.Management.Automation.Language.Parser]::ParseFile(...)`
      passed for `scripts/smoke-release-local-apps.ps1`.
    - Release build: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-cloud-api-0b5b3a84 -Force`
      generated `release\Loom\loom-cloud-api-0b5b3a84`.
    - Formal verify: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-cloud-api-0b5b3a84 -Apps Loom`
      passed with `gitDirty = false`.
    - Release smoke: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-cloud-api-0b5b3a84 -Apps Loom`
      passed.
    - Smoke summary:
      `output\smoke\runs\20260612-090333-Loom-60544-6ee98408cb7f41f0ba24fd1e81774d52\release-local-apps-loom-cloud-api-0b5b3a84-Loom-summary.json`.
    - Packaged executable names:
      `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
    - Cloud evidence:
      `cloudToolExecution = "cloud saw release cloud runtime"`,
      `cloudArtNode.outputType = "base64"`,
      `cloudAhrpProcess.status = "Success"`.

## Evidence

- Commit `158419f feat(loom): execute cloud api registry tools` restored
  explicit registry-backed `ToolExecution::CloudApi` execution with JSON POST
  and MCP-like response normalization.
- Commit `9643e57 feat(loom): route cloud api arts through hook bridge` proved
  cloud API output routes through daemon direct execution, Hook Bridge
  `art_loom/execute_art_node`, and AHRP `art/process`.
- Commit `0b5b3a8 test(loom): smoke cloud api runtime` added release smoke
  coverage for packaged cloud API runtime.
- Release `loom-cloud-api-0b5b3a84` verified and smoked successfully.

## Notes

- This phase follows the approved "方案 B 分层恢复" path: restore the safe,
  explicitly registered `cloud_api` layer before later richer Art pack import.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- This phase intentionally restores the safe, explicit registry-backed cloud
  API layer only. Full old ArtLoom multipart/body/header/template converter
  parity remains a later layer.
