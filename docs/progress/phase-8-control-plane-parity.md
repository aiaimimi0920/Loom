# Phase 8: ArtLoom Control-Plane Parity

## Goal

Restore ArtLoom's MCP, registry, workflow store, and Hook bridge capabilities in
Loom without bulk-copying the old monolith.

## Tasks

- [x] P8.1 MCP core contracts
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_mcp` passes.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_mcp` -> 6 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_mcp` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P8.2 Workflow store and graph codec
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow_store` passes.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow_store` -> 3 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_workflow_store` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P8.3 Tool registry
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry` passes.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry` -> 4 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_tool_registry` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P8.4 Hook bridge protocol contracts
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` passes.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> 5 passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom_hook_bridge` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P8.5 Daemon control-plane APIs
  - Acceptance: daemon tests cover MCP, workflow, tools, and hook-bridge status APIs.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 27 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P8.6 Desktop control-plane surfaces
  - Acceptance: desktop contract, typecheck, build, and Tauri check pass.
  - Completed: 2026-06-12.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1` -> passed.
    - `npm run typecheck` in `Loom/apps/desktop` -> passed.
    - `npm run build` in `Loom/apps/desktop` -> passed.
    - `cargo check --manifest-path src-tauri\Cargo.toml` in `Loom/apps/desktop` -> passed.
    - Browser preview on `http://127.0.0.1:1424/` showed MCP, Registry, Workflow Manager, and Hook Bridge navigation.
- [x] P8.7 Release parity smoke
  - Acceptance: regenerated Loom release passes formal verifier and smoke with
    `loom-desktop.exe` plus control-plane API checks.
  - Completed: 2026-06-12.
  - Release: `release\Loom\loom-control-plane-60f8e263`.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-control-plane-60f8e263 -Force` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-control-plane-60f8e263 -Apps Loom` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-control-plane-60f8e263 -Apps Loom` -> passed with `loom-desktop.exe`, MCP server save/list, tool registry save/list, workflow save/list, Hook Bridge port `19820`, and legacy methods `art_loom/update_workflow_node` plus `art_hook/instantiate`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Do not write to old ArtLoom.
- Keep visible product naming as Loom.
- Compatibility protocol names such as `art_loom/update_workflow_node` are
  allowed only at bridge boundaries.
- OCR, embedded Python, shared memory, and cloud image execution are deferred.
