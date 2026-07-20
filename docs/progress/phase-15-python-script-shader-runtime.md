# Phase 15: Python / Script / Shader Runtime

## Goal

Restore the next old ArtLoom local execution layer in Loom: registry-backed
script execution that can run Python/script/shader-style Arts through Hook
Bridge `art_loom/execute_art_node`, AHRP `art/process`, and packaged
`loom-daemon.exe` release smoke.

## Tasks

- [x] P15.1 Script execution contract
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script -- --nocapture`
    proves `ToolExecution::Script` runs a saved script fixture, passes JSON
    arguments, parses JSON stdout, and reports spawn/exit/parse errors.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script -- --nocapture` failed before script execution support because script tools returned `UnsupportedExecution { execution_type: "script" }`.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script -- --nocapture` -> 3 tests passed.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry` -> 9 tests passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P15.2 Hook Bridge script/Python/shader routing
  - Acceptance: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script -- --nocapture`
    proves registered script Arts execute through both `art_loom/execute_art_node`
    and AHRP `art/process`, including image base64 shaping and shader text
    output.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script -- --nocapture` initially failed to compile because new script errors were not mapped in daemon error handling.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script -- --nocapture` -> 4 tests passed.
    - Full daemon validation first exposed shared control-plane-root races in older registry/Hook Bridge tests; those tests were isolated with the existing `ENV_LOCK` pattern.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 40 lib tests and 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P15.3 Release script/Python/shader smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` executes a script Art fixture and records
    `scriptToolExecution`, `scriptArtNode`, `scriptAhrpProcess`, and
    `scriptShaderArt` evidence while keeping Loom-only executable names.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` failed until `Test-LoomHookBridgeScriptArtNode` existed.
    - GREEN: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - PowerShell parser check for `scripts\smoke-release-local-apps.ps1` -> passed.
    - First packaged smoke exposed a here-string fixture close marker issue; after fixing it, smoke passed on the generated release and the release was regenerated from the fixed HEAD.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-script-runtime-fc53e333 -Force` -> generated `release\Loom\loom-script-runtime-fc53e333`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-script-runtime-fc53e333 -Apps Loom` -> passed with `gitDirty = false` and executable names `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-script-runtime-fc53e333 -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-083019-Loom-47596-4eb44a1ef80e464ca6419f9e89d018ee\release-local-apps-loom-script-runtime-fc53e333-Loom-summary.json`.
    - Script evidence: `scriptToolExecution = "script saw release script runtime"`, `scriptArtNode.outputType = base64`, `scriptAhrpProcess.status = Success`, `scriptShaderArt.outputText = "void fragment() { COLOR = vec4(1.0); }"`.

## Evidence

Phase 15 completed the first Python/script/shader parity layer:
packaged `loom-daemon.exe` now executes explicitly registered script-backed
tools, routes script image output through Hook Bridge `art_loom/execute_art_node`
and AHRP `art/process`, and routes shader-style script text output through
`execute_art_node`.

## Notes

- This phase follows the approved "方案 B 分层恢复" path: restore the safe,
  registry-backed execution layer first instead of wholesale-copying old
  `PythonEngine`.
- Registry-backed MCP tools continue to take precedence when a registry entry
  exists; native image fallback remains limited to known native image Art IDs
  when no registry entry exists.
- This phase does not bundle old ArtLoom's embedded Python distribution; `.py`
  script paths are supported through `LOOM_PYTHON` or system `python`.
- Full GPU shader preview and desktop import UI remain for later UI/API phases.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
