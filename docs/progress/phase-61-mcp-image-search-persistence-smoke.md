# Phase 61: MCP image-search persistence smoke

## Status

Complete.

## Why this phase exists

Phase 60 made MCP `图片搜索` result selection persist into Loom's live
Hook/session shape, but the proof still stopped at daemon/desktop tests plus an
unpackaged repo smoke. One formal evidence gap remained:

- the fake store smoke still treated MCP image search like a single-result image
  return path; and
- release verification did not yet prove that a packaged Loom build could
  select the second result, clear runtime state, and still reload the same
  preview plus candidate metadata.

This phase closes that gap by upgrading the existing all-framework fake store
Hook smoke into a real packaged persistence proof for MCP image search.

## Implemented

- Extended the fake cloud image fixture to expose two distinct image URLs:
  - `/raw-image.png`
  - `/raw-image-alt.png`
- Extended the fake MCP server fixture so `brave_image_search` can return
  multiple structured image-search results when `count >= 2`.
- Upgraded the fake store `store-mcp-art` manifest to declare:
  - `query`
  - `count`
  - `result_index`
  parameters, mirroring the real `图片搜索` Art contract more closely.
- Updated `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` so it now:
  - writes a real live Hook workflow snapshot for `node-mcp` via
    `art_loom/overwrite_workflow`;
  - executes `store-mcp-art` with `count = 2` and `result_index = 1`;
  - verifies the returned image is the second fixture image;
  - stops Hook Bridge to clear daemon runtime state; and then
  - re-reads `/v1/hook-bridge/canvas` plus the node preview endpoint to prove
    `selectedResultIndex`, `resultCandidates`, `params.result_index`, and the
    persisted preview image all survive the runtime clear.
- Added smoke-summary evidence under `mcpSelectionPersistence` so the fake
  store smoke records:
  - selected result index before clear;
  - selected result index after clear;
  - candidate counts before/after clear; and
  - whether the reloaded preview bytes matched the selected result.

## Verification

Commands run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-mcp-image-search-persistence-smoke `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-image-search-persistence-smoke `
  -RunSmoke
```

Results:

- The upgraded fake store smoke passed and now proves:
  - `selectedResultIndexBeforeClear = 1`
  - `selectedResultIndexAfterClear = 1`
  - `candidateCountBeforeClear = 2`
  - `candidateCountAfterClear = 2`
  - `previewBytesMatchedSelectedResult = true`
- The packaged release build completed successfully.
- Formal release verification completed successfully with:
  - `filesChecked = 32`
  - `smoke = passed`
  - `hookCanvasSmoke = passed`
  - `hookErrorPreviewSmoke = passed`
  - `frameworkArtStoreHookSmoke = passed`

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-image-search-persistence-smoke
```

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: d86c924500c078a7b3760dc2b2c547558896726495e0c29c591221eb9a955bbc
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase upgrades formal smoke coverage for MCP image-search result
selection persistence. It does not yet add:

- a dedicated standalone smoke script only for image-search persistence;
- multi-select or gallery persistence behavior; or
- Hook-native result picking outside Loom's inspector flow.
