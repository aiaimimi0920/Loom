# Phase 58: MCP image-search Art closure

## Status

Complete.

## Why this phase exists

Loom already had generic MCP framework support, install-time framework gating,
daemon routes, and a fake-store smoke. But the MCP proof point stopped at an
`echo` text tool.

That was not enough for the real `图片搜索` Art node:

- Hook expects this node to behave like an image generator/search node.
- Brave Search MCP exposes `brave_image_search`, but its modern MCP result is a
  structured search payload, not a ready-to-preview base64 image.
- Loom's existing MCP success path only recognized direct image content or
  data-URL/base64 text, so `图片搜索` could execute without producing a browser /
  Hook preview image.

This phase closes that gap by teaching Loom to adapt MCP image-search results
into the existing image-preview contract, and by moving the repo-owned
framework smoke from generic MCP text to a real image-search Art.

## Implemented

- Added MCP image-search result normalization inside
  `crates/loom_tool_registry/src/lib.rs`.
  - When a tool declares image output and an MCP response does not already
    contain image content, Loom now:
    - inspects `structuredContent` plus JSON-like text fallbacks;
    - locates the first image candidate URL;
    - downloads that image with Loom-owned HTTP fetching; and
    - re-emits it as standard Loom image content so existing Hook/Loom preview
      code can consume it without protocol changes.
- Kept the existing direct-image MCP path untouched:
  - if MCP already returns `content[].type = "image"` or data-URL/base64 text,
    Loom preserves that result.
- Added targeted regression coverage:
  - `loom_tool_registry` now proves a `brave_image_search`-shaped MCP tool with
    image output resolves a structured image URL into image content.
  - `loom-daemon` now proves Hook `art_loom/execute_art_node` returns
    `output_base64` for a `图片搜索` MCP Art node.
- Updated both repo-owned MCP fixture servers used by tests so they expose:
  - `echo`; and
  - `brave_image_search` with structured image-search output.
- Upgraded `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1`:
  - the fake MCP Art is now `图片搜索`;
  - the fake cloud fixture also serves a raw PNG URL;
  - the fake MCP fixture returns structured `brave_image_search` output
    pointing at that image URL; and
  - the smoke now asserts MCP image output rather than text output.
- Added `scripts/Install-LoomImageSearchArt.ps1`.
  - Packages `custom-image-search` as a formal `mcp` Art zip.
  - Can publish it into `.loom-art-store-data\arts\custom-image-search.zip`.
  - Can optionally configure the `brave-search` MCP server in a running daemon.
  - Installs the `mcp` framework before installing the Art itself.
- Updated `README.md` with:
  - the repo-owned `图片搜索` installer command; and
  - the new image-search-specific MCP smoke description.

## Verification

Commands run:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry `
  execute_mcp_image_search_tool_downloads_structured_image_result -- --nocapture

cargo test --manifest-path Loom/Cargo.toml -p loom-daemon `
  daemon_hook_bridge_executes_mcp_image_search_art_node_image_output -- --nocapture

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageSearchArt.ps1 `
  -SkipInstall `
  -SkipServerConfig

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1
```

Results:

- `loom_tool_registry` MCP image-search regression passed.
- `loom-daemon` Hook Art-node MCP image-search regression passed.
- `Install-LoomImageSearchArt.ps1` successfully generated:
  - `target\art-packages\image-search\custom-image-search.zip`
  - `.loom-art-store-data\arts\custom-image-search.zip`
- The repo-owned fake-store smoke passed end-to-end and wrote evidence under:
  - `target\framework-art-store-hook-smoke\20260730-022929-framework-store-43984-9a993fedf6c1467abb03e607ca5323f8\summary.json`
- That smoke proved:
  - the `mcp` framework installs and reports `ready = true`;
  - `store-mcp-art` is now the `图片搜索` Art;
  - the fake MCP call used `toolName = "brave_image_search"`; and
  - Loom returned `output_base64` for the MCP Art instead of only text.

## Boundaries

This phase adapts the first image-search result into Loom's existing single
image preview contract. It does not yet add:

- multi-result gallery selection;
- a dedicated desktop wizard for configuring Brave credentials; or
- result metadata visualization beyond the existing image-preview path.
