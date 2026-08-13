# Final ArtLoom Parity Matrix

> Historical parity audit only. Phase 70 removed the production compatibility
> routes, method aliases, converter, and AHRP execution path catalogued here.
> The current Loom<->Hook contracts are `loom.hook.v1` and `loom.surface.v1`;
> this file must not be used as current API documentation.

Date: 2026-06-18

## Executive conclusion

Updated source and user-visible UX audit result after Phase 31:

- Phase 30 restored runtime/API/package parity but was not sufficient for
  user-visible desktop parity.
- The user-visible gaps reported after Phase 30 are now restored in Phase 31:
  old Art Node language, old Add Art entry routes, manual MCP server linking,
  desktop Hook sync/broadcast affordances, and improved Loom desktop UI
  hierarchy/contrast.
- Product-critical ArtLoom capabilities currently requested for Loom are
  restored.
- Follow-up parity audit on 2026-06-18 closed a `get_user_arts` protocol
  shape drift: Hook Bridge `art_loom/get_user_arts` and daemon
  `/v1/artloom-compat/user-arts` now return the old ArtLoom frontend card
  fields (`category`, `version`, `author`, `status`, `iconColor`,
  `downloads`, `owned`, `executionType`, `autoProcess`, `inputs`, `outputs`)
  instead of the internal Loom compat-tool shape; synced `autoProcess` /
  `auto_process` is preserved in compat metadata.
- Release used by this audit:
  `release\Loom\20260618-182237-f399a0fc`
- Audit packaged executables:
  - `loom.exe`
  - `loom-daemon.exe`
  - `loom-desktop.exe`
- Audit package:
  `release\Loom\20260618-182237-f399a0fc\packages\Loom-20260618-182237-f399a0fc-windows-x64.zip`
- Audit package sha256:
  `70d8298fe4c74c89a06060b1939f639ddebd75a036424408007e1e8cb3da9f2d`
- Audit formal verify:
  blocked by the active dirty worktree manifest policy
  (`Manifest gitDirty must be false for a formal release package`).
- Audit release smoke:
  `status = passed`,
  `output\smoke\runs\20260618-182747-Loom-76172-4a77cb1c97b04d91ac42392139c9d106\release-local-apps-20260618-182237-f399a0fc-Loom-summary.json`

No remaining product-critical parity gap is currently known after Phase 31.

There are still old ArtLoom implementation details that Loom intentionally does
not clone:

- exact old Tauri symbols as desktop-local invoke names
- actual OS startup registration and tray side effects

These are classified below as behaviorally replaced, compatibility-only, or
non-product-critical/obsolete. They do not block the current Loom migration
because the corresponding product-critical runtime paths are covered by daemon
APIs, Hook Bridge WebSocket compatibility, desktop UI, packaged resources, and
release smoke evidence.

## Audit inputs

Old ArtLoom reference, read-only:

```text
Z:\project\project\ArtNexus\ArtLoom
```

Old source files sampled:

```text
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\lib.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\art_registry.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\settings.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\system_settings.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\ipc_service.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\native_engine.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\python_engine.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\shared_memory.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\session_manager.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\workflow_store.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\workflow_codec.rs
Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\mcp_engine.rs
Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx
Z:\project\project\ArtNexus\ArtLoom\src\features\art-registry\*
Z:\project\project\ArtNexus\ArtLoom\src\features\mcp\marketplace.ts
Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\*
Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-manager\*
Z:\project\project\ArtNexus\ArtLoom\src\pages\settings\*
Z:\project\project\ArtNexus\ArtLoom\src\hooks\useAppSettings.ts
```

Current Loom evidence sources:

```text
Loom/apps/daemon/src/lib.rs
Loom/apps/desktop/src/App.tsx
Loom/apps/desktop/src/services/loomApi.ts
Loom/apps/desktop/src/services/mcpMarketplace.ts
Loom/apps/desktop/src/services/pythonArtSource.ts
Loom/apps/desktop/src/services/workflowStudio.ts
Loom/apps/desktop/src-tauri/src/lib.rs
Loom/crates/loom_hook_bridge/src/lib.rs
Loom/crates/loom_tool_registry/src/lib.rs
Loom/crates/loom_configuration/src/store.rs
Loom/resources/*
scripts/build-release-exes.ps1
scripts/smoke-release-local-apps.ps1
scripts/tests/test-loom-artloom-parity-contract.ps1
scripts/tests/test-build-release-exes-contract.ps1
scripts/tests/test-loom-desktop-shell-contract.ps1
docs/loom/progress/phase-8..phase-31
docs/loom/analysis/phase-31-artloom-ux-parity-audit.md
```

