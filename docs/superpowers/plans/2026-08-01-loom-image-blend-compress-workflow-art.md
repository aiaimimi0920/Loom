# Loom Image Blend And Compress Workflow Art Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship and validate an installable `workflow` Art that blends two required images through `custom-image-blend-script`, compresses the blend through `custom-1770146354922`, and returns the compressed image with optional blend and quality parameters.

**Architecture:** Store the workflow graph and Art manifest as repository-owned resources. A dedicated PowerShell installer packages and installs both resources, declares the two child Art dependencies, persists the workflow YAML, and updates the Loom tool registry. Existing `loom_workflow_runtime` and daemon Hook Bridge paths execute the graph; new tests use the real blend script plus a deterministic CLI-wrapper fixture, while live acceptance uses the installed Pingo Art.

**Tech Stack:** Rust, serde/serde_yaml, Loom workflow runtime, Loom tool registry, Hook Bridge AHRP WebSocket, PowerShell 5+, System.Drawing, Pingo CLI, existing Neuro release scripts.

---

## File Map

- Create `resources/workflow-arts/image-blend-compress/workflow.yaml`
  - Declarative two-node graph and fixed child defaults.
- Create `resources/workflow-arts/image-blend-compress/manifest.json`
  - Stable public Art contract, workflow bindings, output binding, and dependencies.
- Modify `crates/loom_workflow_runtime/Cargo.toml`
  - Add the image codec as a test-only dependency for pixel-level assertions.
- Modify `crates/loom_workflow_runtime/src/lib.rs`
  - Add a real cross-framework workflow acceptance test and deterministic CLI fixture.
- Modify `apps/daemon/src/lib.rs`
  - Add an AHRP `art/process` integration test for two images and two scalar bindings.
- Create `scripts/tests/Test-ImageBlendCompressWorkflowArtContract.ps1`
  - Source/package contract checks for the manifest, workflow, installer, and live smoke script.
- Create `scripts/Install-LoomImageBlendCompressWorkflowArt.ps1`
  - Package, publish, dependency-check, persist, install, and broadcast the workflow Art.
- Create `scripts/Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1`
  - Repeatable real WebSocket acceptance test that emits image and JSON evidence.
- Create `docs/progress/phase-66-image-blend-compress-workflow-art.md`
  - Final implementation, verification, live evidence, and release record.

## Task 1: Add Failing Cross-Framework Workflow Acceptance Tests

**Files:**
- Modify: `crates/loom_workflow_runtime/Cargo.toml`
- Modify: `crates/loom_workflow_runtime/src/lib.rs`
- Modify: `apps/daemon/src/lib.rs`
- Create after RED: `resources/workflow-arts/image-blend-compress/workflow.yaml`
- Create after RED: `resources/workflow-arts/image-blend-compress/manifest.json`

- [ ] **Step 1: Add the workflow-runtime test dependency**

Append this test-only dependency to
`crates/loom_workflow_runtime/Cargo.toml`:

```toml
[dev-dependencies]
loom_image_io.workspace = true
```

- [ ] **Step 2: Add test helpers that locate the not-yet-created resources**

In both Rust test modules, add a helper anchored at `CARGO_MANIFEST_DIR`:

```rust
fn workspace_image_blend_compress_resource(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|candidate| {
            let path = candidate
                .join("resources")
                .join("workflow-arts")
                .join("image-blend-compress")
                .join(name);
            path.exists().then_some(path)
        })
        .unwrap_or_else(|| panic!("locate image-blend-compress resource `{name}`"))
}
```

Add a workflow-runtime helper that finds the existing production blend script:

```rust
fn workspace_image_blend_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|candidate| {
            let path = candidate
                .join("resources")
                .join("script-arts")
                .join("image-blend")
                .join("main.ps1");
            path.exists().then_some(path)
        })
        .expect("locate production image blend script")
}
```

- [ ] **Step 3: Add deterministic CLI-wrapper fixtures**

