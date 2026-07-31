# Phase 57: Image-compress runtime preview overlay

## Status

Complete.

## Why this phase exists

After Phase 56, `图片压缩` no longer regressed to the dead legacy Pingo path and
the node executed successfully again. One more runtime mismatch remained:

- the node execution returned a valid compressed image;
- Hook's live workflow snapshot still carried a blank `previewSrc` for the Art
  node; and
- Loom's Hook canvas preview endpoint trusted that blank preview payload, so the
  node rendered as an empty black image even though the execution succeeded.

This phase fixes that by letting successful runtime image outputs override the
blank live preview payload in the daemon's Hook canvas view.

## Implemented

- Extended the daemon's Hook-canvas runtime overlay state to carry:
  - `status`
  - `errorMessage`
  - successful runtime preview image data
- Added a Hook canvas document override hook so the daemon can replace a node's
  preview source with a runtime `data:image/...;base64,...` result and emit a
  fresh preview cache token.
- Updated successful image-output paths to publish runtime preview overlays for:
  - `art_loom/execute_art_node`
  - `art/process`
  - native image AHRP execution
- Cleared runtime preview overlays when an image-producing execution fails or
  returns no usable image output, so stale success previews do not survive a
  later failed run.
- Added a regression test proving that a blank live `previewSrc` is overridden
  by the actual runtime image output after a successful Hook `art/process`
  execution.

## Verification

Commands run:

```powershell
cargo test -p loom-daemon `
  daemon_hook_canvas_overrides_blank_live_art_preview_with_runtime_image_output `
  -- --nocapture

cargo test -p loom-daemon hook_canvas -- --nocapture

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-image-compress-runtime-preview-overlay `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-image-compress-runtime-preview-overlay
```

Results:

- The new regression test passed and proved that a successful runtime image
  output overrides a blank Hook preview payload.
- The daemon Hook canvas test slice passed (`34` tests).
- Parent-scoped release build completed successfully.
- Release verification completed successfully with:
  - `filesChecked = 32`
  - `smoke = not-run`
  - `hookCanvasSmoke = not-run`
  - `hookErrorPreviewSmoke = not-run`
  - `frameworkArtStoreHookSmoke = not-run`

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-image-compress-runtime-preview-overlay
```

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: 685290e934e521d28a49d6b2064538ae29058723b29d3b6228a084e4f5475bfd
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase fixes Loom's daemon-side preview behavior for successful runtime
image outputs. If Hook itself still bakes a blank `previewSrc` into its own live
workflow payload, Hook may still need a separate fix on its side; Loom now no
longer mirrors that blank payload when it already has the real runtime output.
