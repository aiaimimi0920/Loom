# Phase 17: Workflow Graph Runtime

## Goal

Restore old ArtLoom's `workflow` execution layer in Loom so explicitly
registered workflow-backed Arts can execute saved workflow YAML through direct
tool execution, Hook Bridge `art_loom/execute_art_node`, and AHRP
`art/process`.

## Tasks

- [x] P17.1 Workflow runtime crate
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow_runtime -- --nocapture`
    proves saved workflow YAML executes child tools in dependency order,
    resolves node output references, honors primary output bindings, supports
    native image child nodes, and reports unresolved dependencies.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow_runtime -- --nocapture`
      initially failed because package `loom_workflow_runtime` did not exist.
    - RED after test skeleton: all 5 runtime tests failed with
      `NotImplemented`.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow_runtime -- --nocapture`
      passed: 5 passed, 0 failed.
    - Regression: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry -- --nocapture`
      passed: 12 passed, 0 failed.
    - Format: `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
      passed.
- [x] P17.2 Hook Bridge/AHRP workflow routing
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon workflow -- --nocapture`
    proves workflow-backed registry tools execute through direct
    `/v1/tools/*/execute`, Hook Bridge `art_loom/execute_art_node`, and AHRP
    `art/process`.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon workflow -- --nocapture`
      failed with `unsupported_tool_execution` for `workflow` before daemon
      wiring.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon workflow -- --nocapture`
      passed: 4 passed, 0 failed.
    - Regression: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed: 46 lib tests, 2 CLI contract tests, and doc tests passed.
    - Compile: `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon`
      passed.
    - Format: `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check`
      passed.
- [x] P17.3 Release workflow runtime smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` executes a workflow fixture and records
    `workflowToolExecution`, `workflowArtNode`, and `workflowAhrpProcess`
    evidence while keeping Loom-only executable names.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      failed on missing `Test-LoomHookBridgeWorkflowArtNode`.
    - GREEN: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - Parse: PowerShell parser check for `scripts\smoke-release-local-apps.ps1`
      passed.
    - Release build: `loom-workflow-runtime-efd911d8`.
    - Formal verify: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-workflow-runtime-efd911d8 -Apps Loom`
      passed with `gitDirty = false`.
    - Release smoke: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-workflow-runtime-efd911d8 -Apps Loom`
      passed.

## Evidence

- Implementation commits:
  - `d2fa7c9 feat(loom): execute workflow registry tools`
  - `a945cc5 feat(loom): route workflow arts through hook bridge`
  - `efd911d test(loom): smoke workflow runtime`
- Release directory:
  `release\Loom\loom-workflow-runtime-efd911d8`
- Release executables:
  - `loom.exe`
  - `loom-daemon.exe`
  - `loom-desktop.exe`
- Release smoke summary:
  `output\smoke\runs\20260612-094919-Loom-58936-909b3f8c396c494d8860fea211fe9dc4\release-local-apps-loom-workflow-runtime-efd911d8-Loom-summary.json`
- Workflow runtime smoke keys:
  - `workflowToolExecution = "script saw release workflow runtime"`
  - `workflowArtNode.outputType = "base64"`
  - `workflowAhrpProcess.status = "Success"`
- Smoke cleanup note: release smoke emitted a descendant-process warning for
  PID `62220`; a follow-up process check found that PID was no longer running.
  Existing older release processes were observed and left untouched.

## Notes

- This phase follows the approved "方案 B 分层恢复" path: restore workflow
  runtime execution as a bounded layer after script/cloud runtimes.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- This phase does not implement the full visual workflow editor UI or
  shared-memory image I/O.