Under `#[cfg(windows)]`, write a PowerShell fixture that copies its input image
to the requested output and records the received quality value:

```rust
fn write_cli_image_copy_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let script_path = root.join("fixture-compress.ps1");
    let evidence_path = root.join("compress-evidence.txt");
    let source = r#"
param(
    [Parameter(Mandatory = $true)][string]$InputPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][int]$Quality,
    [Parameter(Mandatory = $true)][string]$EvidencePath
)
$ErrorActionPreference = "Stop"
Copy-Item -LiteralPath $InputPath -Destination $OutputPath -Force
[System.IO.File]::WriteAllText(
    $EvidencePath,
    [string]$Quality,
    [System.Text.UTF8Encoding]::new($false)
)
"#;
    fs::write(&script_path, source).expect("write CLI image copy fixture");
    (script_path, evidence_path)
}
```

Register this fixture under the production compression Art ID:

```rust
fn save_fixture_compress_tool(
    registry: &ToolRegistry,
    script_path: &Path,
    evidence_path: &Path,
) {
    registry
        .save_tool(ToolDefinition::new(
            "custom-1770146354922",
            "Fixture Image Compress",
            "Deterministic cli_wrapper child for workflow tests",
            ToolExecution::CliWrapper {
                command: "powershell.exe".to_owned(),
                args: vec![
                    "-NoProfile".to_owned(),
                    "-ExecutionPolicy".to_owned(),
                    "Bypass".to_owned(),
                    "-File".to_owned(),
                    script_path.display().to_string(),
                    "-InputPath".to_owned(),
                    "{{input}}".to_owned(),
                    "-OutputPath".to_owned(),
                    "{{output}}".to_owned(),
                    "-Quality".to_owned(),
                    "{{quality_num}}".to_owned(),
                    "-EvidencePath".to_owned(),
                    evidence_path.display().to_string(),
                ],
            },
        ))
        .expect("save fixture compression tool");
}
```

Add an equivalent daemon-test-local fixture in `apps/daemon/src/lib.rs`. It
must use its own temporary script and evidence paths and register the same
production Art ID, because the daemon test module cannot import private helpers
from `loom_workflow_runtime`.

- [ ] **Step 4: Add the workflow-runtime acceptance test before resources exist**

Add a Windows test that:

1. Reads `workflow.yaml` and `manifest.json` through the resource helper.
2. Parses the manifest into `ToolDefinition`.
3. Registers `custom-image-blend-script` with the production blend script.
4. Registers `custom-1770146354922` with the CLI fixture.
5. Saves the workflow YAML.
6. Executes the workflow with two one-pixel images, `mix_ratio = 25`, and
   `quality_num = 73`.
7. Decodes the terminal image and asserts RGBA `[190, 85, 50, 255]`.
8. Asserts the CLI evidence file contains `73`.

Use these inputs:

```rust
let source = loom_image_io::rgba8_to_png_data_url(1, 1, &[240, 60, 0, 255])
    .expect("encode workflow source image");
let reference = loom_image_io::rgba8_to_png_data_url(1, 1, &[40, 160, 200, 255])
    .expect("encode workflow reference image");
```

Execute with:

```rust
let result = execute_tool_with_workflows(
    &workflow_tool,
    &[],
    &workflow_store,
    &tool_registry,
    json!({
        "input_base64": source,
        "reference": reference,
        "mix_ratio": 25,
        "quality_num": 73
    }),
)
.expect("execute image blend compress workflow");
```

- [ ] **Step 5: Add the daemon Hook Bridge acceptance test before resources exist**

Add a Windows test named:

```rust
daemon_hook_bridge_process_executes_image_blend_compress_workflow_art
```

Follow the existing `daemon_hook_bridge_process_uses_explicit_auxiliary_input_images_for_script_blend`
fixture pattern. Save the production blend script tool, the deterministic CLI
fixture, the repository workflow YAML, and the parsed repository workflow Art
manifest into a temporary control plane. Send:

```json
{
  "method": "art/process",
  "params": {
    "request_id": "req-image-blend-compress-workflow",
    "art_id": "custom-image-blend-compress-workflow",
    "input": {
      "type": "base64",
      "data": "<source data URL>",
      "width": 1,
      "height": 1,
      "format": "rgba8"
    },
    "params": {
      "mix_ratio": 25,
      "quality_num": 73
    },
    "input_images": {
      "reference": "<reference data URL>"
    },
    "disabled_params": []
  }
}
```

Assert:

```rust
assert_eq!(response["request_id"], "req-image-blend-compress-workflow");
assert_eq!(response["status"], "Success");
assert_eq!(response["data"]["output"]["type"], "base64");
assert_eq!(decoded.width, 1);
assert_eq!(decoded.height, 1);
assert_eq!(decoded.data, vec![190, 85, 50, 255]);
assert_eq!(fs::read_to_string(evidence_path).unwrap(), "73");
```

- [ ] **Step 6: Run the two tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p loom_workflow_runtime `
  image_blend_compress -- --nocapture

cargo test --manifest-path .\Cargo.toml -p loom-daemon `
  image_blend_compress -- --nocapture
```

Expected: both tests fail because
`resources/workflow-arts/image-blend-compress/workflow.yaml` and
`manifest.json` do not exist.

- [ ] **Step 7: Add the workflow YAML**

Create `resources/workflow-arts/image-blend-compress/workflow.yaml`:

```yaml
name: 图片融合并压缩
nodes:
  - id: blend
    uses: custom-image-blend-script
    with:
      mix_ratio: 50

  - id: compress
    uses: custom-1770146354922
    needs:
      - blend
    with:
      input: ${{ nodes.blend.outputs.output_base64 }}
      level_num: 2
      quality_num: 90
      lossless: false
```

- [ ] **Step 8: Add the workflow Art manifest**

Create `resources/workflow-arts/image-blend-compress/manifest.json`:

```json
{
  "id": "custom-image-blend-compress-workflow",
  "name": "图片融合并压缩",
  "description": "先混合图片 A 与图片 B，再使用 Pingo 压缩并输出最终图片",
  "enabled": true,
  "execution": {
    "type": "workflow",
    "workflowId": "image-blend-compress-workflow",
    "workflowBindings": {
      "inputs": [
        {
          "workflowParam": "input",
          "nodeId": "blend",
          "target": "input",
          "kind": "input_image"
        },
        {
          "workflowParam": "reference",
          "nodeId": "blend",
          "target": "reference",
          "kind": "param"
        },
        {
          "workflowParam": "mix_ratio",
          "nodeId": "blend",
          "target": "mix_ratio",
          "kind": "param"
        },
        {
          "workflowParam": "quality_num",
          "nodeId": "compress",
          "target": "quality_num",
          "kind": "param"
        }
      ],
      "primaryOutput": {
        "nodeId": "compress",
        "output": "output_base64",
        "kind": "node_result"
      }
    }
  },
  "inputs": [
    {
      "name": "input",
      "label": "图片 A",
      "type": "image",
      "execution_type": "image_buffer"
    },
    {
      "name": "reference",
      "label": "图片 B",
      "type": "image",
      "execution_type": "image_buffer",
      "exposePort": true
    }
  ],
  "outputs": [
    {
      "name": "output",
      "label": "结果",
      "type": "image",
      "execution_type": "image_buffer"
    }
  ],
  "params": [
    {
      "id": "mix_ratio",
      "label": "融合值",
      "widget": "slider",
      "default": 50,
      "min": 0,
      "max": 100,
      "step": 1,
      "disabled": false,
      "data_type": "number"
    },
    {
      "id": "quality_num",
      "label": "压缩比例",
      "widget": "slider",
      "default": 90,
      "min": 60,
      "max": 100,
      "step": 1,
      "disabled": false,
      "data_type": "number"
    }
  ],
  "metadata": {
    "dependencies": {
      "framework": "workflow",
      "arts": [
        "custom-image-blend-script",
        "custom-1770146354922"
      ]
    },
    "artloomCompat": {
      "executionType": "workflow",
      "source": "loom-local",
      "execution": {
        "workflowId": "image-blend-compress-workflow",
        "sourceType": "installed"
      }
    }
  }
}
```

- [ ] **Step 9: Run the tests and verify GREEN**

Run the two commands from Step 6. Expected: both targeted tests pass, the
decoded image pixel is `[190, 85, 50, 255]`, and the CLI fixture records `73`.

- [ ] **Step 10: Format and commit the runtime/resources batch**

```powershell
cargo fmt --manifest-path .\Cargo.toml --all
git add -- `
  crates/loom_workflow_runtime/Cargo.toml `
  crates/loom_workflow_runtime/src/lib.rs `
  apps/daemon/src/lib.rs `
  resources/workflow-arts/image-blend-compress/workflow.yaml `
  resources/workflow-arts/image-blend-compress/manifest.json
git commit -m "feat(loom): add blend compress workflow art resources"
```