## Status legend

| Status | Meaning |
| --- | --- |
| Restored directly | The old external API/protocol/resource name or behavior exists in Loom. |
| Restored behaviorally | Loom exposes equivalent product behavior through the daemon-first architecture or desktop shell rather than the old Tauri command symbol. |
| Compatibility-only | Loom returns enough old-shape data for legacy clients, but the new source of truth is the Loom daemon/configuration model. |
| Intentionally replaced | Old implementation side effects were replaced by safer or more maintainable Loom architecture. |
| Non-product-critical / obsolete | Old code path is not used by the current product-critical migration flow. |
| Product-critical missing | Must be fixed before claiming migration completeness. |

## Old Tauri command group matrix

| Old group / commands | Current Loom mapping | Status | Evidence | Notes |
| --- | --- | --- | --- | --- |
| `art_registry::list_arts`, `get_art` | `/v1/artloom-compat/arts`, compat-managed `ToolDefinition` entries, Hook Bridge `list_arts`, `art_loom/get_user_arts` | Restored behaviorally | `Loom/apps/daemon/src/lib.rs`, `Loom/crates/loom_tool_registry/src/lib.rs`, `Loom/crates/loom_hook_bridge/src/lib.rs`, release smoke `hookBridgeMethods` | Old "Art" is a compat-managed Loom tool. Native Loom tools remain in `/v1/tools` but no longer get exposed as old ArtLoom user Arts. |
| `art_registry::enable_art`, `disable_art`, `update_art_defaults` | `ToolDefinition.enabled`, `PUT /v1/tools/{toolId}`, Hook Bridge `get_enabled_arts`, daemon WebSocket `update_art_param` persistence | Restored behaviorally | `Loom/crates/loom_tool_registry/src/lib.rs` has `enabled`; `Loom/apps/daemon/src/lib.rs` handles persistent `update_art_param`; release smoke `hookBridgeSettings.updatedArtId = "fixture-artloom-compat"`, `updatedStrength = 0.5` | Loom persists the full tool definition and updates the compat Art defaults when Hook sends a param mutation. |
| `art_registry::sync_user_arts`, `get_user_arts` | HTTP `POST /v1/artloom-compat/arts/sync`, daemon `GET /v1/artloom-compat/user-arts`, Hook Bridge `sync_user_arts` / `art_loom/sync_user_arts`, `art_loom/get_user_arts` | Restored behaviorally | `Loom/apps/daemon/src/lib.rs`, `Loom/crates/loom_hook_bridge/src/lib.rs`, `Loom/apps/desktop/src/App.tsx`, `scripts/smoke-release-local-apps.ps1` | Payload sync now mirrors old ArtLoom semantics: replace sync-managed user Arts from `arts=[...]`, preserve native Loom tools, persist metadata/defaults/ports including `autoProcess`, and broadcast `art_loom/arts_updated`. `get_user_arts` now returns old frontend registry-card fields (`category`, `version`, `author`, `status`, `iconColor`, `downloads`, `owned`, `executionType`, `autoProcess`, `inputs`, `outputs`) while `list_arts` keeps Loom's compat-tool shape. Empty sync remains a safe mirror/broadcast operation. |
| `settings::get_settings`, `get_shortcuts` | Hook Bridge `get_settings`, `get_shortcuts`; desktop Settings links to daemon settings pages | Restored compatibility-only | Phase 29 plus follow-up audit; release smoke `hookBridgeSettings.settingsTheme = "system"`, `shortcutCount = 7`; `Loom/apps/desktop/src/App.tsx` settings panel | Old data shape is returned for ArtHook compatibility, including old ArtHook copy/paste/save/OCR/translation shortcuts. Loom managed configuration remains separate. |
| `settings::update_settings`, `update_shortcut` | `/v1/artloom-compat/settings`, `/v1/artloom-compat/shortcuts/{shortcutId}`, Hook Bridge `get_settings`, `get_shortcuts`, `sync_shortcuts` read the same compat store | Restored behaviorally | `Loom/apps/daemon/src/lib.rs`; daemon test `daemon_exposes_artloom_settings_shortcuts_and_safe_system_contracts`; release smoke `hookBridgeSettings.shortcutCount = 7` | The compatibility store is Loom-owned, persisted on disk, and now synchronized into Hook Bridge reads instead of returning static defaults. |
| `settings::get_app_paths` | Desktop/daemon settings links and Loom storage roots; default configuration root now `%APPDATA%\Loom\configuration\apps` | Restored behaviorally | `Loom/apps/desktop/src-tauri/src/lib.rs`, `Loom/crates/loom_configuration/src/store.rs`, Phase 30 | Exact Tauri command not cloned; paths are surfaced through daemon/desktop shell. |
| `system_settings::enable_autostart`, `disable_autostart`, `is_autostart_enabled`, `set_autostart`, `set_minimize_to_tray` | GET/POST `/v1/artloom-compat/system/autostart`, POST `/v1/artloom-compat/system/autostart/enable`, POST `/v1/artloom-compat/system/autostart/disable`, POST `/v1/artloom-compat/system/minimize-to-tray`, and desktop System Settings controls | Restored compatibility-only / intentionally side-effect-safe | `Loom/apps/daemon/src/lib.rs`; `Loom/apps/desktop/src/App.tsx`; release smoke `artLoomSystemCompat` verifies the legacy command language and `sideEffect = false` | Loom persists compatibility preferences but does not perform actual OS autostart or tray registration. |
| `ipc_service::get_ipc_status` | `GET /v1/hook-bridge/status` | Restored behaviorally | `Loom/apps/daemon/src/lib.rs`; release smoke covers `/v1/hook-bridge/status` | Daemon controls Hook Bridge lifecycle. |
| `ipc_service::instantiate_workflow`, `execute_art_node`, `broadcast_arts_updated` | Hook Bridge `art_loom/instantiate_workflow`, `art_loom/execute_art_node`, broadcasts `art_hook/instantiate`, `art_loom/workflow_updated`, `art_loom/arts_updated` | Restored directly | `Loom/crates/loom_hook_bridge/src/lib.rs`; release smoke `websocketBroadcast`, `hookLiveWorkflow`, `executeArtNode`, `workflowArtNode` | Legacy method names remain; Phase 37 proves instantiate persistence to `hook-live`, not only broadcast. |
| `native_engine::native_process_art` | Native image filter runtime under registry and AHRP | Restored behaviorally | Phase 14; release smoke `nativeImageFilter.outputChanged = true` | The old command symbol is replaced by registry-backed runtime. |
| `python_engine::execute_python_art`, `python_process_image` | Python Art execution type and `python/Launcher.py` | Restored behaviorally | Phase 24; `pythonArtToolExecution = "python art saw release installed python art"` | Packaged Python Art catalog is restored under Loom resources. |
| `python_engine::python_engine_status` | Packaged Python script smoke inspects actual executable; daemon runtime resolution | Restored behaviorally | Phase 22 and Phase 30 smoke `pythonToolExecution.packagedPython = true` | Status is proved through execution path rather than a standalone Tauri status command. |
| `python_engine::read_art_json`, `read_python_file`, `check_art_json_nearby` | `POST /v1/python-arts/source/read`, `/read-art-json`, `/check-art-json` | Restored behaviorally | Phase 28; release smoke `pythonArtSourceImport.nearbyArtJsonFound = true` | Safer daemon helpers with `.py`/`art.json` and size bounds. |
| `python_engine::list_installed_arts` | `GET /v1/python-arts`, desktop Installed Python Arts UI | Restored behaviorally | Phase 24; `pythonArtCatalog.count = 1`, `pythonArtCatalog.artId = "loom_echo"` | |
| `python_engine::prefetch_shader` | POST /v1/python-arts/shader/prefetch plus desktop `prefetch_shader` action | Restored compatibility-only | `Loom/apps/daemon/src/lib.rs`; `Loom/apps/desktop/src/App.tsx`; release smoke `pythonEngineCompat.prefetchCommand = "prefetch_shader"` | Exact `compatCommand = "prefetch_shader"` is preserved through the Python Art runtime. |
| `shared_memory::shm_create_buffer`, `shm_release_buffer`, `shm_list_buffers`, `shm_get_buffer_info` | `/v1/shared-images` create/list/get/delete and AHRP `shared_memory` input/output | Restored behaviorally | Phase 18; release smoke `sharedImageAhrpProcess.outputType = "shared_memory"` | The external protocol still uses `shared_memory`; backing implementation is Loom-owned. |
| `session_manager::read_arthook_session` | GET /v1/hook-bridge/session plus desktop ArtHook session panel | Restored compatibility-only | `Loom/apps/daemon/src/lib.rs`; `Loom/apps/desktop/src/App.tsx`; release smoke `hookSessionCompat` verifies the method, protocol, availability, and snapshot counts | Loom reads legacy `session.json` when present and safely returns a snapshot; live Hook Bridge remains the runtime source. |
| `workflow_store::list_workflows`, `save_workflow_metadata`, `save_workflow_data`, `load_workflow_data`, `delete_workflow_data` | `GET/PUT/DELETE /v1/workflows`, Hook Bridge `art_loom/list_workflows`, `save_workflow_data`, `load_workflow_data`, `delete_workflow_data` | Restored directly / behaviorally | Phase 17, Phase 25; release smoke `managementCrud.workflowLoaded`, `workflowDeleted`, `workflowArtNode`, `workflowAhrpProcess` | Old metadata/data split is mapped onto Loom workflow bundles/YAML. |
| `mcp_engine::test_mcp_connection` | `POST /v1/mcp/test` | Restored behaviorally | Phase 26; `mcpMarketplace.connectionTestSuccess = true`, `connectionTestTool = "echo"` | |
| `mcp_engine::call_mcp_tool` | `/v1/tools/{toolId}/execute`, Hook Bridge execute Art node, AHRP process through MCP-backed tools | Restored behaviorally | Phase 12, Phase 13; release smoke `mcpToolExecution`, `executeArtNode`, `ahrpProcess` | |
| `mcp_engine::install_mcp_package`, `check_mcp_package_installed` | Marketplace saves stdio server configs; `npx`, `uvx`, or `docker` resolve packages at runtime | Intentionally replaced | Phase 26 audit; desktop `Install server`, `Install & Test`; daemon `/v1/mcp/registry`, `/v1/mcp/test` | Avoids direct `pip install` side effects in the app. |
| `mcp_engine::fetch_mcp_registry` | `GET /v1/mcp/registry`, `LOOM_MCP_REGISTRY_ENDPOINT` | Restored behaviorally | Phase 26; smoke `registryServerName = "io.modelcontextprotocol/fixture"` | |
| `mcp_engine::get_mcp_servers`, `save_mcp_server`, `delete_mcp_server` | `GET/PUT/DELETE /v1/mcp/servers/{serverId}` and desktop Configured servers UI | Restored behaviorally | Phase 25/26; smoke `managementCrud.mcpServerDeleted = true` | |

