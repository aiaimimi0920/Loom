# Phase 65: Script image-blend Art

## Status

Complete.

## Why this phase exists

Loom's generic `script` framework had already been restored at the runtime
layer, but the repo did not yet ship a real, installable image-processing Art
that used that framework for something beyond echo/shader fixtures.

This phase closes that gap with a repo-owned `图片混合` Art that accepts a
source image, a reference image, and a blend ratio, then outputs a blended PNG
through the `script` execution path.

## Implemented

- Added a repo-owned task record:
  - `docs/superpowers/plans/2026-07-31-loom-script-image-blend-art.md`
- Added the production PowerShell Art script:
  - `resources/script-arts/image-blend/main.ps1`
- The script now:
  - accepts `input` / `reference` image values as data URLs, local paths,
    `file://`, or `asset.localhost` URLs;
  - rescales the reference image to the source image size;
  - blends both images by `mix_ratio` (`0..100`);
  - returns image output in Loom's normal `content[].type = image` shape.
  - supports an explicit duplicated-port opt-in on `reference` so Hook can
    expose it as a true second image input port while still preserving the
    matching parameter-row fallback.
- Hardened Loom's generic `script` runtime on Windows so PowerShell/Python
  script payloads are staged through temporary files instead of being passed as
  oversized command-line arguments, preventing `os error 206` for large image
  inputs.
- Added a repo-owned installer:
  - `scripts/Install-LoomImageBlendScriptArt.ps1`
- The installer now:
  - builds a formal Loom Art ZIP with `execution.type = "script"`;
  - publishes it into `.loom-art-store-data\arts\custom-image-blend-script.zip`;
  - installs it locally into `%APPDATA%\Loom\control-plane\arts\custom-image-blend-script`;
  - updates `%APPDATA%\Loom\control-plane\tools\tools.json`.
- Added `loom_tool_registry` regression coverage proving direct script execution
  can blend two image inputs correctly.
- Added `loom-daemon` Hook Bridge coverage proving
  `art_loom/execute_art_node` can execute the script Art and return a correct
  blended image.
- Added Hook-side regression coverage proving the opted-in duplicated
  `reference` image input remains visible as a node port for true two-image
  script Arts.
- Added Windows-specific regression coverage proving large script payloads now
  execute successfully through both:
  - direct `loom_tool_registry` script execution;
  - daemon Hook Bridge `execute_art_node`.

## Verification

Commands run:

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom_tool_registry `
  execute_script_tool_blends_input_and_reference_images_with_mix_ratio `
  -- --nocapture

cargo test --manifest-path Loom\Cargo.toml -p loom-daemon `
  daemon_hook_bridge_executes_script_image_blend_art_node `
  -- --nocapture

cargo test --manifest-path Loom\Cargo.toml -p loom_tool_registry `
  script `
  -- --nocapture

cargo test --manifest-path Loom\Cargo.toml -p loom-daemon `
  script `
  -- --nocapture

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\Loom\scripts\Install-LoomImageBlendScriptArt.ps1
```

Results:

- `loom_tool_registry` direct script image-blend test passed.
- `loom-daemon` Hook Bridge script image-blend Art-node test passed.
- `loom_tool_registry` large-payload script regression passed.
- `loom-daemon` large-payload Hook Bridge script regression passed.
- Hook art-port and art-node factory regression tests passed for the opted-in
  second image port behavior.
- The installer completed successfully and locally installed
  `custom-image-blend-script` into the real Loom control-plane.

## Release output

- Loom standalone package:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260731-script-image-blend-art`
