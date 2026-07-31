# Phase 66: Image blend and compress workflow Art

## Status

Complete.

## Why this phase exists

Loom already had independently usable Art execution frameworks, including the
`script` framework used by `图片融合` and the `cli_wrapper` framework used by
Pingo `图片压缩`. The remaining acceptance gap was proving that the
`workflow` framework could install and execute a real cross-framework graph,
bind multiple image inputs and scalar parameters to child Arts, and return the
terminal child image through Hook Bridge.

This phase closes that gap with an installable `图片融合并压缩` Art. It first
executes the production Script Art to blend two images, then passes that image
to the production Pingo CLI-wrapper Art, and finally returns the compressed
PNG.

## Stable contract

- Art ID: `custom-image-blend-compress-workflow`
- Workflow ID: `image-blend-compress-workflow`
- Framework: `workflow`
- Child Art 1:
  - ID: `custom-image-blend-script`
  - Framework: `script`
  - Responsibility: blend image A and image B.
- Child Art 2:
  - ID: `custom-1770146354922`
  - Framework: `cli_wrapper`
  - Responsibility: compress the blended image with Pingo.

Public inputs and parameters:

| Name | UI label | Required | Binding | Default/range |
| --- | --- | --- | --- | --- |
| `input` | 图片 A | Yes | `blend.input` as `input_image` | Image port |
| `reference` | 图片 B | Yes | `blend.reference` as `param` | Exposed image port |
| `mix_ratio` | 融合值 | No | `blend.mix_ratio` as `param` | `50`, range `0..100` |
| `quality_num` | 压缩比例 | No | `compress.quality_num` as `param` | `90`, range `60..100` |

The workflow fixes Pingo's internal `level_num` to `2` and `lossless` to
`false`. The public `压缩比例` value maps directly to Pingo's `quality_num`,
as approved in方案 A.

## Implemented

### Workflow resources

- Added `resources/workflow-arts/image-blend-compress/workflow.yaml`.
- Added `resources/workflow-arts/image-blend-compress/manifest.json`.
- The graph executes:
  1. `blend` using `custom-image-blend-script`;
  2. `compress` using `custom-1770146354922` and depending on `blend`;
  3. terminal `compress.output_base64` as the workflow Art result.
- The manifest declares both child Art dependencies and all four public
  bindings.

### Runtime acceptance coverage

- Added a `loom_workflow_runtime` cross-framework acceptance test:
  - `workflow_runtime_executes_image_blend_then_cli_compress_with_bound_inputs_and_params`
- Added a daemon Hook Bridge AHRP integration test:
  - `daemon_hook_bridge_process_executes_image_blend_compress_workflow_art`
- Both tests use the real production blend script and a deterministic
  CLI-wrapper compression fixture.
- The fixture records the received Pingo quality value, so the tests verify
  both image flow and scalar binding rather than only checking for a nominal
  success result.
- Test source RGBA is `(240,60,0,255)`, reference RGBA is
  `(40,160,200,255)`, `mix_ratio` is `25`, and `quality_num` is `73`.
- The asserted terminal RGBA is `[190,85,50,255]`, and the compression fixture
  must record `73`.

### Installation and smoke tooling

- Added `scripts/Install-LoomImageBlendCompressWorkflowArt.ps1`.
- Added `scripts/Invoke-LoomImageBlendCompressWorkflowArtSmoke.ps1`.
- Added `scripts/tests/Test-ImageBlendCompressWorkflowArtContract.ps1`.
- The installer supports:
  - `local`: direct control-plane installation;
  - `store`: Art Store publication and installation;
  - `upload`: ZIP upload installation through the daemon API.
- Before local installation, the installer requires:
  - the `workflow` framework;
  - `custom-image-blend-script`;
  - `custom-1770146354922`.
- The installer updates only the matching Art entry in `tools.json`, preserves
  unrelated tools, persists the workflow YAML, and broadcasts the Art update.
- The smoke script generates two real PNGs, calls `art/process` through the
  WebSocket Hook Bridge, decodes the terminal image, validates dimensions and
  representative pixel values, and writes `output.png` plus `summary.json`.

## TDD evidence

Before the workflow resources were added, the new runtime test failed with:

```text
locate image-blend-compress resource `workflow.yaml`
```

After adding the manifest and workflow resources, the same test passed and
verified the expected output pixel plus the propagated compression quality.

The PowerShell contract test also exposed two implementation defects through
RED/GREEN iterations:

1. Windows PowerShell sent workflow JSON bodies without explicit UTF-8 bytes.
   The repository and ZIP contained `图片融合并压缩`, but the installed
   workflow contained `???????`. The installer now sends UTF-8 bytes with
   `application/json; charset=utf-8`, and the installed workflow SHA-256 now
   matches the repository source.
2. StrictMode error reporting accessed `$Response.error.message` directly.
   When `error` was a string, that access masked the real Art failure. The
   smoke script now safely checks `error.message`, `error`, nested
   `data.error`, and message fallbacks without assuming an object shape.

