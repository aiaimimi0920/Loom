# Phase 59: MCP image-search manual flow and multi-result UI

## Status

Complete.

## Why this phase exists

Phase 58 proved that Loom could adapt `brave_image_search`-style MCP results
into a previewable image, but two user-facing gaps remained:

- there was no direct Loom desktop manual flow for saving the Brave MCP server,
  installing the MCP framework, and registering the repo-owned `图片搜索` Art; and
- once multiple search results existed, Loom still behaved like a fixed
  first-image picker.

This phase closes both gaps by wiring `图片搜索` into the desktop testing flow
and surfacing a result-picker UI in the Hook canvas inspector.

## Implemented

- Added desktop-side `mcpImageSearch` helper contracts:
  - `IMAGE_SEARCH_ART_ID = "custom-image-search"`
  - `IMAGE_SEARCH_SERVER_ID = "brave-search"`
  - `buildImageSearchServerConfig(...)`
  - `buildImageSearchArtDefinition(...)`
  - `canExecuteHookCanvasNodeManually(...)`
  - `buildImageSearchExecutionRequest(...)`
- Added a dedicated `图片搜索` quick-start card to Loom's MCP page in:
  - `/C:/Users/Public/nas_home/AI/GameEditor/Neuro/Loom/apps/desktop/src/App.tsx`
  - It now:
    - saves the Brave Search MCP server;
    - installs the `mcp` framework;
    - registers `custom-image-search`;
    - tests the MCP connection; and
    - exposes direct buttons into Workflow Studio and Hook live workflow.
- Extended MCP image-search normalization to preserve candidate metadata:
  - candidate list;
  - selected result index; and
  - selected output image.
- Extended daemon Hook-canvas runtime overlays so selected nodes can carry:
  - `resultCandidates`; and
  - `selectedResultIndex`
  in addition to status/error/preview overlays.
- Extended the Hook canvas inspector UI to:
  - show the currently selected result index;
  - render candidate result cards; and
  - re-execute the current node with a chosen result index.
- Added direct manual execution support for generator-like Hook canvas nodes,
  which is enough to hand-test `图片搜索` without requiring an upstream input
  image.

## Verification

Commands run:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry mcp -- --nocapture

cargo test --manifest-path Loom/Cargo.toml -p loom-daemon mcp -- --nocapture

node --test src/services/mcpImageSearch.test.ts

npm test

npm run typecheck

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1
```

Results:

- `loom_tool_registry` MCP tests passed, including:
  - structured image download; and
  - explicit `result_index` selection plus candidate metadata.
- `loom-daemon` MCP tests passed, including:
  - Hook `execute_art_node` image output for `图片搜索`; and
  - Hook canvas runtime snapshot surfacing `resultCandidates` plus
    `selectedResultIndex`.
- Desktop service tests passed, including new `mcpImageSearch` helper coverage.
- Desktop `npm test` passed.
- Desktop `npm run typecheck` passed.
- Repo-owned all-framework fake-store smoke still passed after the MCP changes.

## Boundaries

This phase adds the desktop manual test path and result selection for the
current single-output image-preview contract. It does not yet add:

- batch downloading multiple selected results at once;
- a separate gallery persistence model; or
- Hook-side native UI for result picking outside Loom's desktop inspector.