## Old frontend feature directory matrix

| Old feature directory | Current Loom mapping | Status | Evidence | Notes |
| --- | --- | --- | --- | --- |
| `src/components/AddArtModal.tsx` | Desktop `AddArtWizard` in Registry with `CLI wrapper Art`, `Cloud API Art`, `Script / Python Art`, `MCP-linked Art`, `Installed Python Art`, `Workflow-backed Art`, and `Native Image Art` | Restored behaviorally in Phase 31 | `Loom/apps/desktop/src/App.tsx`, `createArtToolFromWizard`, browser screenshot `output\smoke\phase31-ui\loom-phase31-registry-add-art-ui.png`, UI contract assertions | Old AntD modal is replaced by an inline Loom workbench wizard that writes daemon tool definitions. |
| `src/features/art-registry` | Desktop `RegistryPanel`, daemon `/v1/tools`, `ToolRegistry`, Python Art catalog, source import, and visible Add Art wizard | Restored behaviorally | `Loom/apps/desktop/src/App.tsx`, `Loom/crates/loom_tool_registry/src/lib.rs`, Phase 24/25/28/31 evidence | Old AntD list/header/hooks are replaced by Loom workbench UI. |
| `src/features/mcp/marketplace.ts` | `Loom/apps/desktop/src/services/mcpMarketplace.ts` with curated templates, registry mapping, config builder | Restored behaviorally | Phase 26; contract asserts `MCP_MARKET_SERVERS`, `mapRegistryResponseToMarketplace`, `buildMarketplaceServerConfig` | |
| `src/features/workflow-editor/components/nodes/ArtNode.tsx` | Desktop Workflow Studio `art-node-card` with `Preview`, `Inputs`, `Outputs`, `Params`, and `Result` labels | Restored behaviorally in Phase 31 | `Loom/apps/desktop/src/App.tsx`, browser screenshot `output\smoke\phase31-ui\loom-phase31-workflow-art-node-ui.png`, UI contract assertions | Old ReactFlow node implementation is not cloned, but the Art Node concept is visible again. |
| `src/features/workflow-editor/components/Sidebar.tsx` | Desktop Workflow Studio `Art node palette` and `Add Art node` action backed by daemon registry tools | Restored behaviorally in Phase 31 | `Loom/apps/desktop/src/App.tsx`, DOM check for `Art node palette` and `Add Art node` | Empty state still shows the add-node affordance so the flow is visible when daemon data is offline. |
| `src/features/workflow-editor` | Desktop Workflow Studio graph view, Art Node cards, Art node palette, edge chips, node properties editor, YAML serialization helpers | Restored behaviorally | Phase 27 plus Phase 31 browser validation and contract assertions | ReactFlow itself is intentionally not cloned. |
| `src/features/workflow-manager` | Desktop Workflow Studio and management CRUD: load/save/delete YAML, wrap workflow as Loom tool | Restored behaviorally | Phase 23/25; smoke `managementCrud.workflowLoaded`, `workflowDeleted`, `workflowToolExecution` | |