## Task 2: Add the Installer Contract and Installer

**Files:**
- Create: `scripts/tests/Test-ImageBlendCompressWorkflowArtContract.ps1`
- Create: `scripts/Install-LoomImageBlendCompressWorkflowArt.ps1`

- [ ] **Step 1: Write the failing installer contract test**

The contract test must:

1. Resolve the repository root from `$PSScriptRoot`.
2. Require the manifest, workflow, installer, and smoke script paths.
3. Parse the manifest and assert the exact IDs and execution type.
4. Assert exactly four workflow bindings with the approved node/target/kind
   tuples.
5. Assert dependency Arts are exactly the two production child IDs.
6. Parse workflow text and require `blend`, `compress`, `needs`, fixed
   `level_num: 2`, fixed `lossless: false`, and the blend output reference.
7. Parse both scripts with the PowerShell parser and fail on syntax errors.

Use this assertion helper:

```powershell
function Assert-Equal {
    param($Expected, $Actual, [string]$Message)
    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}
```

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ImageBlendCompressWorkflowArtContract.ps1
```

Expected: FAIL because the installer and smoke script do not exist.

- [ ] **Step 2: Implement the dedicated installer**

Create `scripts/Install-LoomImageBlendCompressWorkflowArt.ps1` with these
parameters and defaults:

```powershell
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [string]$ArtId = "custom-image-blend-compress-workflow",
    [string]$WorkflowId = "image-blend-compress-workflow",
    [string]$StoreRoot,
    [string]$StoreUrl = "http://127.0.0.1:8790",
    [string]$ControlPlaneRoot,
    [ValidateSet("local", "store", "upload")]
    [string]$InstallMode = "local",
    [switch]$SkipInstall,
    [switch]$SkipPublish
)
```

The implementation must perform these exact operations:

```powershell
$repoRoot = Split-Path -Parent $PSScriptRoot
$resourceRoot = Join-Path $repoRoot "resources\workflow-arts\image-blend-compress"
$manifestPath = Join-Path $resourceRoot "manifest.json"
$workflowPath = Join-Path $resourceRoot "workflow.yaml"
$workRoot = Join-Path $repoRoot "target\art-packages\image-blend-compress-workflow"
$stageRoot = Join-Path $workRoot "stage"
$stageWorkflowRoot = Join-Path $stageRoot "workflow"
$packagePath = Join-Path $workRoot "$ArtId.zip"
```

Validate the static manifest before packaging:

```powershell
$manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
if ([string]$manifest.id -ne $ArtId) { throw "Manifest Art id mismatch" }
if ([string]$manifest.execution.type -ne "workflow") { throw "Manifest must use workflow execution" }
if ([string]$manifest.execution.workflowId -ne $WorkflowId) { throw "Manifest workflow id mismatch" }
```

Stage and package:

```powershell
Remove-Item -Recurse -Force -LiteralPath $stageRoot -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stageWorkflowRoot | Out-Null
Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $stageRoot "manifest.json") -Force
Copy-Item -LiteralPath $workflowPath -Destination (Join-Path $stageWorkflowRoot "workflow.yaml") -Force
[System.IO.Compression.ZipFile]::CreateFromDirectory($stageRoot, $packagePath)
```

Before local installation, read `tools/tools.json` and require both child IDs.
Also read `frameworks.json` and require `workflow`. Error messages must name the
missing framework or Art ID.

For local installation:

```powershell
$artDir = Join-Path $ControlPlaneRoot "arts\$ArtId"
$workflowsDir = Join-Path $ControlPlaneRoot "workflows"
$toolsDir = Join-Path $ControlPlaneRoot "tools"
$toolsPath = Join-Path $toolsDir "tools.json"
New-Item -ItemType Directory -Force -Path $artDir, $workflowsDir, $toolsDir | Out-Null
Get-ChildItem -LiteralPath $stageRoot -Force |
    Copy-Item -Destination $artDir -Recurse -Force
