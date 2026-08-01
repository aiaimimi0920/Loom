# Loom Pluginized Art Frameworks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Loom's current installable-Art proof line into a true plugin boundary where Art frameworks and Art nodes are independently packaged, installed, discovered, enabled, disabled, upgraded, and uninstalled without modifying Loom or Hook source.

**Architecture:** Loom remains the host and owns package installation, registry, permissions, execution brokering, cache, and Hook-facing protocol. Each optional Art framework is shipped as an external framework package with a process entrypoint and manifest; each Art node is shipped as an external Art package that declares one required framework and its ports, parameters, resources, permissions, and result schema. Hook consumes only dynamic capabilities and generic Art node result contracts; any hardcoded Art ID or framework-specific Hook branch is treated as debt unless it is a generic capability renderer.

**Tech Stack:** Rust, serde/serde_json, zip, PowerShell 5+, Loom daemon HTTP APIs, Hook Bridge WebSocket, existing `loom-art-store`, Tauri desktop API tests, separate framework package build scripts, Windows x64 release packaging.

---

## Current baseline

Baseline tags created before this plan:

| Repository | Tag | Commit |
| --- | --- | --- |
| Loom | `框架修改前的最后一个版本` | `a8e3df0712bcaa4ba640d8848cf82d2271582054` |
| Hook | `框架修改前的最后一个版本` | `a86272a5b06e3b3f5a92d01dda6be138ab6e087f` |

The formal framework IDs in current Loom source are:

| Framework ID | Current proof Art | Current problem to fix |
| --- | --- | --- |
| `cli_wrapper` | `图片压缩` / `custom-1770146354922` | Marked built-in/default-installed in `crates/loom_tool_registry/src/framework.rs`. |
| `cloud_api` | Remove BG / cloud API fixture | Marked built-in/default-installed and executed by core registry code. |
| `script` | `图片融合` / `custom-image-blend-script` | Marked built-in/default-installed and executed by core registry code. |
| `python_art` | `Color Transfer (RBF)` / `custom-1770131241684` | Has installable runtime, but execution is still core-owned. Hook-facing shader behavior is compatibility metadata, not a separate current framework ID. |
| `mcp` | `图片搜索` / `custom-image-search` | Installed on demand, but the MCP client/execution adapter is still core-owned. |
| `workflow` | `图片融合并压缩` / `custom-image-blend-compress-workflow` | Marked built-in/default-installed and child execution still assumes core-owned framework adapters. |

Key current source anchors:

- `crates/loom_tool_registry/src/framework.rs`
  - `FRAMEWORK_IDS`
  - `BUILT_IN_FRAMEWORKS`
  - `FrameworkRegistry`
  - `framework_ready_in`
- `crates/loom_tool_registry/src/install.rs`
  - Art ZIP installation and framework readiness gate.
- `crates/loom_tool_registry/src/lib.rs`
  - `ToolExecution` and current framework-specific execution code.
- `apps/daemon/src/lib.rs`
  - `/v1/frameworks`, `/v1/arts/install`, `/v1/arts/store/*`, `/v1/tools/{id}/readiness`.
- `apps/art-store/src/lib.rs`
  - current Art package catalog and framework ZIP serving.
- `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1`
  - current all-framework fake-store smoke that must evolve into the plugin-boundary smoke.
- `..\Hook\src\components\UnitParamsPanel.tsx`
  - generic result/candidate renderer with image-search wording that must be audited for Art-specific leakage.
- `..\Hook\src\hooks\useNodeParameters.ts`
  - shader/cache handling must become capability-driven rather than specific-Art-driven.

## Non-negotiable acceptance criteria