## Old settings pages / hook matrix

| Old page/hook | Current Loom mapping | Status | Evidence | Notes |
| --- | --- | --- | --- | --- |
| `src/hooks/useAppSettings.ts` | Daemon ArtLoom compat settings store plus Hook Bridge settings/shortcut reads | Restored behaviorally for compat settings | `Loom/apps/daemon/src/lib.rs`; `/v1/artloom-compat/settings`; Hook Bridge `get_settings`, `get_shortcuts`, `sync_shortcuts` | Old Tauri local settings file is not cloned, but the old settings shape is persisted and visible to Hook. |
| `GeneralSettings.tsx` | Desktop Settings panel links to `/settings`, `/settings/tea`, `/settings/hook`, `/settings/talk` | Restored behaviorally for navigation; not exact clone | `Loom/apps/desktop/src/App.tsx`, `Loom/apps/desktop/src-tauri/src/lib.rs` | |
| `EngineSettings.tsx` | Python executable/runtime handled by packaged `bin/python-embed`, `LOOM_PYTHON`, Python Art catalog/source helpers | Restored behaviorally | Phase 22/24/28 smoke | Old ComfyUI-specific settings are not product-critical for Loom migration. |
| `HotkeySettings.tsx` | Hook Bridge `get_shortcuts`, `sync_shortcuts` compatibility payload plus desktop Settings shortcut editor | Restored behaviorally / compatibility payload | Phase 29 plus follow-up audit; smoke `shortcutCount = 7`, `toggle_translation`, `synced = true` | Hook Bridge now exposes the old ArtHook shortcut set; desktop provides a Loom-native shortcut editing surface. |
| `MCPSettings.tsx` | Desktop MCP panel with Configured servers, Manual MCP server, Save MCP server, Connect MCP server, MCP Marketplace, Refresh Registry, Install server, Install & Test, Delete server | Restored behaviorally; manual linking made visible in Phase 31 | Phase 26 plus Phase 31; desktop contract, browser screenshot, release smoke | Old command/args/env manual server workflow is now visible alongside marketplace installs. |
| `SystemSettings.tsx` | GET/POST `/v1/artloom-compat/system/autostart`, POST `/v1/artloom-compat/system/autostart/enable`, POST `/v1/artloom-compat/system/autostart/disable`, POST `/v1/artloom-compat/system/minimize-to-tray`, and desktop System Settings controls | Restored compatibility-only / intentionally side-effect-safe | `Loom/apps/desktop/src/App.tsx`, `Loom/apps/desktop/src/services/loomApi.ts`; release smoke `artLoomSystemCompat` | The UI persists compatibility preferences while actual OS startup registration and tray side effects remain intentionally replaced. |

