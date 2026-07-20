# Phase 25 Management CRUD Audit

## Scope

Phase 25 restores the low-level management CRUD paths that old ArtLoom exposed
for MCP servers, saved workflow YAML, and user-facing registry items.

Restored capabilities:

- daemon deletion API for saved MCP servers
- daemon load and deletion API for saved workflows
- daemon deletion API for saved tool definitions
- desktop Tauri DELETE bridge
- desktop API helpers for loading/deleting workflow bundles, deleting tools,
  and deleting MCP servers
- desktop UI actions for deleting servers/tools/workflows and loading saved
  workflow YAML back into the editor
- release smoke evidence that the generated package exercises those CRUD paths

The visible product names remain unchanged:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old ArtLoom management sources:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\lib.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\mcp_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\workflow_store.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src\services\runtimeBridge.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-manager\hooks\useWorkflows.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-manager\components\WorkflowList.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\components\Canvas.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\pages\settings\MCPSettings.tsx`

Old Tauri command registration in `src-tauri/src/lib.rs` included:

```text
workflow_store::list_workflows
workflow_store::save_workflow_metadata
workflow_store::save_workflow_data
workflow_store::load_workflow_data
workflow_store::delete_workflow_data
mcp_engine::get_mcp_servers
mcp_engine::save_mcp_server
mcp_engine::delete_mcp_server
```

Old `mcp_engine.rs` persisted MCP server configuration and exposed:

```text
get_mcp_servers
save_mcp_server
delete_mcp_server
```

Old `workflow_store.rs` persisted metadata and workflow YAML and exposed:

```text
save_workflow_data
load_workflow_data
delete_workflow_data
list_workflows
save_workflow_metadata
```

Old runtime bridge and Hook IPC compatibility kept those workflow store
operations available in both desktop and external runtime paths:

```text
art_loom/list_workflows
art_loom/save_workflow_data
art_loom/load_workflow_data
art_loom/delete_workflow_data
```

Old desktop UI evidence:

- `MCPSettings.tsx` loaded, saved, toggled, and deleted MCP servers through
  `get_mcp_servers`, `save_mcp_server`, and `delete_mcp_server`.
- `useWorkflows.ts` used `delete_workflow_data`, `load_workflow_data`, and
  `save_workflow_data` for workflow management.
- `WorkflowList.tsx` exposed delete actions for saved workflows.
- `Canvas.tsx` loaded workflow YAML through `load_workflow_data` before
  entering the visual editor.

## Loom state before Phase 25

Before this phase, Loom had save/list management paths from earlier parity
work, including:

- `GET /v1/mcp/servers`
- `PUT /v1/mcp/servers/{serverId}`
- `GET /v1/tools`
- `PUT /v1/tools/{toolId}`
- `GET /v1/workflows`
- `PUT /v1/workflows/{workflowId}`

Missing relative to old ArtLoom:

- no daemon route for `DELETE /v1/mcp/servers/{serverId}`
- no daemon route for `DELETE /v1/tools/{toolId}`
- no daemon route for `GET /v1/workflows/{workflowId}`
- no daemon route for `DELETE /v1/workflows/{workflowId}`
- no desktop Tauri bridge for daemon DELETE calls
- no desktop helpers for loading saved workflow YAML or deleting saved
  workflows/tools/MCP servers
- no desktop UI actions equivalent to old management delete/load paths
- no release smoke proof for management deletion/load parity

## Phase 25 implementation

### Daemon management routes

Updated:

```text
Loom/apps/daemon/src/lib.rs
```

Daemon help now includes:

```text
DELETE /v1/mcp/servers/{serverId}
DELETE /v1/tools/{toolId}
GET  /v1/workflows/{workflowId}
DELETE /v1/workflows/{workflowId}
```

New route handlers:

```text
delete_mcp_server
delete_tool
get_workflow
delete_workflow
```

`GET /v1/workflows/{workflowId}` returns metadata plus the saved YAML payload:

```json
{
  "workflow": {
    "id": "release-workflow",
    "name": "Release Workflow",
    "nodeCount": 1,
    "updatedAt": "...",
    "data": "name: Release Workflow\n..."
  }
}
```

Deletion routes return explicit `{ deleted: true }` success payloads and 404
structured errors when the requested item does not exist.

### Desktop Tauri DELETE bridge

Updated:

```text
Loom/apps/desktop/src-tauri/src/lib.rs
```

Added Tauri command:

```rust
delete_loom_daemon_json(base_url: String, path: String) -> Result<Value, String>
```

Added internal helper:

```rust
http_delete_json(base_url: &str, path: &str) -> Result<Value, String>
```

The command is registered in the desktop invoke handler, matching the existing
GET/PUT/POST daemon JSON bridge pattern.

### Desktop API helpers

Updated:

```text
Loom/apps/desktop/src/services/loomApi.ts
```

Added workflow bundle interfaces:

```ts
export interface LoomWorkflowBundle extends LoomWorkflowMetadata {
  data: string;
}

