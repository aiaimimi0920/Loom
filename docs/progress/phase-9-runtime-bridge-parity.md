# Phase 9: Runtime Bridge Parity

## Goal

Restore the runtime behavior behind the Phase 8 ArtLoom parity surfaces:
stdio MCP execution, MCP-backed registry tool invocation, Hook bridge request
handling, and daemon bridge lifecycle.

## Tasks

- [x] P9.1 MCP stdio runtime
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_mcp`
    proves initialize, `tools/list`, and `tools/call` against a fixture server.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_mcp` -> 9 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_mcp` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P9.2 MCP-backed tool execution
  - Acceptance:
    `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry`
    proves a registry entry can call a fixture MCP tool and receive structured
    content.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry` -> 6 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_tool_registry` -> passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P9.3 Hook bridge request handlers
  - Acceptance:
    `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge`
    proves handshake response, instantiate broadcast, and workflow node
    write-back.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> 8 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P9.4 Daemon bridge lifecycle
  - Acceptance:
    `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon`
    proves start/status/stop routes and non-conflicting test ports.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 28 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P9.5 Desktop and release runtime smoke
  - Acceptance: regenerated Loom release contains `loom-desktop.exe` and smoke
    covers MCP runtime execution plus Hook bridge runtime status.
  - Completed: 2026-06-12.
  - Release: `release\Loom\loom-runtime-bridge-0c703230`.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 30 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
    - `npm run typecheck` in `Loom/apps/desktop` -> passed.
    - `npm run build` in `Loom/apps/desktop` -> passed.
    - `cargo check --manifest-path src-tauri\Cargo.toml` in `Loom/apps/desktop` -> passed.
    - `cargo fmt --manifest-path src-tauri\Cargo.toml --all -- --check` in `Loom/apps/desktop` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-runtime-bridge-0c703230 -Force` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-runtime-bridge-0c703230 -Apps Loom` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-runtime-bridge-0c703230 -Apps Loom` -> passed.

## Evidence

P9.1 completed with a self-hosted fixture MCP server covering stdio
initialize, `tools/list`, and `tools/call`.
P9.2 completed with a registry-backed fixture MCP tool call returning
structured content.
P9.3 completed with socket-independent Hook bridge handlers for handshake,
instantiate broadcasts, and workflow node write-back.
P9.4 completed with daemon lifecycle routes for bridge status, start, conflict
handling, and stop on non-conflicting test ports.
P9.5 completed with desktop Hook Bridge start/stop controls, daemon
MCP-backed tool execute route, and packaged release smoke proving
`mcpToolExecution = release mcp runtime` plus Hook bridge runtime ports.

## Notes

- Phase 9 is a runtime parity layer, not a UI redesign.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
- Compatibility protocol names such as `art_loom/update_workflow_node` are
  allowed only at bridge boundaries.
- OCR, embedded Python, shared memory, and cloud image execution remain
  deferred.