## Protocol and runtime surface matrix

| Protocol/runtime surface | Current Loom status | Evidence |
| --- | --- | --- |
| Hook WebSocket on `127.0.0.1:19820` | Restored directly | Phase 10/11; release smoke `hookBridgePort = 19820`, `websocketHandshake.hasSessionId = true` |
| `handshake` | Restored directly | `hookBridgeMethods` includes `handshake`; smoke `websocketHandshake.type = "handshake"` |
| `list_arts`, `get_enabled_arts`, `art_loom/get_user_arts`, `art_loom/get_capabilities` | Restored directly | `Loom/crates/loom_hook_bridge/src/lib.rs`; release smoke method list |
| `get_settings`, `get_shortcuts`, `update_art_param`, `sync_shortcuts` | Restored directly; `update_art_param` persists compat Art defaults | smoke `hookBridgeSettings.settingsTheme = "system"`, `shortcutCount = 7`, `updatedArtId = "fixture-artloom-compat"`, `updatedStrength = 0.5`, `synced = true` |
| `art_loom/instantiate_workflow` and `art_hook/instantiate` broadcast | Restored directly | smoke `websocketBroadcast.method = "art_hook/instantiate"` plus `hookLiveWorkflow.workflowId = "hook-live"` and `hookLiveWorkflow.edgePersisted = true` |
| `art_loom/update_workflow_node`, `art_loom/overwrite_workflow`, `workflow_updated`, `arts_updated` | Restored directly | contract asserts legacy method names and broadcasts |
| `art_loom/execute_art_node` | Restored directly | smoke `executeArtNode.success = true` |
| AHRP `art/process` | Restored directly | smoke `ahrpProcess.status = "Success"` |
| `art/update_property` | Restored compatibility acknowledgement | `Loom/crates/loom_hook_bridge/src/lib.rs` |
| OCR `art_loom/ocr_image` | Restored directly with fixture and real OCR providers | smoke `ocrImage.fullText = "release loom ocr"`, `realOcrImage.fullTextLength = 63` |
| Translation `art_loom/translate_text` | Restored with configurable provider-backed translation via `LOOM_TRANSLATE_ENDPOINT`, with passthrough fallback when unset | smoke `hookBridgeSettings.translatedText = "translated:release loom translate:zh"`, `translationSource = "loom-translate-provider"` |
| `shared_memory` AHRP input/output | Restored directly at protocol level | smoke `sharedImageAhrpProcess.outputType = "shared_memory"` |
| `image_path`, `image_base64`, `image_buffer` helper surfaces | Restored behaviorally | smoke `imageHelperConvert.inputType = "image_base64"`, `outputType = "image_buffer"` |
| Native image filter | Restored behaviorally | smoke `nativeImageFilter.outputChanged = true` |
| Script/Python runtime | Restored behaviorally | smoke `scriptToolExecution`, `pythonToolExecution.packagedPython = true` |
| Shader text output | Restored behaviorally | smoke `scriptShaderArt.outputText = "void fragment() { COLOR = vec4(1.0); }"` |
| Cloud API runtime | Restored behaviorally | smoke `cloudToolExecution`, `cloudArtNode`, `cloudAhrpProcess` |
| Old cloud multipart/template behavior | Restored behaviorally | smoke `cloudMultipartArtNode.multipartSeen = true`, `unresolvedTemplateSeen = false`; `ToolRegistry` supports `url`, `contentType`, `headers`, `body`, multipart |
| MCP stdio | Restored behaviorally | `StdioMcpClient`; smoke `mcpMarketplace.connectionTestSuccess = true`, `mcpToolExecution = "release mcp runtime"` |
| Python Art catalog | Restored behaviorally | `GET /v1/python-arts`; smoke `pythonArtCatalog.artId = "loom_echo"`, `pythonArtToolExecution` |
| Python source import helpers | Restored behaviorally | `/v1/python-arts/source/*`; smoke `pythonArtSourceImport.inferredInputs = 2`, `inferredOutputs = 2` |
| Workflow store and workflow-backed tools | Restored behaviorally | `/v1/workflows`, `/v1/tools`; smoke `workflowToolExecution`, `workflowArtNode.success = true`, `workflowAhrpProcess.status = "Success"` |