- [ ] A default Loom release contains no optional Art framework runtime package.
- [ ] A fresh control-plane root starts with zero installed optional frameworks.
- [ ] Installing an Art without its framework returns a named `framework_not_ready` error.
- [ ] A framework can be installed from a package ZIP, listed, readiness-probed, disabled, re-enabled, upgraded, and uninstalled.
- [ ] An Art can be installed from a package ZIP, listed through Loom, instantiated by Hook, executed once, disabled, re-enabled, upgraded, and uninstalled.
- [ ] Installing a new third-party Art package does not require modifying Loom source.
- [ ] Installing a new third-party Art package does not require modifying Hook source.
- [ ] Installing a new third-party framework package does not require modifying Loom source outside stable host APIs.
- [ ] Hook renders ports, parameters, candidate images, image previews, error details, and exports through generic capability/result contracts.
- [ ] No production Hook code branches on the six sample Art IDs.
- [ ] No production Loom host code branches on the six sample Art IDs except test fixtures, compatibility smoke fixtures, or migration docs.
- [ ] The six existing proof Arts are available only through packages after this phase.
- [ ] A final packaged release under `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\<version-id>` proves default-empty, install, execute, restart, disable, uninstall, and no-source-change flows.

## File map

Planned Loom files:

- Modify `crates/loom_tool_registry/src/framework.rs`
  - Replace default-built-in framework state with package-backed installed state.
  - Add package manifest parsing, enabled/disabled state, version, protocol, platform, and runtime entry metadata.
- Modify `crates/loom_tool_registry/src/install.rs`
  - Validate Art packages against installed framework package manifests.
  - Record Art package metadata needed for upgrade, disable, uninstall, and restart discovery.
- Modify `crates/loom_tool_registry/src/lib.rs`
  - Add a generic framework execution adapter that calls installed framework process runtimes.
  - Remove sample-Art-specific normalization from the core path or move it behind generic result normalizers.
- Modify `apps/daemon/src/lib.rs`
  - Add framework package upload/install, disable/enable, upgrade, uninstall, and Art disable/enable/uninstall endpoints.
  - Expose dynamic capability metadata to Hook.
- Modify `apps/art-store/src/lib.rs`
  - Serve framework package manifests and versions, not only raw `<id>.zip` downloads.
- Modify `apps/desktop/src` files after API shape is stable
  - Keep desktop UI generic and data-driven for framework/Art install state.
- Modify `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1`
  - Convert the smoke from "fixtures installed through mostly built-in adapters" to "fresh host + package install + package execution".
- Create `scripts/tests/Test-ArtPluginBoundaryContract.ps1`
  - Static and source-contract guard for default-empty frameworks and no sample-Art Hook hardcoding.
- Create `scripts/tests/Test-ArtFrameworkPackageContract.ps1`
  - Framework package manifest and ZIP shape guard.
- Create `scripts/Build-LoomArtFrameworkPackages.ps1`
  - Independently builds the six sample framework packages outside the default Loom release payload.
- Create `scripts/Build-LoomSampleArtPackages.ps1`
  - Independently builds the six sample Art packages.
- Create `framework-packages/<framework-id>/framework.manifest.json`
  - Source manifests for repo-owned sample frameworks.
- Create `framework-packages/<framework-id>/runtime/...`
  - Process runtime entrypoints or wrappers for the repo-owned sample frameworks.
- Move sample Art sources under `art-packages/samples/<art-id>/`
  - Keep them buildable as packages, not as default Loom runtime resources.

Planned Hook files:

- Modify `..\Hook\src\types\...`
  - Add or align generic Art capability/result types only if current types are insufficient.
- Modify `..\Hook\src\components\UnitParamsPanel.tsx`
  - Replace image-search-specific candidate wording with generic image candidate thumbnail rendering.
- Modify `..\Hook\src\hooks\useNodeParameters.ts`
  - Keep shader behavior behind generic capability metadata.
- Modify `..\Hook\src\store\graphStore.ts` and related tests only if dynamic capability persistence needs schema extension.

## Task 1: Add source-contract guards before runtime changes

**Files:**
- Create: `scripts/tests/Test-ArtPluginBoundaryContract.ps1`
- Modify: `docs/progress/phase-67-pluginized-art-frameworks.md`

- [ ] **Step 1: Write the boundary contract test**

Create `scripts/tests/Test-ArtPluginBoundaryContract.ps1` with assertions that intentionally fail on the current code:

```powershell
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)

$frameworkRs = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "crates\loom_tool_registry\src\framework.rs")
Assert-True (-not $frameworkRs.Contains("BUILT_IN_FRAMEWORKS")) "Optional Art frameworks must not be installed by default."
Assert-True ($frameworkRs.Contains("framework.manifest.json")) "Framework package manifest support must be present."

$readme = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $repoRoot "README.md")
Assert-True (-not $readme.Contains("installed by default")) "README must describe explicit framework installation only."

$hookRoot = Resolve-Path (Join-Path $repoRoot "..\Hook")
$hookSource = Get-ChildItem -LiteralPath (Join-Path $hookRoot "src") -Recurse -File -Include *.ts,*.tsx |
    Where-Object { $_.FullName -notmatch "\\node_modules\\" }

$forbiddenArtIds = @(
    "custom-1770146354922",
    "custom-image-search",
    "custom-1770131241684",
    "custom-image-blend-script",
    "custom-image-blend-compress-workflow"
)

foreach ($file in $hookSource) {
    $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $file.FullName
    foreach ($id in $forbiddenArtIds) {
        Assert-True (-not $text.Contains($id)) "Hook production source must not branch on sample Art id '$id' in $($file.FullName)."
    }
}

Write-Host "Art plugin boundary contract passed."
```

