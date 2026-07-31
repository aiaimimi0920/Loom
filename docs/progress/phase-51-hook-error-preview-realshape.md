# Phase 51: Hook failed-art real-shape fixture hardening

## Status

Complete.

## Why this phase exists

Phase 50 added a dedicated packaged smoke for failed Art preview parity, but
the fixture was still simpler than the Hook session shape currently seen in
real user data.

The real session sampled on July 29, 2026 showed an Art node shaped more like:

- `previewSrc` absent
- local absolute-path `src`
- `minified = true`
- `savedRect`
- `cropOffset`
- `params.reference`
- incoming link `output -> input`

This phase hardens the regression coverage around that exact shape so the
release chain stays closer to the live Hook contract.

## Implemented

- Added a new daemon regression test using the realistic Art-node shape and
  asserting that Loom still prefers the Art node's own local failed preview
  instead of the upstream input image.
- Updated `Invoke-LoomHookErrorPreviewSmoke.ps1` so its isolated Hook fixture
  now uses:
  - absolute local image paths
  - `minified = true`
  - `savedRect`
  - `cropOffset`
  - `params.reference = "upstream"`
  - upstream link ports `output -> input`
- Extended `Test-StandaloneReleaseContract.ps1` so the smoke script must keep
  those realistic-shape markers.

## Verification

Commands run:

```powershell
cargo fmt --all
cargo test -p loom-daemon error_art_preview_prefers_realistic_src_only_shape_over_upstream_input -- --nocapture --test-threads=1
cargo test -p loom-daemon hook_canvas -- --nocapture --test-threads=1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomHookErrorPreviewSmoke.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-smoke `
  -EvidenceRoot .\target\runtime-smoke\hook-error-preview
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-hook-error-preview-realshape `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-realshape `
  -RunSmoke
```

Results:

- targeted realistic-shape test: passed
- daemon Hook canvas suite: passed (`31` tests)
- standalone release contract: passed
- dedicated failed-preview smoke: passed
- rebuilt parent release: passed
- full release chain: passed with
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `hookErrorPreviewSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Evidence

- Realistic-shape failed-preview smoke summary:

```text
target\runtime-smoke\hook-error-preview\hook-error-preview-c2c86a53595247f89df625c506956ff4\summary.json
```

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-realshape
```