Copy-Item -LiteralPath $workflowPath -Destination (Join-Path $workflowsDir "$WorkflowId.yaml") -Force
```

Replace only the matching Art entry in `tools.json`, preserve all unrelated
entries, sort by ID, and write UTF-8 without BOM. For `store` and `upload`, use
the same endpoints as the existing installers, then persist the workflow with:

```powershell
$workflowBody = @{ data = (Get-Content -Raw -Encoding UTF8 -LiteralPath $workflowPath) } |
    ConvertTo-Json -Depth 10
Invoke-RestMethod `
    -Uri ($BaseUrl.TrimEnd('/') + "/v1/workflows/$WorkflowId") `
    -Method Put `
    -ContentType "application/json" `
    -Body $workflowBody `
    -TimeoutSec 30
```

After any successful install, best-effort POST
`/v1/artloom-compat/arts/broadcast-updated`. Emit a JSON report containing:

```text
artId, workflowId, packagePath, publishedZipPath, controlPlaneRoot,
installMode, artDir, workflowPath, toolsPath
```

- [ ] **Step 3: Add the smoke-script path check but leave it RED**

Keep the contract test requiring
`scripts/Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1`; rerun it and
confirm the only remaining failure is the missing smoke script.

## Task 3: Add the Repeatable Live WebSocket Smoke

**Files:**
- Create: `scripts/Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1`
- Test: `scripts/tests/Test-ImageBlendCompressWorkflowArtContract.ps1`

- [ ] **Step 1: Implement the smoke script**

Use parameters:

```powershell
[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8765",
    [int]$BridgePort = 19820,
    [string]$ArtId = "custom-image-blend-compress-workflow",
    [int]$MixRatio = 25,
    [int]$Quality = 90,
    [int]$Size = 64,
    [string]$OutputDir
)
```

The script must:

1. Validate `MixRatio` is `0..100`, `Quality` is `60..100`, and `Size > 0`.
2. Confirm `$BaseUrl/status` is ready and Hook Bridge reports running.
3. Generate a solid source PNG `(240,60,0,255)` and reference PNG
   `(40,160,200,255)` using `System.Drawing`.
4. Connect to `ws://127.0.0.1:$BridgePort` with `ClientWebSocket`.
5. Send `art/process` with source as primary input, reference under
   `input_images.reference`, and both scalar params.
6. Ignore unrelated messages until the matching `request_id` arrives.
7. Require `status = Success` and a non-empty base64 output.
8. Decode the output, require `$Size x $Size`, and save `output.png`.
9. Record pixel `(0,0)`, elapsed milliseconds, PNG bytes, and response chars.
10. Write `summary.json` as UTF-8 without BOM and print the same JSON.