- [ ] **Step 2: Run the contract and record RED**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtPluginBoundaryContract.ps1
```

Expected before implementation: failure on `BUILT_IN_FRAMEWORKS` and any remaining Hook sample-Art hardcoding.

- [ ] **Step 3: Record the RED evidence**

Append the exact failing message and command exit code to `docs/progress/phase-67-pluginized-art-frameworks.md`.

- [ ] **Step 4: Commit the RED contract**

```powershell
git add -- scripts/tests/Test-ArtPluginBoundaryContract.ps1 docs/progress/phase-67-pluginized-art-frameworks.md
git commit -m "test(loom): guard art plugin boundary"
```

## Task 2: Define framework package manifests and explicit installed state

**Files:**
- Modify: `crates/loom_tool_registry/src/framework.rs`
- Create: `scripts/tests/Test-ArtFrameworkPackageContract.ps1`
- Create: `framework-packages/cli_wrapper/framework.manifest.json`
- Create: `framework-packages/cloud_api/framework.manifest.json`
- Create: `framework-packages/script/framework.manifest.json`
- Create: `framework-packages/python_art/framework.manifest.json`
- Create: `framework-packages/mcp/framework.manifest.json`
- Create: `framework-packages/workflow/framework.manifest.json`

- [ ] **Step 1: Add framework manifest source-contract test**

Create `scripts/tests/Test-ArtFrameworkPackageContract.ps1` that loads every `framework-packages/*/framework.manifest.json` and asserts:

```text
id equals directory name
version is non-empty
protocolVersion is "loom.framework.v1"
platforms contains "windows-x64"
entry.kind is "process"
entry.command is non-empty
permissions is an array
```

The test must also assert there are exactly six repo-owned sample framework manifests.

- [ ] **Step 2: Add six framework manifests**

Use this shape for each manifest, replacing `id`, `name`, `description`, and command:

```json
{
  "id": "script",
  "name": "Script Framework",
  "description": "Executes script-backed Art packages in an isolated process boundary.",
  "version": "0.1.0",
  "protocolVersion": "loom.framework.v1",
  "platforms": ["windows-x64"],
  "entry": {
    "kind": "process",
    "command": "runtime/loom-framework-script.exe",
    "args": ["--stdio"]
  },
  "permissions": ["process.spawn", "file.read", "file.write"],
  "artExecution": {
    "requestSchema": "loom.art.execute.v1",
    "responseSchema": "loom.art.result.v1"
  }
}
```

For scriptless wrappers that are not yet implemented, use the final executable names that Task 4 will build:

| Framework | Command |
| --- | --- |
| `cli_wrapper` | `runtime/loom-framework-cli-wrapper.exe` |
| `cloud_api` | `runtime/loom-framework-cloud-api.exe` |
| `script` | `runtime/loom-framework-script.exe` |
| `python_art` | `runtime/loom-framework-python-art.exe` |
| `mcp` | `runtime/loom-framework-mcp.exe` |
| `workflow` | `runtime/loom-framework-workflow.exe` |

- [ ] **Step 3: Add Rust manifest types**

In `crates/loom_tool_registry/src/framework.rs`, add serializable types:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkPackageManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub protocol_version: String,
    pub platforms: Vec<String>,
    pub entry: FrameworkRuntimeEntry,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub art_execution: FrameworkArtExecutionContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkRuntimeEntry {
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkArtExecutionContract {
    pub request_schema: String,
    pub response_schema: String,
}
```

- [ ] **Step 4: Replace default installed frameworks with empty state**

Remove `BUILT_IN_FRAMEWORKS`. Change `default_installed()` so a missing `frameworks.json` returns an empty `BTreeSet<String>`.

Expected behavior:

```rust
let root = temp_root();
let registry = FrameworkRegistry::new(&root);
assert!(registry.installed_ids().is_empty());
for id in FRAMEWORK_IDS {
    assert!(!registry.is_installed(id));
}
```

- [ ] **Step 5: Add package-backed status**

Extend `FrameworkStatus` with fields:

```rust
pub enabled: bool,
pub version: Option<String>,
pub runtime_dir: Option<PathBuf>,
```

Status semantics:

```text
installed=false, enabled=false, ready=false when no package is present
installed=true, enabled=false, ready=false when disabled
installed=true, enabled=true, ready=true when manifest entry command exists
```

- [ ] **Step 6: Run tests**

```powershell
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry framework -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1
```

Expected after implementation: all framework tests pass and the PowerShell contract passes.

- [ ] **Step 7: Commit**

```powershell
git add -- `
  crates/loom_tool_registry/src/framework.rs `
  scripts/tests/Test-ArtFrameworkPackageContract.ps1 `
  framework-packages
git commit -m "feat(loom): define package-backed art frameworks"
```

## Task 3: Implement framework package install, disable, upgrade, and uninstall

**Files:**
- Modify: `crates/loom_tool_registry/src/framework.rs`
- Modify: `apps/daemon/src/lib.rs`
- Modify: `apps/art-store/src/lib.rs`
- Modify: `apps/desktop/src/api/loomApi.ts` if endpoint helpers are needed.

- [ ] **Step 1: Add framework ZIP installer tests**

Add Rust tests in `framework.rs` that build a ZIP with:

```text
framework.manifest.json
runtime/loom-framework-script.exe
```

and assert:

```text
install_framework_package_from_zip stores package under <root>/frameworks/script/
status reports installed=true, enabled=true, ready=true
disable sets enabled=false and ready=false
enable restores ready=true
upgrade replaces version and runtime bytes
uninstall removes package directory and installed state
unsafe ZIP paths are rejected
unknown manifest IDs are rejected
```

- [ ] **Step 2: Add daemon routes**

Add or extend these routes:

```http
POST /v1/frameworks/install
POST /v1/frameworks/{frameworkId}/enable
POST /v1/frameworks/{frameworkId}/disable
POST /v1/frameworks/{frameworkId}/upgrade
POST /v1/frameworks/{frameworkId}/uninstall
```

`POST /v1/frameworks/install` accepts:

```json
{
  "zipBase64": "data:application/zip;base64,..."
}
```

Responses include:

```json
{
  "framework": {
    "id": "script",
    "installed": true,
    "enabled": true,
    "ready": true,
    "version": "0.1.0"
  }
}
```

- [ ] **Step 3: Keep old route compatibility only as package install**

Keep `POST /v1/frameworks/{id}/install`, but change it to fetch `<store>/frameworks/{id}.zip` and call the same package installer. It must not mark a framework installed without a package.

- [ ] **Step 4: Run daemon tests**

```powershell
cargo test --manifest-path .\Cargo.toml -p loom-daemon framework -- --nocapture
```

Expected: daemon route tests prove install, disable, enable, upgrade, uninstall, and missing package errors.

- [ ] **Step 5: Commit**

```powershell
git add -- crates/loom_tool_registry/src/framework.rs apps/daemon/src/lib.rs apps/art-store/src/lib.rs apps/desktop/src/api/loomApi.ts
git commit -m "feat(loom): install framework packages at runtime"
```

## Task 4: Add the generic external framework execution protocol

**Files:**
- Modify: `crates/loom_tool_registry/src/lib.rs`
- Modify: `crates/loom_tool_registry/src/install.rs`
- Create: `crates/loom_tool_registry/src/framework_process.rs`

- [ ] **Step 1: Add protocol request and response types**

Create `framework_process.rs` with:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteRequest {
    pub protocol_version: String,
    pub framework_id: String,
    pub art_id: String,
    pub art_dir: PathBuf,
    pub inputs: serde_json::Value,
    pub params: serde_json::Value,
    pub disabled_params: Vec<String>,
    pub context: FrameworkExecutionContext,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionContext {
    pub request_id: String,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteResponse {
    pub status: String,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub error: Option<FrameworkExecuteError>,
    #[serde(default)]
    pub candidates: Vec<serde_json::Value>,
    #[serde(default)]
    pub cache: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}
```

- [ ] **Step 2: Add process invocation**

Add a helper that:

```text
resolves the framework manifest from FrameworkRegistry
spawns manifest.entry.command with manifest.entry.args
writes one JSON request to stdin
reads one JSON response from stdout
applies a 120-second default timeout
returns structured ToolRegistryError on spawn, timeout, invalid JSON, or error status
```

- [ ] **Step 3: Add a generic execution variant or metadata bridge**

Prefer a minimal host variant:

```rust
ToolExecution::FrameworkArt {
    framework: String,
}
```

The Art package keeps framework-specific resource details in its installed `art_dir` and manifest metadata. This avoids adding new `ToolExecution` variants for third-party framework types.

- [ ] **Step 4: Route Art execution through the installed framework**

For pluginized packages, `execute_tool` must call the external framework process. Existing `ToolExecution` variants may remain only until the six sample frameworks are converted and the old variants are no longer used by production sample packages.

- [ ] **Step 5: Add tests with a fake framework process**

Create a tiny test executable or test script fixture that echoes a valid image/text response. Assert:

```text
request JSON contains art_id, art_dir, params, and input
successful stdout response becomes normal ToolExecutionResult
error stdout response preserves code/message/detail
timeout returns a structured timeout error
invalid stdout returns a structured protocol error
```

- [ ] **Step 6: Run tests and commit**

```powershell
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry framework_process -- --nocapture
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry -- --nocapture
git add -- crates/loom_tool_registry/src/lib.rs crates/loom_tool_registry/src/install.rs crates/loom_tool_registry/src/framework_process.rs
git commit -m "feat(loom): execute arts through framework processes"
```

## Task 5: Convert the six sample frameworks into independent packages

**Files:**
- Create: `framework-packages/*/runtime/*`
- Create: `scripts/Build-LoomArtFrameworkPackages.ps1`
- Modify: `scripts/tests/Test-ArtFrameworkPackageContract.ps1`

- [ ] **Step 1: Add independent package build script**

Create `scripts/Build-LoomArtFrameworkPackages.ps1` with parameters:

```powershell
param(
    [string]$OutputRoot = ".loom-art-store-data\frameworks",
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)
```

The script must:

```text
build each framework runtime independently
stage framework.manifest.json
stage runtime files
write <OutputRoot>/<frameworkId>.zip
write <OutputRoot>/<frameworkId>.zip.sha256
emit a JSON summary with ids, paths, bytes, and hashes
```

- [ ] **Step 2: Keep framework runtime builds outside default Loom package**

If framework runtimes are Rust crates, place them outside the root workspace members or build them only with explicit `--manifest-path framework-packages/<id>/runtime/Cargo.toml`.

The root `Cargo.toml` default workspace build must not list framework runtime packages as members.

- [ ] **Step 3: Implement the six repo-owned framework runtimes**

Each runtime supports stdin/stdout `loom.framework.v1`:

| Framework | Runtime responsibility |
| --- | --- |
| `cli_wrapper` | Invoke an Art-bundled command/template safely and return image/text output. |
| `cloud_api` | Execute the Art's HTTP template/multipart request and normalize image/text output. |
| `script` | Invoke the Art-bundled PowerShell/script entry and normalize image/text output. |
| `python_art` | Invoke the installed Python Art launcher/runtime from the framework package. |
| `mcp` | Invoke configured MCP servers and normalize candidate/image/text results. |
| `workflow` | Execute child Art calls through the Loom framework broker without embedding child-specific code. |

- [ ] **Step 4: Run package build contract**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Build-LoomArtFrameworkPackages.ps1 `
  -OutputRoot .\.loom-art-store-data\frameworks

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtFrameworkPackageContract.ps1
```

Expected: six ZIPs and six hashes exist, and every manifest entry command points to a staged runtime file.

- [ ] **Step 5: Commit**

```powershell
git add -- framework-packages scripts/Build-LoomArtFrameworkPackages.ps1 scripts/tests/Test-ArtFrameworkPackageContract.ps1
git commit -m "feat(loom): package art framework runtimes"
```

## Task 6: Convert the six sample Arts into external Art packages

**Files:**
- Create: `art-packages/samples/*`
- Create: `scripts/Build-LoomSampleArtPackages.ps1`
- Modify: existing `scripts/Install-Loom*Art.ps1` scripts or replace them with package builders.
- Remove default runtime ownership from `resources/script-arts`, `resources/workflow-arts`, and framework-specific resource staging after replacements are in place.

- [ ] **Step 1: Add sample Art package source directories**

Create one directory per sample Art:

```text
art-packages/samples/image-compress/
art-packages/samples/remove-bg/
art-packages/samples/image-search/
art-packages/samples/color-transfer/
art-packages/samples/image-blend/
art-packages/samples/image-blend-compress/
```

Each directory contains:

```text
manifest.json
resources/...
```

Each manifest declares:

```json
{
  "id": "custom-image-blend-script",
  "name": "图片融合",
  "enabled": true,
  "execution": {
    "type": "framework_art",
    "framework": "script"
  },
  "metadata": {
    "dependencies": {
      "framework": "script"
    },
    "capabilities": {
      "preview": "image",
      "exports": ["image/png"],
      "parameters": "dynamic"
    }
  }
}
```

- [ ] **Step 2: Add sample Art package build script**

Create `scripts/Build-LoomSampleArtPackages.ps1` that:

```text
builds six Art ZIPs under .loom-art-store-data/arts
verifies every package has manifest.json
verifies every package declares metadata.dependencies.framework
verifies no package writes into %APPDATA% during build
emits summary JSON with id, framework, zip path, bytes, sha256
```

- [ ] **Step 3: Update installers**

Either update each existing installer to call the package build script or replace per-Art logic with a thin wrapper over:

```powershell
Build-LoomSampleArtPackages.ps1
POST /v1/frameworks/{frameworkId}/install
POST /v1/arts/store/install
```

The installers must not modify Loom source, Hook source, or default release resources.

- [ ] **Step 4: Run package and install tests**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Build-LoomSampleArtPackages.ps1 `
  -OutputRoot .\.loom-art-store-data\arts

cargo test --manifest-path .\Cargo.toml -p loom_tool_registry install -- --nocapture
cargo test --manifest-path .\Cargo.toml -p loom-daemon art_store -- --nocapture
```

- [ ] **Step 5: Commit**

```powershell
git add -- art-packages scripts/Build-LoomSampleArtPackages.ps1 scripts/Install-Loom*Art.ps1
git commit -m "feat(loom): package sample arts outside the host"
```

## Task 7: Make Hook fully capability-driven for plugin Arts

**Files:**
- Modify: `..\Hook\src\components\UnitParamsPanel.tsx`
- Modify: `..\Hook\src\hooks\useNodeParameters.ts`
- Modify: `..\Hook\src\types\unit.ts`
- Add/modify Hook tests under `..\Hook\__tests__`

- [ ] **Step 1: Add Hook static guard**

Add a Hook-side test that fails if production `src/**/*.ts*` contains any of the sample Art IDs listed in Task 1, except fixture/test files.

- [ ] **Step 2: Replace Art-specific UI labels**

Change candidate rendering from image-search-specific wording to generic:

```text
候选 1
候选 2
候选 3
```

with thumbnails coming from generic result candidate metadata:

```json
{
  "kind": "image.candidates",
  "selectedIndex": 0,
  "items": [
    { "index": 0, "thumbnail": "data:image/png;base64,...", "preview": "data:image/png;base64,..." }
  ]
}
```

- [ ] **Step 3: Replace shader-specific behavior with capability metadata**

The UI may still render shader/live preview behavior, but it must be activated by capability fields such as:

```json
{
  "capabilities": {
    "preview": "shader",
    "requiresLiveInputs": true,
    "parameterEditor": "generic"
  }
}
```

not by checking a concrete Art ID.

- [ ] **Step 4: Run Hook verification**

```powershell
Push-Location ..\Hook
npm test
npm run typecheck
cargo test --manifest-path .\src-tauri\Cargo.toml
Pop-Location
```

- [ ] **Step 5: Commit Hook changes**

```powershell
Push-Location ..\Hook
git add -- src __tests__
git commit -m "feat(hook): render plugin arts by capability"
Pop-Location
```

## Task 8: Add end-to-end plugin boundary smoke

**Files:**
- Modify: `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1`
- Create: `scripts/Invoke-LoomPluginBoundarySmoke.ps1`
- Modify: `scripts/verify-release.ps1`

- [ ] **Step 1: Add fresh-host default-empty assertions**

The smoke must start with a new control-plane root and assert:

```text
GET /v1/frameworks returns six known framework IDs
all installed=false
all enabled=false
all ready=false
GET /v1/tools returns no six sample Arts before package install
```

- [ ] **Step 2: Install packages through the store**

The smoke must:

```text
start temporary loom-art-store
serve six framework ZIPs and six Art ZIPs
install each framework package
install each matching Art package
restart the daemon
assert all installed frameworks and Arts are rediscovered
instantiate six Hook nodes
execute each Art once
```

- [ ] **Step 3: Test disable and uninstall**

For each framework:

```text
disable framework -> dependent Art readiness false
enable framework -> dependent Art readiness true
uninstall framework -> dependent Art execution returns framework_not_ready
reinstall framework -> dependent Art executes again
```

For each Art:

```text
disable Art -> Hook no longer offers it as runnable
enable Art -> Hook offers it again
uninstall Art -> Hook no longer lists it
reinstall Art -> Hook lists and executes it
```

- [ ] **Step 4: Prove third-party no-source-change path**

The smoke must create a temporary `third-party-echo` framework and `third-party-image-echo` Art outside the repository tree, package them, install them, instantiate them, and execute them once.

Acceptance evidence:

```json
{
  "thirdPartyFrameworkInstalled": true,
  "thirdPartyArtInstalled": true,
  "thirdPartyArtExecuted": true,
  "loomSourceChanged": false,
  "hookSourceChanged": false
}
```

- [ ] **Step 5: Wire into release verification**

Add the plugin boundary smoke to `verify-release.ps1 -RunSmoke` after the existing framework/store smoke.

- [ ] **Step 6: Run local packaged smoke**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomPluginBoundarySmoke.ps1 `
  -Configuration Debug `
  -EvidenceRoot .\target\plugin-boundary-smoke
```