## Packaged resource matrix

| Old packaged resource class | Current Loom package evidence | Status |
| --- | --- | --- |
| `python/Launcher.py` | `Loom/resources/python/Launcher.py`; packaged under `release\Loom\...\python\Launcher.py` | Restored directly |
| `python/Arts/*` | `Loom/resources/python/Arts/Art_LoomEcho/art.json` and `main.py`; smoke discovers `loom_echo` | Restored directly with Loom-owned fixture Art |
| `bin/python-embed/*` | `Loom/resources/python-embed/python.exe`, `python312.zip`, `python312._pth`, DLLs; smoke uses package-local `bin\python-embed\python.exe` | Restored directly |
| OCR ONNX models | `Loom/resources/ocr/ch_PP-OCRv4_det_infer.onnx`, rec models, cls model | Restored directly |
| OCR runtime DLLs | `Loom/resources/ocr/onnxruntime.dll`, `onnxruntime_providers_shared.dll` | Restored directly |
| OCR fixture | `Loom/resources/ocr/fixtures/test_1.png`; smoke real OCR result length | Restored directly |
| Release metadata | `BUILD_INFO.txt` begins with `Loom Windows release artifact` after Phase 30 | Restored naming consistency |

## Product naming matrix

| Surface | Required Loom state | Evidence | Status |
| --- | --- | --- | --- |
| CLI executable | `loom.exe` | Phase 31 release package | Restored directly |
| Daemon executable | `loom-daemon.exe` | Phase 31 release package | Restored directly |
| Desktop executable | `loom-desktop.exe` | Phase 31 release package and smoke `desktopExe` | Restored directly |
| Desktop product/window title | `Loom` | `Loom/apps/desktop/src-tauri/tauri.conf.json`; desktop shell contract | Restored directly |
| Release BUILD_INFO heading | `Loom Windows release artifact` | Phase 30 `BUILD_INFO.txt` | Restored directly |
| Default managed configuration root | `%APPDATA%\Loom\configuration\apps`, `.runtime\loom\configuration\apps` | Phase 30 `loom_configuration` tests | Restored directly |
| Desktop/root Cargo authors | `Loom contributors` | Phase 30 code and desktop shell contract | Restored directly |
| Protocol compatibility names | `art_loom/*`, `art_hook/*`, `art/process`, `shared_memory`, `image_*` remain unchanged | Hook Bridge method list and smoke | Intentionally retained for compatibility |
| External gateway name | `Neuro Gateway` remains external dependency concept | `Loom/crates/loom_gateway` | Not a Loom product-prefix leak |