Use a 60-second receive timeout. The request body must be:

```powershell
$request = [ordered]@{
    method = "art/process"
    params = [ordered]@{
        request_id = $requestId
        art_id = $ArtId
        input = [ordered]@{
            type = "base64"
            data = $source
            width = $Size
            height = $Size
            format = "rgba8"
        }
        params = [ordered]@{
            mix_ratio = $MixRatio
            quality_num = $Quality
        }
        input_images = [ordered]@{
            reference = $reference
        }
        disabled_params = @()
    }
}
```

- [ ] **Step 2: Run the source contract and verify GREEN**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ImageBlendCompressWorkflowArtContract.ps1
```

Expected: PASS with manifest, workflow, installer, and smoke script contracts
all satisfied.

- [ ] **Step 3: Commit installer and smoke tooling**

```powershell
git add -- `
  scripts/tests/Test-ImageBlendCompressWorkflowArtContract.ps1 `
  scripts/Install-LoomImageBlendCompressWorkflowArt.ps1 `
  scripts/Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1
git commit -m "feat(loom): install blend compress workflow art"
```

## Task 4: Install the Production Art and Run Real Child Arts

**Files:**
- Local control plane under `%APPDATA%\Loom\control-plane`
- Evidence under `output\smoke\image-blend-compress-workflow`

- [ ] **Step 1: Refresh the two production child Arts**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageBlendScriptArt.ps1

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageCompressArt.ps1
```

Expected: both installers report their stable production IDs and local Art
directories.

- [ ] **Step 2: Install the workflow Art**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageBlendCompressWorkflowArt.ps1
```

Expected: the report names `custom-image-blend-compress-workflow`, writes
`image-blend-compress-workflow.yaml`, updates `tools.json`, and broadcasts the
Art update.

- [ ] **Step 3: Verify daemon-visible definitions**

```powershell
$tool = (Invoke-RestMethod http://127.0.0.1:8765/v1/tools).tools |
    Where-Object id -eq 'custom-image-blend-compress-workflow'
$tool | ConvertTo-Json -Depth 30

Invoke-RestMethod `
  http://127.0.0.1:8765/v1/workflows/image-blend-compress-workflow |
  ConvertTo-Json -Depth 20
```

Expected: execution type is `workflow`, both inputs and both params are
present, and the workflow contains `blend` followed by `compress`.

- [ ] **Step 4: Run the real WebSocket acceptance smoke**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1 `
  -OutputDir .\output\smoke\image-blend-compress-workflow
```

Expected:

- `status = Success`
- output dimensions `64 x 64`
- output PNG is decodable and non-empty
- response arrives within 60 seconds
- `output.png` and `summary.json` exist
- the representative pixel is the compressed form of the expected blend near
  `(190,85,50,255)`

- [ ] **Step 5: Inspect current Hook runtime log**

```powershell
$log = Join-Path $env:LOCALAPPDATA 'Hook\logs\hook-runtime.log'
Select-String -LiteralPath $log `
  -Pattern 'custom-image-blend-compress-workflow|Failed to read ArtLoom response' |
  Select-Object -Last 80
```

Expected: requests show `has_reference_input_image=true`, parameter keys include
`mix_ratio,quality_num`, and no new read timeout is associated with the
workflow Art request.

## Task 5: Run Full Regression Verification

**Files:**
- No production edits unless a failing regression identifies a root cause.

- [ ] **Step 1: Run formatting check**

```powershell
cargo fmt --manifest-path .\Cargo.toml --all -- --check
```

Expected: exit code `0`.

- [ ] **Step 2: Run workflow-runtime tests**

```powershell
cargo test --manifest-path .\Cargo.toml -p loom_workflow_runtime -- --nocapture
```

Expected: all workflow-runtime tests pass, including the new cross-framework
image workflow test.