## Verification evidence

### Rust and source-contract regression

The following commands completed with zero failures during implementation:

```powershell
cargo fmt --manifest-path .\Cargo.toml --all -- --check

cargo test --manifest-path .\Cargo.toml `
  -p loom_workflow_runtime -- --nocapture

cargo test --manifest-path .\Cargo.toml `
  -p loom-daemon workflow -- --nocapture

cargo test --manifest-path .\Cargo.toml `
  -p loom-daemon `
  daemon_hook_bridge_process_executes_image_blend_compress_workflow_art `
  -- --nocapture

cargo test --manifest-path .\Cargo.toml `
  -p loom_tool_registry execute_script -- --nocapture

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-ImageBlendCompressWorkflowArtContract.ps1
```

Observed results:

- `loom_workflow_runtime`: `6 passed; 0 failed`.
- `loom-daemon workflow`: `13 passed; 0 failed`.
- Targeted daemon composite test: `1 passed; 0 failed`.
- Script-framework regression: `7 passed; 0 failed`.
- PowerShell contract: `Image blend compress workflow Art contract passed.`
- Cargo formatting check: exit code `0`.

The final fresh verification pass repeated the minimum release gate after the
isolated binary smoke:

```text
cargo-fmt-check: PASS
loom_workflow_runtime: 6 passed; 0 failed
daemon composite workflow test: 1 passed; 0 failed
PowerShell workflow Art contract: PASS
overall exit code: 0
```

### Real installed child-Art smoke

The real installed workflow executed the production PowerShell blend Art and
the downloaded official Pingo executable, rather than the deterministic test
fixture.

Representative successful runs:

| Installation path | Status | Response | Pixel | PNG bytes |
| --- | --- | ---: | --- | ---: |
| Local install | `Success` | `858 ms` | `[190,85,50,255]` | `86` |
| Upload install | `Success` | `1019 ms` | `[190,85,50,255]` | `86` |

Daemon-visible registration reported:

```text
id: custom-image-blend-compress-workflow
executionType: workflow
inputs: input, reference
params: mix_ratio, quality_num
```

### Negative paths

- In an isolated control plane without the blend child Art, installation
  failed with the named dependency error:

  ```text
  Required child Art is not installed: custom-image-blend-script
  ```

- A real Hook Bridge request without image B returned `EngineError` with:

  ```text
  reference image is required
  ```

  This proves the workflow does not silently pass image A through when a
  required image binding is missing.

## Release

Release directory:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260801-workflow-image-blend-compress-art
```

Release artifacts:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `loom.exe` | `2635264` | `d006d535768075ec853f5a1de391b2ebb0ade457b4a7e84b03cd5543b5deeab5` |
| `loom-daemon.exe` | `21586432` | `165269c98612e14d457ea345d65cd6442235ea1ca0ee1cee5f2bfd0ff875bbb1` |
| `loom-desktop.exe` | `8832000` | `762dcda2968313dd518904873397880d15c1c26e9c835ce484c7e78496dc3f14` |
| `Loom-20260801-workflow-image-blend-compress-art-windows-x64.zip` | `52767759` | `f6eb747b52c290c173370728ddbec2e385abec62c6c3f1bb45be3e381922608d` |
| `custom-image-blend-compress-workflow.zip` | `1237` | `7b30fff026390f79f0cf1021f059fa371b8b4889698d3f24ad5eb9e4d1491765` |

### Isolated release acceptance

The newly built `loom-daemon.exe` was run against a fresh, isolated control
plane on daemon port `18765`. The three Arts were installed into that control
plane, a random Hook Bridge port was started, and the release binary executed
the real workflow successfully:

```text
status: Success
responseMs: 842
width: 64
height: 64
pixel: [190,85,50,255]
outputPngBytes: 86
```

Evidence directory:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\release-workflow-image-blend-compress-8e0dfc729ef844948c3494e335f1d673
```

The isolated daemon stderr log was empty.

## Review note

Two independent reviewer attempts were made through the available subagents,
but both external review calls failed with `429 Too Many Requests` after their
retry limits. They were not repeated. A local strict review was performed
instead and found the StrictMode error-shape defect described above, which was
then covered by the PowerShell contract test and fixed.

## Acceptance checklist

- [x] Installed Art reports `execution.type = workflow`.
- [x] Manifest declares both child Art dependencies.
- [x] Hook receives two required image ports.
- [x] Hook exposes `mix_ratio` and `quality_num` with approved defaults and ranges.
- [x] Workflow-runtime cross-framework test passes.
- [x] Daemon AHRP integration test passes.
- [x] Real installed blend and Pingo children return a valid final image.
- [x] Missing child dependency produces a named installation error.
- [x] Missing image input produces execution failure rather than pass-through success.
- [x] Local, upload, source-contract, Rust, and formatting checks pass.
- [x] A fresh Loom release and workflow Art ZIP exist under the required release root.
- [x] The fresh release binary passes an isolated real-child workflow smoke.