## User-visible desktop UX matrix

| Old user-visible flow | Current Loom state | Evidence | Status |
| --- | --- | --- | --- |
| Add Art button/modal | Registry page shows `AddArtWizard` and `Add Art` section | `Loom/apps/desktop/src/App.tsx`; `output\smoke\phase31-ui\loom-phase31-registry-add-art-ui.png` | Restored behaviorally in Phase 31 |
| CLI wrapper Art creation | `CLI wrapper Art` mode creates `execution.type = "cli_wrapper"` tool definitions | `createArtToolFromWizard`; UI contract | Restored behaviorally |
| Cloud API Art creation | `Cloud API Art` mode exposes endpoint, method, content type, headers, body | `createArtToolFromWizard`; UI contract | Restored behaviorally |
| Script / Python Art creation | `Script / Python Art` mode plus Python source import helpers | `AddArtWizard`; `Python source import`; Phase 28/31 evidence | Restored behaviorally |
| MCP-linked Art creation | `MCP-linked Art` mode selects configured server and tool name | `AddArtWizard`; UI contract | Restored behaviorally |
| Installed Python Art import | `Installed Python Art` mode and Installed Python Arts catalog | Phase 24/31 evidence | Restored behaviorally |
| Workflow-backed Art creation | `Workflow-backed Art` mode and Workflow Studio wrap action | Phase 23/31 evidence | Restored behaviorally |
| Native/image utility Art | `Native Image Art` mode exposes `image_path`, `image_base64`, and `image_buffer` language | Phase 14/18/31 evidence | Restored behaviorally |
| Workflow Art Node visual | Workflow Studio renders `art-node-card` with `Preview`, `Inputs`, `Outputs`, `Params`, `Result` | `output\smoke\phase31-ui\loom-phase31-workflow-art-node-ui.png`; UI contract | Restored behaviorally |
| Workflow Art node library/sidebar | Workflow Studio shows `Art node palette` and `Add Art node`, including empty-state visibility | Browser DOM check; UI contract | Restored behaviorally |
| Manual MCP server linking | MCP page shows `Manual MCP server`, `Save MCP server`, `Connect MCP server` | Browser snapshot and UI contract | Restored behaviorally |
| Desktop Hook sync | Hook Bridge page shows `Sync desktop Hook`, `Broadcast hook sync`, and legacy sync method chips | `output\smoke\phase31-ui\loom-phase31-hook-ui.png`; UI contract | Restored behaviorally |
| ArtLoom-like workbench visual hierarchy | Modern-gradient industrial terminal shell with left rail, main board, Art cards, node cards, contrast fixes | `Loom/apps/desktop/src/styles.css`; browser screenshots | Restored with Loom visual system |

