# Phase 13: AHRP Process Compatibility

## Goal

Restore the first AHRP runtime layer by making Hook Bridge `art/update_property`
return an old-compatible ACK and making `art/process` execute MCP-backed Loom
tools with base64 image output responses.

## Tasks

- [x] P13.1 AHRP response shaping and property ACK
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge`
    proves `property_ack`, process base64 success, error response, and MCP image
    extraction helpers use old-compatible shapes.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge ahrp -- --nocapture` failed before helper functions existed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge ahrp -- --nocapture` -> 4 tests passed.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> 14 tests passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed after applying the repo formatter.
- [x] P13.2 Daemon AHRP MCP-backed process execution
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
    proves a WebSocket `art/process` request executes a registered MCP-backed
    Loom tool by `art_id` and returns an AHRP base64 image result.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon ahrp_process -- --nocapture` failed before daemon runtime dispatch because response lacked the AHRP request id.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon ahrp_process -- --nocapture` -> 1 test passed.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 34 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed after applying the repo formatter.
- [x] P13.3 Release AHRP process smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` handles WebSocket `art/process` through the fixture
    MCP-backed tool and records `ahrpProcess` response evidence.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` failed until `Test-LoomHookBridgeAhrpProcess` existed in release smoke.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-ahrp-process-74c2a485 -Force` -> generated `release\Loom\loom-ahrp-process-74c2a485`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-ahrp-process-74c2a485 -Apps Loom` -> passed with `gitDirty = false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-ahrp-process-74c2a485 -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-072451-Loom-48756-2a8c07b4a3fa4bc2b54fb149bffb4a85\release-local-apps-loom-ahrp-process-74c2a485-Loom-summary.json`.
    - AHRP evidence: `requestId = release-ahrp-process`, `status = Success`, `outputType = base64`, `width = 1`, `height = 1`.

## Evidence

Phase 13 completed the first AHRP process/property compatibility layer:
packaged `loom-daemon.exe` can ACK `art/update_property` and execute a
registered MCP-backed Loom tool through WebSocket `art/process`, returning an
old-compatible AHRP base64 image result.

## Notes

- This phase restores AHRP process/property compatibility only for MCP-backed
  tools and base64 output.
- Native filters, Python/script/shader, cloud image API, workflow graph
  execution, shared memory, and OCR remain for later phases.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
