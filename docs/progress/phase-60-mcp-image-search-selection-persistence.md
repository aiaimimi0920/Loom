# Phase 60: MCP image-search selection persistence

## Status

Complete.

## Why this phase exists

Phase 59 added a multi-result picker for the MCP `图片搜索` Art, but the chosen
result still lived only in Loom's daemon-side runtime overlay:

- clicking a candidate re-executed the node with `result_index`;
- the live Hook canvas preview updated immediately; but
- clearing runtime state dropped the selected result, candidate metadata, and
  preview because the live Hook/session source had never been updated.

This phase closes that gap by persisting the chosen image-search result back
into the live Hook workflow/session shape so Loom reloads the same state after a
refresh or runtime reset.

## Implemented

- Added daemon-side Hook canvas persistence helpers that can patch a live node's:
  - `params.result_index`
  - `loomMetadata.imageSearch`
  - `previewSrc`
  and write the updated live snapshot/session back to disk.
- Extended Hook canvas parsing so persisted live nodes now restore:
  - `resultCandidates` from `loomMetadata.imageSearch.candidates`; and
  - `selectedResultIndex` from `loomMetadata.imageSearch.selectedIndex` or
    fallback `params.result_index`.
- Added a new compat HTTP route:
  - `POST /v1/artloom-compat/ipc/update-workflow-node`
  which mirrors `art_loom/update_workflow_node` and persists live Hook node
  params for `hook-live`.
- Added a desktop `updateArtLoomWorkflowNode(...)` helper and wired the
  Workflow Studio Hook canvas inspector to persist `result_index` before it
  re-executes an MCP image-search node.
- Updated successful MCP image-search execution paths to persist the selected
  candidate metadata plus preview image for both:
  - `art_loom/execute_art_node`; and
  - legacy Hook `art/process`.
- Corrected the daemon test helper so Hook-bridge text fixtures use the runtime's
  real workflow root instead of a fake `.` path.

## Verification

Commands run:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon `
  daemon_hook_canvas_surfaces_mcp_image_search_candidates_and_selection `
  -- --nocapture

cargo test --manifest-path Loom/Cargo.toml -p loom-daemon `
  daemon_artloom_update_workflow_node_route_persists_live_hook_params `
  -- --nocapture

cargo test --manifest-path Loom/Cargo.toml -p loom-daemon -- --nocapture

node --test Loom/apps/desktop/src/services/loomApi.test.ts

npm test

npm run typecheck

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-mcp-image-search-selection-persistence `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-image-search-selection-persistence
```

Results:

- The MCP image-search Hook canvas regression test now proves that after
  runtime state is cleared, Loom still reloads:
  - `params.result_index = 1`
  - `selectedResultIndex = 1`
  - the full candidate list; and
  - the persisted preview image bytes.
- The new compat route regression test proves
  `POST /v1/artloom-compat/ipc/update-workflow-node` persists live Hook node
  params for `hook-live`.
- The full `loom-daemon` suite passed (`158` tests).
- Desktop service tests, full desktop `npm test`, and `npm run typecheck`
  passed.
- The repo-owned all-framework fake-store Hook smoke still passed.
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
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-image-search-selection-persistence
```

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: 11132a060d6ed21fe558facf15d5685e9908aa107be88357850f887e2430b989
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase persists the currently selected MCP image-search result back into
Loom's live Hook/session representation so reloads stay in sync. It does not
yet add:

- batch persistence for multiple simultaneously selected results;
- a separate persisted gallery/history model; or
- native Hook-side result-picker UI outside Loom's desktop inspector.