## Remaining non-critical old surfaces

| Old surface | Classification | Reason |
| --- | --- | --- |
| Exact old Tauri command symbols | Intentionally replaced | Loom is daemon-first; desktop Tauri is a thin shell/proxy. |
| Exact AntD UI clone | Intentionally replaced | Loom uses its own desktop workbench UI while preserving behavior. |
| ReactFlow dependency/implementation | Intentionally replaced | Visual graph behavior is restored with lightweight Loom-native graph UI. |
| `install_mcp_package` direct `pip install` | Intentionally replaced | Marketplace saves stdio configs; package resolution is delegated to `npx`/`uvx`/`docker` on use. |
| `check_mcp_package_installed` direct Python import check | Intentionally replaced | Real connection test via `POST /v1/mcp/test` is stronger product evidence. |
| Exact OS autostart/tray registration side effects | Intentionally replaced | Loom preserves compatibility preferences and command language without changing host OS startup or tray registration. |

## Product-critical gap register

| Gap | Status |
| --- | --- |
| MCP marketplace/config/test lost in Loom | Closed in Phase 26 |
| Workflow graph UI lost in Loom | Closed in Phase 27 |
| Python source import helpers lost in Loom | Closed in Phase 28 |
| Hook Bridge settings/shortcuts methods advertised but unimplemented | Closed in Phase 29 |
| Remaining visible Loom release/config `Neuro` prefix | Closed in Phase 30 |
| `get_user_arts` returned internal Loom compat-tool shape instead of old ArtLoom frontend card shape | Closed on 2026-06-18 |
| Product-critical gaps after final matrix | None found |

## Verification commands most relevant to this final matrix

Already passed during Phase 30:

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom_configuration default_root --offline -- --nocapture
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-build-release-exes-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
cargo check --manifest-path Loom\Cargo.toml -p loom-daemon --offline
npm --prefix Loom\apps\desktop run typecheck
cargo fmt --manifest-path Loom\Cargo.toml --all -- --check
git diff --check
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-naming-cleanup-phase30 -Force
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-naming-cleanup-phase30 -Apps Loom
```

The broad source-level ArtLoom parity contract remains available:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

This contract asserts the current release builder, daemon, desktop UI/services,
Hook Bridge methods, MCP marketplace, Python Art/source helpers, shared image
runtime, OCR, cloud multipart, and workflow surfaces that are central to this
matrix.
