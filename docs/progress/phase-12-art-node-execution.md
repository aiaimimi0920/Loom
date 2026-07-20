# Phase 12: Art Node Execution Runtime

## Goal

Restore the first executable `art_loom/execute_art_node` runtime layer by
mapping old Art node execution requests to Loom's existing tool registry and
MCP-backed execution runtime.

## Tasks

- [x] P12.1 Hook bridge response shaping
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge`
    proves MCP execution results can be converted into old-compatible
    `execute_art_node` success/error responses.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge execute_art_node -- --nocapture` failed before helper functions existed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> 10 tests passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P12.2 Daemon MCP-backed Art node execution
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
    proves a WebSocket `art_loom/execute_art_node` request can execute a
    registered MCP-backed Loom tool by `art_id`.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon executes_mcp_backed_art_node -- --nocapture` failed before runtime dispatch because response `type` was `error`.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 33 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P12.3 Release execute-art-node smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` executes `art_loom/execute_art_node` through the fixture
    MCP-backed tool and records the response in the smoke summary.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` failed until `Test-LoomHookBridgeExecuteArtNode` existed in release smoke.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-art-node-565a4966 -Force` -> generated `release\Loom\loom-art-node-565a4966`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-art-node-565a4966 -Apps Loom` -> passed with `gitDirty = false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-art-node-565a4966 -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-070210-Loom-46856-55a189863fb4416faf3b17bf0349492a\release-local-apps-loom-art-node-565a4966-Loom-summary.json`.
    - Execute evidence: `type = success`, `success = true`, `nodeId = release-node-mcp`, `outputText = release execute art node`.

## Evidence

Phase 12 completed the first executable `art_loom/execute_art_node` layer:
packaged `loom-daemon.exe` can execute a registered MCP-backed Loom tool through
the legacy Hook bridge method and return old-compatible execution response
JSON.

## Notes

- This phase restores the MCP-backed execution layer only.
- Native image filters, OCR, Python/script/shader execution, cloud image APIs,
  workflow graph execution, and shared-memory image paths remain out of scope
  for later phases.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
