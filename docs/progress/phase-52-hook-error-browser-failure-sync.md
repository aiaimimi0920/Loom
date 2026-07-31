# Phase 52: Hook failed-art browser-view failure sync

## Status

Complete.

## Why this phase exists

Phases 49 through 51 restored daemon-side failed-Art preview routing and added
packaged regression coverage for the preview bytes themselves.

However, on July 29, 2026, the user reported one remaining UI inconsistency:

- Hook showed a failed Art node as `执行失败`
- Loom's visual browser view still rendered the incoming image-style preview

That meant the packaged desktop UI could still drift from Hook even when the
daemon was already returning the correct failed-Art preview data.

## Implemented

- Added a desktop presentation helper in
  `apps/desktop/src/services/hookCanvas.ts` so failed Art nodes now prefer an
  explicit execution-failure placeholder over any preview image.
- Added desktop regression tests in
  `apps/desktop/src/services/hookCanvas.test.ts` covering:
  - failed Art nodes surface `执行失败`
  - ready nodes still use the neutral `预览不可用` fallback when preview loading
    really fails
- Updated `apps/desktop/src/components/hook/HookCanvasNode.tsx` so the Hook
  canvas node renderer respects `node.status === "error"` for Art nodes and no
  longer reuses an image preview in that state.
- Added matching failure styling in `apps/desktop/src/styles.css`.
- Extended the packaged Hook canvas UI smoke:
  - `scripts/Invoke-LoomHookCanvasUiSmoke.ps1` now seeds a `failed-art` node in
    the Hook fixture
  - `scripts/Inspect-LoomWebView.mjs` now captures thumbnail/full-canvas node
    presentations and explicitly proves the failed node shows an execution
    failure placeholder with no rendered image
- Tightened `scripts/tests/Test-HookCanvasUiContract.ps1` so the new UI smoke
  and inspector evidence cannot silently regress.

## Verification

Commands run:

```powershell
npm run typecheck --prefix .\apps\desktop
npm test --prefix .\apps\desktop
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-HookCanvasUiContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-hook-error-browser-failure-sync `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomHookCanvasUiSmoke.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-browser-failure-sync `
  -EvidenceRoot .\target\runtime-smoke\hook-canvas
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-browser-failure-sync `
  -RunSmoke
```

Results:

- desktop typecheck: passed
- desktop tests: passed (`49` tests)
- Hook canvas UI contract: passed
- standalone release contract: passed
- packaged Hook canvas UI smoke: passed
- full release verification chain: passed with
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `hookErrorPreviewSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Evidence

- Packaged Hook canvas UI smoke summary:

```text
target\runtime-smoke\hook-canvas\hook-canvas-5550f834a28148a6a1e9f41c29dfd6cc\summary.json
```

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-browser-failure-sync
```