export interface LoomWorkflowBundleResponse {
  workflow?: LoomWorkflowBundle;
}
```

Added DELETE helpers and exported management functions:

```text
deleteJsonViaTauri
deleteJson
getWorkflowBundle
deleteWorkflowBundle
deleteToolDefinition
deleteMcpServer
```

Browser mode uses direct `fetch(..., { method: "DELETE" })`; desktop mode uses
the new Tauri command.

### Desktop management UI

Updated:

```text
Loom/apps/desktop/src/App.tsx
```

Restored practical management actions:

- MCP panel server cards now expose `Delete server`.
- Registry tool cards now expose `Delete tool`.
- Workflow Studio saved workflow rows now expose `Load YAML`.
- Workflow Studio saved workflow rows now expose `Delete workflow`.

`Load YAML` calls `getWorkflowBundle()` and writes the saved YAML back into the
editor. Delete actions call the corresponding API helper and then refresh the
snapshot.

### Parity contract

Updated:

```text
scripts/tests/test-loom-artloom-parity-contract.ps1
```

The contract now asserts the restored source and release-smoke surfaces:

```text
DELETE /v1/mcp/servers/{serverId}
DELETE /v1/tools/{toolId}
GET  /v1/workflows/{workflowId}
DELETE /v1/workflows/{workflowId}
DELETE /v1/mcp/servers/fixture-delete
DELETE /v1/tools/fixture-delete-tool
GET /v1/workflows/release-workflow
DELETE /v1/workflows/fixture-delete-workflow
getWorkflowBundle
deleteWorkflowBundle
deleteToolDefinition
deleteMcpServer
deleteJsonViaTauri
delete_loom_daemon_json
Load YAML
Delete workflow
Delete tool
Delete server
```

### Release smoke

Updated:

```text
scripts/smoke-release-local-apps.ps1
```

The generated release smoke now performs these management operations against
the packaged daemon:

```text
DELETE /v1/mcp/servers/fixture-delete
DELETE /v1/tools/fixture-delete-tool
GET /v1/workflows/release-workflow
DELETE /v1/workflows/fixture-delete-workflow
```

It also verifies the non-loopback bearer-token guard for the new DELETE paths.

Smoke summary evidence:

```json
{
  "managementCrud": {
    "mcpServerDeleted": true,
    "toolDeleted": true,
    "workflowLoaded": "release-workflow",
    "workflowDeleted": true
  }
}
```

## Validation

Fresh validation after implementation and formatting:

```text
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_and_writes_mcp_servers --offline -- --nocapture --test-threads=1
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_reads_and_writes_tool_and_workflow_contracts --offline -- --nocapture --test-threads=1
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1
```

All commands passed. The prefix regression check returned no matches.

Generated release:

```text
release\Loom\loom-management-crud-phase25
```

Formal verification:

```text
status = passed
gitHead = 022c1fb386c868628556a0c12667f45ad40d6e05
gitDirty = false
checksumEntries = 31
```

Package:

```text
packages\Loom-loom-management-crud-phase25-windows-x64.zip
size = 50034898 bytes
sha256 = 7ad2fe1a3d853b7c55371065f630e0247a0d4f6c5f2a89150ba4c4fce0c95425
```

Release smoke:

```text
output\smoke\runs\20260612-202655-Loom-78112-ccf2bdab51cd4c60956e0a2d653bb652\release-local-apps-loom-management-crud-phase25-Loom-summary.json
output\smoke\latest\release-local-apps-loom-management-crud-phase25-Loom-summary.json
```

Smoke evidence includes:

```text
controlPlane.managementCrud.mcpServerDeleted = true
controlPlane.managementCrud.toolDeleted = true
controlPlane.managementCrud.workflowLoaded = "release-workflow"
controlPlane.managementCrud.workflowDeleted = true
pythonArtCatalog.artId = "loom_echo"
pythonArtToolExecution = "python art saw release installed python art"
pythonToolExecution.packagedPython = true
workflowToolExecution = "script saw release workflow runtime"
cloudMultipartArtNode.multipartSeen = true
realOcrImage.fullTextLength = 63
```

## Non-goals

Phase 25 intentionally does not restore every old higher-level UI around these
stores.

Still pending for later source-backed decisions:

- full old MCP Registry marketplace/install UI from `MCPSettings.tsx`
- full old ReactFlow visual graph editor from `features/workflow-editor`
- old Python source editing/import helpers beyond Phase 24's installed catalog
  discovery, import, and launcher-backed execution path
- final full-source audit matrix proving no other product-critical ArtLoom
  surfaces remain omitted
