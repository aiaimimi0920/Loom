# Phase 50: Hook failed-art preview smoke

## Status

Complete.

## Why this phase exists

Phase 49 fixed the daemon-side preview-source precedence bug for failed Art
nodes, but that protection still only lived in source tests.

Given the user requirement that Loom stay synchronized with Hook on execution
failures, the regression also needed a dedicated packaged smoke in the formal
release chain.

## Implemented

- Added `scripts\Invoke-LoomHookErrorPreviewSmoke.ps1`.
- The smoke:
  - starts the packaged `runtime\loom-daemon.exe`;
  - creates an isolated Hook session with an upstream screenshot node and a
    failed Art node;
  - fetches `/v1/hook-bridge/canvas`;
  - downloads the failed Art node preview; and
  - asserts the preview hash matches the failed Art node's own local preview
    image and differs from the upstream screenshot hash.
- Wired the new smoke into `verify-release.ps1 -RunSmoke`.
- Extended `Test-StandaloneReleaseContract.ps1` so the new smoke is now part
  of the formal release contract and the shared smoke-port helper contract.

## Verification

Commands run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomHookErrorPreviewSmoke.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-sync `
  -EvidenceRoot .\target\runtime-smoke\hook-error-preview
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-hook-error-preview-smoke `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-smoke `
  -RunSmoke
```

Results:

- standalone release contract: passed
- dedicated Hook failed-preview smoke: passed
- rebuilt parent-scoped release: passed
- full release smoke chain: passed with
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `hookErrorPreviewSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Evidence

- Dedicated failed-preview smoke summary:

```text
target\runtime-smoke\hook-error-preview\hook-error-preview-7552a77a4a13406e986f56268ed6ff2d\summary.json
```

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-smoke
```
