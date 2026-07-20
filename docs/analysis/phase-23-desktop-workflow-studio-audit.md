# Phase 23 Desktop Workflow Studio Audit

## Scope

Phase 23 restores a desktop UI parity layer for workflow authoring and
workflow-backed tool generation. The target is a Loom-native desktop Workflow
Studio over the current daemon APIs, not a full copy of the old React/AntD
graph editor.

Restored desktop capabilities:

- editable workflow YAML authoring surface
- Smart Import language for cURL/request and response-template parsing
- workflow interface inference from current registry definitions
- wrapping a saved workflow as a Loom tool with `execution.type = "workflow"`
  and `workflowBindings`
- Tauri bridge support for daemon `GET` and `PUT` JSON requests

The visible Loom product names remain unchanged:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old desktop UI and utility sources:

- `Z:\project\project\ArtNexus\ArtLoom\src\App.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\utils\cliTemplateParser.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src\utils\workflowArtInterface.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-manager\index.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\index.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\components\Canvas.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\components\Sidebar.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\features\workflow-editor\components\PropertiesPanel.tsx`
- `Z:\project\project\ArtNexus\ArtLoom\src\services\WorkflowOrchestrator.ts`
- `Z:\project\project\ArtNexus\ArtLoom\src\utils\yamlHelper.ts`

Old desktop routes included:

```tsx
<Route index element={<Workflows />} />
<Route path="workflows" element={<Workflows />} />
<Route path="editor/:id" element={<WorkflowEditor />} />
```

Old `AddArtModal.tsx` provided multiple user-facing import and wrapping
features:

- execution types including `cli_wrapper`, `cloud_api`, `script`, `mcp`, and
  `workflow`
- Smart Import for cURL/request templates and response samples
- `parseCurlCommand`
- `autoTemplateResponse`
- MCP schema import
- workflow selection and workflow-backed Art/tool creation
- `inferWorkflowArtInterface`
- `attachWorkflowBindingMetadata`
- generated `workflowBindings`

Old `workflowArtInterface.ts` behavior inferred:

- unconnected image inputs as workflow image inputs
- unset value inputs as workflow value inputs
- unset node params as workflow params
- terminal node output as the primary workflow result

The binding compatibility kinds remain:

```text
input_image
input_value
param
node_result
```

Old `cliTemplateParser.ts` behavior included:

- cURL and raw CLI token parsing
- request placeholders like `{{inputs.prompt.value}}`
- file/path placeholders like `{{inputs.input.path}}`
- response placeholders like `{{outputs.result.value}}`
- response output port inference

## Loom state before Phase 23

Before Phase 23, Loom desktop was a minimal Tauri/React shell under:

```text
Loom/apps/desktop/src
```

It had panels for overview, MCP, registry, workflow manager, Hook bridge,
workflows, agents, runs, settings, and about. The `WorkflowsPanel` was static
copy:

```text
Import or author Loom workflow definitions
Dispatch through `loom-daemon` capability/run APIs
Inspect events, evidence, and follow-up actions
```

It did not provide:

- editable workflow YAML
- Smart Import cURL parsing
- response-template output inference
- workflow interface inference
- a button to generate workflow-backed tool definitions
- a Tauri `PUT` bridge for daemon writes

The daemon already provided the required write endpoints:

```text
GET  /v1/workflows
PUT  /v1/workflows/{workflowId}
GET  /v1/tools
PUT  /v1/tools/{toolId}
```

No separate workflow data or metadata subroutes exist in the current daemon, so
Phase 23 intentionally writes workflow bundles through the existing
`PUT /v1/workflows/{workflowId}` contract.

## Phase 23 implementation

### Workflow Studio utility layer

Added:

```text
Loom/apps/desktop/src/services/workflowStudio.ts
```

It restores Loom-native equivalents of the old utility behavior:

- `parseCurlCommand`
- `parseTemplate`
- `autoTemplateResponse`
- `parseWorkflowYamlLite`
- `inferWorkflowArtInterface`

The implementation accepts current `LoomToolDefinition[]` and uses
`unknown`-first type guards to read optional old-style fields from `inputs`,
`params`, and `outputs`. This keeps the desktop layer compatible with existing
registry JSON while avoiding `any`.

`parseWorkflowYamlLite` is intentionally lightweight. It extracts the
workflow-level `name`, `description`, and node fields `id`, `uses`, `needs`,
and `with`. It does not replace the Rust workflow parser or old visual graph
editor.

### Desktop API writes

Updated:

```text
Loom/apps/desktop/src/services/loomApi.ts
```

New API helpers:

- `getJsonViaTauri`
- `putJsonViaTauri`
- `saveWorkflowBundle`
- `saveToolDefinition`

`saveWorkflowBundle` writes:

```text
PUT /v1/workflows/{workflowId}
```

with:

```json
{"data":"<workflow yaml>"}
```

`saveToolDefinition` writes:

```text
PUT /v1/tools/{toolId}
```

with a full `LoomToolDefinition`.

### Tauri bridge

Updated:

```text
Loom/apps/desktop/src-tauri/src/lib.rs
```

New commands:

- `get_loom_daemon_json`
- `put_loom_daemon_json`