- [ ] **Step 3: Run targeted daemon workflow and script/CLI coverage**

```powershell
cargo test --manifest-path .\Cargo.toml -p loom-daemon workflow -- --nocapture
cargo test --manifest-path .\Cargo.toml -p loom-daemon image_blend_compress -- --nocapture
cargo test --manifest-path .\Cargo.toml -p loom_tool_registry execute_script -- --nocapture
```

Expected: zero failures.

- [ ] **Step 4: Run installer contract again**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ImageBlendCompressWorkflowArtContract.ps1
```

Expected: PASS.

- [ ] **Step 5: Re-run real live acceptance after regression tests**

Run the Task 4 smoke command again. Expected: `Success` with a decodable image.

## Task 6: Record Progress and Build the Loom Release

**Files:**
- Create: `docs/progress/phase-66-image-blend-compress-workflow-art.md`
- Output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-workflow-image-blend-compress-art`

- [ ] **Step 1: Write the progress record**

Document:

- why this Art is the workflow-framework acceptance case;
- stable workflow and Art IDs;
- child Art IDs and frameworks;
- public inputs and defaults;
- exact RED and GREEN commands;
- live smoke summary values;
- release path and ZIP checksum;
- any remaining non-runtime release verifier mismatch.

The status must be `Complete` only after Tasks 1 through 5 pass.

- [ ] **Step 2: Build a fresh Loom release**

From the Neuro parent repository:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release-exes.ps1 `
  -Apps Loom `
  -VersionId 20260801-workflow-image-blend-compress-art `
  -Force
```

Expected release executables:

```text
release\Loom\20260801-workflow-image-blend-compress-art\loom.exe
release\Loom\20260801-workflow-image-blend-compress-art\loom-daemon.exe
release\Loom\20260801-workflow-image-blend-compress-art\loom-desktop.exe
```

- [ ] **Step 3: Copy the generated Art package into release evidence**

```powershell
$release = '.\release\Loom\20260801-workflow-image-blend-compress-art'
$artPackages = Join-Path $release 'art-packages'
New-Item -ItemType Directory -Force -Path $artPackages | Out-Null
Copy-Item `
  -LiteralPath '.\Loom\target\art-packages\image-blend-compress-workflow\custom-image-blend-compress-workflow.zip' `
  -Destination $artPackages `
  -Force
Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $artPackages 'custom-image-blend-compress-workflow.zip')
```

- [ ] **Step 4: Verify release artifacts and checksums**

```powershell
Get-ChildItem -LiteralPath `
  '.\release\Loom\20260801-workflow-image-blend-compress-art' `
  -File | Select-Object Name,Length,LastWriteTime

Get-FileHash -Algorithm SHA256 -LiteralPath `
  '.\release\Loom\20260801-workflow-image-blend-compress-art\packages\Loom-20260801-workflow-image-blend-compress-art-windows-x64.zip'
```

Expected: all three executables exist and the ZIP hash is recorded in the
progress document.

- [ ] **Step 5: Commit the progress record**

```powershell
git add -- docs/progress/phase-66-image-blend-compress-workflow-art.md
git commit -m "docs(loom): record workflow art acceptance"
```

## Final Acceptance Checklist

- [ ] Installed Art reports `execution.type = workflow`.
- [ ] Manifest declares both child Art dependencies.
- [ ] Hook receives two required image ports.
- [ ] Hook exposes `mix_ratio` and `quality_num` with approved defaults/ranges.
- [ ] Workflow-runtime cross-framework test passes.
- [ ] Daemon AHRP integration test passes.
- [ ] Real installed blend and Pingo children return a valid final image.
- [ ] Missing child dependency produces a named installation error.
- [ ] Missing image input produces execution failure, not pass-through success.
- [ ] Source contract, Rust tests, formatting, and live smoke pass.
- [ ] New Loom release and workflow Art ZIP are present under the required release root.