- [ ] **Step 7: Commit**

```powershell
git add -- scripts/Invoke-LoomPluginBoundarySmoke.ps1 scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1 scripts/verify-release.ps1
git commit -m "test(loom): prove plugin art framework boundary"
```

## Task 9: Update documentation and remove default-build/resource leakage

**Files:**
- Modify: `README.md`
- Modify: `Cargo.toml`
- Modify: `scripts/build-release.ps1`
- Modify: `scripts/verify-release.ps1`
- Modify: `docs/progress/phase-67-pluginized-art-frameworks.md`

- [ ] **Step 1: Update README**

Replace the current default-installed statement with:

```text
Loom ships the host, installer, registry, and execution broker by default.
Optional Art frameworks are distributed as separate packages and must be
installed before dependent Arts can be installed or executed.
```

- [ ] **Step 2: Guard release payload**

Release verification must fail if the default desktop package contains:

```text
framework-packages/
art-packages/samples/
resources/script-arts/
resources/workflow-arts/
framework-runtimes/
```

unless those are inside an explicit optional plugin package artifact, not the default `Loom.exe` runtime tree.

- [ ] **Step 3: Guard Cargo workspace membership**

If framework runtimes are Rust crates, root `Cargo.toml` must not include them in `workspace.members`.

- [ ] **Step 4: Run verification**