The existing `post_loom_daemon_json` path now shares a generic local HTTP JSON
request helper. The bridge keeps the same loopback guard:

```text
127.0.0.1
localhost
[::1]
```

and still rejects non-local daemon URLs.

### React Workflow Studio panel

Updated:

```text
Loom/apps/desktop/src/App.tsx
Loom/apps/desktop/src/styles.css
```

The new `WorkflowStudioPanel` provides:

- workflow id input
- generated tool display name input
- editable workflow YAML textarea
- `Save workflow YAML`
- Smart Import cURL textarea
- response sample textarea
- `Smart Import` preview
- `Infer workflow interface`
- workflow binding JSON preview
- `Wrap workflow as Loom tool`
- saved daemon workflow list from the current snapshot

Generated workflow-backed tools use:

```json
{
  "execution": {
    "type": "workflow",
    "workflowId": "<workflow id>",
    "workflowBindings": {
      "inputs": [],
      "primaryOutput": {
        "nodeId": "<terminal node>",
        "output": "result",
        "kind": "node_result"
      }
    }
  }
}
```

When registry definitions include `inputs`, `params`, or `outputs`, the
inference layer emits the corresponding input bindings and output port
metadata.

### Parity contract

Updated:

```text
scripts/tests/test-loom-artloom-parity-contract.ps1
```

New source-level contract assertions require:

- `WorkflowStudioPanel`
- `Smart Import`
- `Infer workflow interface`
- `Wrap workflow as Loom tool`
- `saveToolDefinition`
- `parseCurlCommand`
- `autoTemplateResponse`
- `parseWorkflowYamlLite`
- `inferWorkflowArtInterface`
- `putJsonViaTauri`
- `put_loom_daemon_json`

The initial RED failure was:

```text
Loom desktop must expose an editable workflow studio surface. Missing=[WorkflowStudioPanel]
```

## Validation evidence

Targeted validation passed after implementation:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
cargo test --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline -- --nocapture
cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline
cargo fmt --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml -- --check
```

Key results:

```text
Loom ArtLoom parity release contract passed.
tsc --noEmit passed.
rsbuild build passed.
Tauri Rust tests: 4 passed.
cargo check finished successfully.
cargo fmt check passed.
```

Browser UI smoke was also run against the local Rsbuild dev server:

```text
http://127.0.0.1:1423/
```

Observed visible UI:

- navigation item `Workflow Studio`
- page heading `Workflow Studio`
- `Save workflow YAML`
- `Infer workflow interface`
- `Wrap workflow as Loom tool`
- Smart Import cURL/response inputs

Observed interactions:

- `Smart Import` produced request/template/output-port preview
- `Infer workflow interface` produced output/binding/warning preview

The browser console had expected daemon fetch errors because no development
daemon was running during this UI-only smoke.

## Release evidence

Release generation:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-desktop-workflow-studio-phase23 -Force
```

Generated:

```text
release\Loom\loom-desktop-workflow-studio-phase23
packages\Loom-loom-desktop-workflow-studio-phase23-windows-x64.zip
```

Executable names:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Zip size:

```text
49997281 bytes
```

Formal release verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-desktop-workflow-studio-phase23 -Apps Loom
```

reported:

```text
status = passed
gitHead = 1103a98b852f7e92ffbb8e1655da8614c05f43d9
gitDirty = false
checksumEntries = 29
```

Release smoke:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-desktop-workflow-studio-phase23 -Apps Loom
```

passed and wrote:

```text
output\smoke\runs\20260612-190801-Loom-90608-51f7f6a3dcfc48118ee9600d155571e2\release-local-apps-loom-desktop-workflow-studio-phase23-Loom-summary.json
output\smoke\latest\release-local-apps-loom-desktop-workflow-studio-phase23-Loom-summary.json
```

The smoke summary proved the generated release still contains:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

and still passes previously restored runtime parity surfaces, including:

- MCP-backed tool execution
- script-backed tool execution
- packaged embedded Python execution
- cloud API execution
- workflow-backed direct tool execution
- Hook WebSocket handshake
- Hook broadcast fanout
- `art_loom/execute_art_node`
- AHRP `art/process`
- native image filter
- shared image I/O
- image helper conversion
- `art_loom/ocr_image`
- real packaged OCR
- cloud multipart/template execution
- workflow Art node execution
- workflow AHRP execution

Important smoke evidence:

```text
desktopExe = "...release\Loom\loom-desktop-workflow-studio-phase23\loom-desktop.exe"
pythonToolExecution.packagedPython = true
workflowToolExecution = "script saw release workflow runtime"
workflowArtNode.type = "success"
workflowAhrpProcess.status = "Success"
cloudMultipartArtNode.multipartSeen = true
realOcrImage.fullTextLength = 63
```

## Non-goals and remaining gaps

Phase 23 is not the final Loom migration completion point.

Not restored in this phase:

- full old visual node canvas
- React Flow / AntD route-level editor clone
- richer Python Art plugin management UI
- final full-source audit against old ArtLoom

The next recommended layer is either:

1. decide whether richer Python Art plugin management UI is still required; or
2. run the final full-source audit against old ArtLoom and turn any remaining
   omissions into small follow-up phases.
