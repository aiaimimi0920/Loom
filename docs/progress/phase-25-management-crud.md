# Phase 25: Management CRUD Parity

## Goal

Restore the old ArtLoom management CRUD paths for saved MCP servers, saved
workflow YAML, and saved tool definitions in Loom, including desktop load/delete
actions and packaged release smoke proof, while keeping visible product names as
`loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`.

## Tasks

- [x] P25.1 Source audit and parity boundary
  - Acceptance: source-backed audit identifies old MCP server deletion,
    workflow load/delete, workflow management UI, current Loom gaps, and the
    Phase 25 recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-25-management-crud-audit.md` records old
      `src-tauri/src/lib.rs`, `mcp_engine.rs`, `workflow_store.rs`,
      `ipc_service.rs`, `runtimeBridge.ts`, `useWorkflows.ts`,
      `WorkflowList.tsx`, `Canvas.tsx`, `MCPSettings.tsx`, current Loom gaps,
      implementation design, release evidence, and non-goals.

- [x] P25.2 Daemon CRUD routes
  - Acceptance: Loom daemon exposes direct management routes for deletion and
    workflow loading.
  - Evidence:
    - `Loom/apps/daemon/src/lib.rs` exposes:
      - `DELETE /v1/mcp/servers/{serverId}`
      - `DELETE /v1/tools/{toolId}`
      - `GET /v1/workflows/{workflowId}`
      - `DELETE /v1/workflows/{workflowId}`
    - Added handlers:
      - `delete_mcp_server`
      - `delete_tool`
      - `get_workflow`
      - `delete_workflow`
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_and_writes_mcp_servers --offline -- --nocapture --test-threads=1`
      passed with 1 test.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_and_writes_tool_and_workflow_contracts --offline -- --nocapture --test-threads=1`
      passed with 1 test.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline`
      passed.

- [x] P25.3 Desktop DELETE bridge and API helpers
  - Acceptance: desktop code can call daemon DELETE routes through Tauri or
    browser fetch fallback, and can load saved workflow YAML bundles.
  - Evidence:
    - `Loom/apps/desktop/src-tauri/src/lib.rs` adds
      `delete_loom_daemon_json` and `http_delete_json`.
    - `Loom/apps/desktop/src/services/loomApi.ts` adds
      `LoomWorkflowBundle`, `LoomWorkflowBundleResponse`,
      `deleteJsonViaTauri`, `deleteJson`, `getWorkflowBundle`,
      `deleteWorkflowBundle`, `deleteToolDefinition`, and `deleteMcpServer`.
    - `cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline`
      passed.
    - `npm --prefix Loom/apps/desktop run typecheck` passed.

- [x] P25.4 Desktop management UI actions
  - Acceptance: desktop UI exposes practical delete/load controls matching old
    management behavior.
  - Evidence:
    - `Loom/apps/desktop/src/App.tsx` adds:
      - MCP panel `Delete server`
      - Registry panel `Delete tool`
      - Workflow Studio `Load YAML`
      - Workflow Studio `Delete workflow`
    - `Load YAML` calls `getWorkflowBundle()` and populates the workflow YAML
      editor.
    - Delete actions call their corresponding helpers and refresh the desktop
      snapshot.
    - `npm --prefix Loom/apps/desktop run typecheck` passed.
    - `npm --prefix Loom/apps/desktop run build` passed.

- [x] P25.5 Contract, release, and smoke
  - Acceptance: parity contract passes; regenerated release contains the new
    CRUD paths, keeps all previously restored runtime paths green, and proves
    management CRUD through release smoke.
  - Evidence:
    - `scripts/tests/test-loom-artloom-parity-contract.ps1` asserts the new
      daemon routes, release-smoke tokens, desktop API helpers, Tauri DELETE
      bridge, and desktop button labels.
    - `scripts/smoke-release-local-apps.ps1` exercises:
      - `DELETE /v1/mcp/servers/fixture-delete`
      - `DELETE /v1/tools/fixture-delete-tool`
      - `GET /v1/workflows/release-workflow`
      - `DELETE /v1/workflows/fixture-delete-workflow`
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed
      after applying official formatting.
    - `rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1`
      returned no matches.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-management-crud-phase25 -Force`
      generated `release\Loom\loom-management-crud-phase25` with
      `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`.
    - `packages\Loom-loom-management-crud-phase25-windows-x64.zip`
      was generated with size `50034898` bytes.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-management-crud-phase25 -Apps Loom`
      passed formal verification with
      `gitHead = 022c1fb386c868628556a0c12667f45ad40d6e05`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-management-crud-phase25 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-202655-Loom-78112-ccf2bdab51cd4c60956e0a2d653bb652\release-local-apps-loom-management-crud-phase25-Loom-summary.json`.
    - Smoke evidence includes:
      - `managementCrud.mcpServerDeleted = true`
      - `managementCrud.toolDeleted = true`
      - `managementCrud.workflowLoaded = "release-workflow"`
      - `managementCrud.workflowDeleted = true`
      - `pythonArtCatalog.artId = "loom_echo"`
      - `pythonArtToolExecution = "python art saw release installed python art"`
      - `pythonToolExecution.packagedPython = true`
      - `workflowToolExecution = "script saw release workflow runtime"`
      - `cloudMultipartArtNode.multipartSeen = true`
      - `realOcrImage.fullTextLength = 63`

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Protocol compatibility names such as `art_loom/*`, `art_hook/*`,
  `art/process`, `shared_memory`, `image_path`, `image_base64`,
  `image_buffer`, and `python_art` remain intentionally supported.
- Phase 25 restores the low-level save/load/delete management behavior needed
  for product usability. It does not claim that the full old MCP marketplace UI,
  full ReactFlow visual graph editor, or old Python source-edit/import UI are
  restored.
- The next recommended step is to continue the final full-source audit and turn
  any remaining required omission into a focused follow-up phase.