```powershell
cargo fmt --manifest-path .\Cargo.toml --all -- --check
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry -- --nocapture
cargo test --manifest-path .\Cargo.toml -p loom-daemon -- --nocapture
npm test --prefix apps/desktop
npm run typecheck --prefix apps/desktop
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ArtPluginBoundaryContract.ps1
```

- [ ] **Step 5: Commit**

```powershell
git add -- README.md Cargo.toml scripts/build-release.ps1 scripts/verify-release.ps1 docs/progress/phase-67-pluginized-art-frameworks.md
git commit -m "docs(loom): document plugin art framework boundary"
```

## Task 10: Build final Loom and Hook releases

**Files:**
- Output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-pluginized-art-frameworks`
- Output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\<matching-version-if-hook-changed>`

- [ ] **Step 1: Build framework and Art package artifacts**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Build-LoomArtFrameworkPackages.ps1 `
  -OutputRoot .\.loom-art-store-data\frameworks

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Build-LoomSampleArtPackages.ps1 `
  -OutputRoot .\.loom-art-store-data\arts
```

- [ ] **Step 2: Build Loom release**

From `C:\Users\Public\nas_home\AI\GameEditor\Neuro`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release-exes.ps1 `
  -Apps Loom `
  -VersionId 20260801-pluginized-art-frameworks `
  -Force
