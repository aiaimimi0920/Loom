# Phase 62: MCP server sync + persistence fix

## Status

Complete.

## Why this phase exists

The MCP `图片搜索` art could already exist inside Loom's tool registry, but the
actual MCP server definition behind its `serverId` was easy to lose:

- Loom daemon kept MCP server configs only in memory, so a restart could clear
  them.
- Hook only synced Arts into Loom, not the legacy ArtNexus
  `mcp_servers.json` definitions those Arts referenced.

That combination produced the runtime error:

- `MCP server '<id>' for tool '<toolId>' was not found or is disabled`

## Implemented

- Added durable MCP server persistence in Loom daemon:
  - persisted file: `<control-plane>/mcp/servers.json`
  - daemon startup now reloads saved MCP servers from disk
  - `PUT /v1/mcp/servers/{id}` and compat save/delete routes now persist
    atomically and roll back in-memory changes on write failure
- Added Hook-side MCP server sync:
  - Hook now loads legacy ArtNexus MCP configs from
    `%APPDATA%\\ArtNexus\\mcp_servers.json`
  - Hook pushes those definitions into Loom via `PUT /v1/mcp/servers/{id}`
    before it syncs local Arts
  - this preserves legacy UUID-based MCP server ids such as the current
    `图片搜索` Brave Search server id

## Verification

Commands run:

```powershell
cargo test -p loom-daemon mcp_server -- --nocapture

$env:CARGO_TARGET_DIR='C:\Users\Public\nas_home\AI\GameEditor\Neuro\Hook\src-tauri\target-test-sync'
cargo test --manifest-path src-tauri/Cargo.toml mock_artloom:: -- --nocapture

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-mcp-server-persistence-sync-fix `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-server-persistence-sync-fix `
  -RunSmoke
```

Results:

- Loom daemon MCP server tests passed, including persistence across runtime
  reloads.
- Hook `mock_artloom` tests passed, including the new HTTP PUT sync test for
  ArtNexus MCP server definitions.
- Packaged Loom release verification passed with:
  - `filesChecked = 32`
  - `smoke = passed`
  - `hookCanvasSmoke = passed`
  - `hookErrorPreviewSmoke = passed`
  - `frameworkArtStoreHookSmoke = passed`

## Release

Generated Loom package:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-server-persistence-sync-fix
```

Generated Hook exe:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260729-mcp-server-persistence-sync-fix\hook.exe
```
