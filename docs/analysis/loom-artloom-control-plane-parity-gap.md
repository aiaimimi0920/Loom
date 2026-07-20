# Loom ArtLoom Control-Plane Parity Gap

Date: 2026-06-12

## Summary

The current `Neuro/Loom` implementation is not feature-equivalent to the local
ArtLoom control plane at `Z:\project\project\ArtNexus\ArtLoom`.

The previous Loom migration intentionally focused on a headless Rust runtime and
classified multiple ArtLoom capabilities as deferred or out of scope. That
scope decision is now incorrect for the product target. Loom must restore the
local control-plane capabilities that ArtLoom already provided, while preserving
the newer Loom daemon-first architecture and Loom-only product naming.

This document records the concrete omissions and their target restoration
layers.

## Source evidence

### ArtLoom role

`Z:\project\project\ArtNexus\ArtLoom\README.md` describes ArtLoom as:

- an ArtHook scheduling terminal;
- a registry and workflow hub;
- an IPC backend;
- a control plane for settings, engines, MCP, and instantiation entry points.

The same README documents the main loop:

1. edit a workflow in the ArtLoom editor;
2. load/save `%APPDATA%\ArtNexus\workflows\*.yaml`;
3. broadcast the workflow to ArtHook through port `19820`;
4. accept parameter updates from ArtHook and write them back to YAML;
5. refresh the ArtLoom editor through `workflow_updated`.

### MCP implementation

Old implementation:

- Frontend: `Z:\project\project\ArtNexus\ArtLoom\src\pages\settings\MCPSettings.tsx`
- Marketplace data: `Z:\project\project\ArtNexus\ArtLoom\src\features\mcp\marketplace.ts`
- Backend: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\mcp_engine.rs`

Old MCP backend provides:

- `fetch_mcp_registry`
- `test_mcp_connection`
- `call_mcp_tool`
- `install_mcp_package`
- `check_mcp_package_installed`
- `get_mcp_servers`
- `save_mcp_server`
- `delete_mcp_server`
- synchronous tool execution for workflow/IPC worker threads

Current Loom has no equivalent MCP crate, daemon API, desktop page, or release
smoke.

### Tool / Art registry implementation

Old implementation:

- Backend: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\art_registry.rs`
- Frontend hook: `Z:\project\project\ArtNexus\ArtLoom\src\features\art-registry\hooks\useArtsRegistry.ts`
- Add flow: `Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx`

Old registry supported execution types:

- `cli_wrapper`
- `cloud_api`
- `script`
- `mcp`
- `workflow`

Current Loom has agent specs and workflow graphs, but no user-managed tool/art
registry or MCP-backed tool definition layer.

### Hook bridge / sync implementation

Old implementation:

- IPC backend: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs`
- Protocol definitions: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ahrp.rs`
- Session reader: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\session_manager.rs`

Old bridge provided:

- WebSocket listener on `127.0.0.1:19820`
- `handshake`
- `list_arts`
- `get_enabled_arts`
- `art_loom/get_user_arts`
- `art_loom/get_capabilities`
- `art_loom/instantiate_workflow`
- `art_loom/update_workflow_node`
- `art_loom/overwrite_workflow`
- `art_loom/workflow_updated` broadcasts
- `art_loom/arts_updated` broadcasts
- `art_loom/execute_art_node`

Current `loom_hooks` only models internal run/tool lifecycle dispatch. It is not
a Hook bridge and does not restore the ArtLoom-ArtHook synchronization loop.

### Workflow store / codec implementation

Old implementation:

- Store: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\workflow_store.rs`
- Codec: `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\workflow_codec.rs`

Old workflow layer provided:

- `%APPDATA%\ArtNexus\workflows`
- `arthook-live` mapped to `latest.yaml`
- workflow metadata index
- save/load/list/delete workflow commands
- YAML to graph JSON conversion
- graph JSON to YAML conversion
- graph snapshot merge
- parameter normalization

Current `loom_workflow::artloom` only converts a selected YAML subset into a
native Loom graph. It does not provide the interactive workflow store/editor
roundtrip required by the old control plane.

### Desktop UI implementation

Old ArtLoom has feature directories for:

- `art-registry`
- `mcp`
- `workflow-editor`
- `workflow-manager`

Current Loom desktop only contains:

- `App.tsx`
- `styles.css`
- `services/loomApi.ts`

It is a thin daemon status shell and does not expose the old control-plane
features.

## Required restoration layers

| Capability | Current Loom state | Required target |
| --- | --- | --- |
| MCP server config CRUD | Missing | `loom_mcp` crate plus daemon and desktop surfaces |
| MCP registry fetch | Missing | `loom_mcp::registry_url` and daemon endpoint |
| MCP stdio initialize/tools/list/tools/call | Missing | `loom_mcp::McpClient` with process sandbox policy |
| MCP marketplace | Missing | Desktop marketplace data and install/test UI |
| Tool/Art registry | Missing | `loom_tool_registry` crate and daemon CRUD API |
| MCP-backed tool execution | Missing | Registry entries can call configured MCP tools |
| Workflow store | Missing | `loom_workflow_store` crate with YAML files and metadata index |
| Graph JSON <-> YAML codec | Partial only | `loom_workflow_store` codec preserving interactive editor data |
| Live workflow alias | Missing | `hook-live` plus compatibility alias `arthook-live` |
| Hook bridge | Missing | `loom_hook_bridge` crate or daemon module with WS protocol |
| 19820 compatibility | Missing | Configurable bridge port, default compatible mode for old Hook clients |
| Workflow instantiation broadcast | Missing | `hook/instantiate` plus compatibility `art_hook/instantiate` |
| Node update write-back | Missing | `hook/update_workflow_node` plus compatibility `art_loom/update_workflow_node` |
| Desktop MCP settings | Missing | Loom desktop MCP page |
| Desktop workflow manager/editor shell | Missing | Loom desktop workflow pages backed by daemon APIs |

## Scope correction

These capabilities are now in scope for Loom parity restoration:

- MCP server management and stdio tool invocation.
- Tool/Art registry for MCP and workflow-backed tools.
- Workflow store and graph codec roundtrip.
- Hook bridge and compatibility with the old ArtLoom/ArtHook IPC method names.
- Desktop control-plane pages for MCP, registry, workflows, and bridge status.

These remain deferred until after parity restoration:

- OCR/image enhancement runtime.
- Embedded Python packaging.
- Shared-memory zero-copy image transport.
- Cloud/provider-specific image execution.
- Gateway provider routing internals.

## Acceptance evidence

The restoration is complete only when local tests and release smoke prove:

1. Loom can persist MCP server configs and list them through daemon APIs.
2. Loom can perform a stdio MCP initialize and tools/list against a fixture
   server.
3. Loom can call a fixture MCP tool and receive structured content.
4. Loom can persist a tool registry entry backed by an MCP tool.
5. Loom can save/load/list/delete workflow YAML with graph JSON roundtrip.
6. Loom can start a Hook bridge and answer `handshake`.
7. Loom can broadcast workflow instantiation to a connected client.
8. Loom can process a node update from the bridge and write the updated workflow
   YAML.
9. Loom desktop exposes MCP, registry, workflow, and bridge status surfaces.
10. Loom release contains `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`
    and the release smoke covers the new parity contracts.