```

- [ ] **Step 3: Run release verification**

From `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-pluginized-art-frameworks `
  -RunSmoke
```

Expected:

```text
smoke=passed
hookCanvasSmoke=passed
frameworkArtStoreHookSmoke=passed
pluginBoundarySmoke=passed
```

- [ ] **Step 4: Build Hook release if Hook source changed**

Use the existing Hook release command from the Hook repository release docs. The final report must include the exact package path and SHA-256.

- [ ] **Step 5: Commit final progress**

```powershell
git add -- docs/progress/phase-67-pluginized-art-frameworks.md
git commit -m "docs(loom): record pluginized framework acceptance"
```

## Final acceptance checklist

- [ ] Baseline tags exist in Loom and Hook.
- [ ] The boundary contract test passes.
- [ ] The framework package contract test passes.
- [ ] A fresh control-plane root has zero installed optional frameworks.
- [ ] Each of the six framework packages installs, disables, enables, upgrades, uninstalls, and reinstalls.
- [ ] Each of the six sample Art packages installs only after its framework is installed.
- [ ] Each of the six sample Art packages executes once through Hook.
- [ ] A temporary third-party framework and third-party Art install and execute without source edits.
- [ ] Hook source has no production branch on sample Art IDs.
- [ ] Loom source has no production branch on sample Art IDs outside documented fixtures/tests.
- [ ] Default release payload excludes optional framework runtimes and sample Art resources.
- [ ] `verify-release.ps1 -RunSmoke` includes and passes the plugin boundary smoke.
- [ ] Final release is built under the required Neuro release root.

