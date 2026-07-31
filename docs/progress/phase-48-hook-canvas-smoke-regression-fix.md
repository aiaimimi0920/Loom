# Phase 48: Hook canvas smoke regression fix

## Status

Complete.

## Why this phase exists

Phase 47 correctly integrated the all-framework fake store smoke into Loom's
formal release verification path, but it also exposed that the pre-existing
Hook canvas UI smoke had drifted away from the current desktop UI.

The failing packaged smoke showed two concrete regressions:

1. the live Hook thumbnail no longer exposed a stable visual-workflow entry;
2. the advanced technical disclosure no longer exposed the contract-level test
   identifier that the source contract still required.

That drift blocked the full `verify-release.ps1 -RunSmoke` chain before the new
framework/store smoke could even run.

## Root cause

- The live-thumbnail toolbar had been refactored around workflow selection,
  saving, zoom, and minimap controls, but the explicit `打开可视化工作流`
  entrypoint was dropped.
- The WebView inspector still tried to click an older thumbnail action
  selector, so it never reached the full visual canvas.
- The desktop `details` disclosures for advanced technical information kept
  their styling class but no longer carried the explicit smoke target
  `data-testid`.

In other words, the failure was a UI contract regression, not a daemon or
framework/store runtime failure.

## Implemented

- Added a dedicated live-thumbnail button:
  - label: `打开可视化工作流`
  - target: `data-testid="hook-canvas-open-workflow"`
- Wired `HookCanvasThumbnail` back into the existing `hook-live` opening path
  so the Hook sync page can again enter the full visual workflow canvas.
- Updated `Inspect-LoomWebView.mjs` to click the dedicated visual-workflow
  target instead of the removed legacy selector.
- Restored `data-testid="advanced-technical-information"` on the desktop's
  advanced technical disclosures so the source contract matches the rendered
  app again.
- Extended the Hook canvas UI contract so both the thumbnail and inspector now
  require the dedicated visual-workflow target explicitly.

## Verification

Commands run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-HookCanvasUiContract.ps1
npm run typecheck --prefix .\apps\desktop
npm run test --prefix .\apps\desktop
npm run build --prefix .\apps\desktop
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomHookCanvasUiSmoke.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-canvas-smoke-regression-fix `
  -EvidenceRoot .\target\runtime-smoke\hook-canvas
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-canvas-smoke-regression-fix `
  -RunSmoke
```

Results:

- `Test-HookCanvasUiContract.ps1`: passed.
- Desktop `typecheck`: passed.
- Desktop `test`: passed (`47` tests).
- Desktop `build`: passed.
- Packaged `Invoke-LoomHookCanvasUiSmoke.ps1`: passed.
- Full `verify-release.ps1 -RunSmoke`: passed with:
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Evidence

- Direct packaged Hook canvas smoke summary:

```text
target\runtime-smoke\hook-canvas\hook-canvas-41145cc5ad6a47068e9e1fd94620b47c\summary.json
```

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-canvas-smoke-regression-fix
```
