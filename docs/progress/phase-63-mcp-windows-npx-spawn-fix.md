# Phase 63 - MCP Windows npx spawn fix

## Why this phase exists

On Windows, Loom's MCP stdio client previously called `Command::new("npx")`
directly. That does not resolve to `npx.cmd`, so the `图片搜索` MCP Art could
fail before initialization with:

- `failed to start mcp process npx`

The prior Phase 62 work already restored MCP server persistence and Hook ->
Loom server syncing. This phase closes the next runtime blocker: Windows
process launch compatibility for extensionless MCP commands.

## What changed

- Updated `crates/loom_mcp/src/lib.rs` so Windows MCP spawns now:
  - resolve extensionless commands through `PATH` + `PATHEXT`;
  - resolve explicit extensionless paths to sibling `.exe` / `.cmd` / `.bat`
    / `.com` wrappers;
  - wrap `.ps1` commands with `powershell.exe -NoProfile -ExecutionPolicy
    Bypass -File ...`;
  - leave non-Windows spawn behavior unchanged.
- Added a regression test that proves an extensionless Windows `.cmd` MCP
  fixture can now spawn and complete `initialize` + `tools/list`.
- Added focused Windows resolution tests covering bare-command PATH lookup and
  `.ps1` wrapping.

## Verification

- `cargo test -p loom_mcp -- --nocapture`
- `cargo test -p loom-daemon mcp_server -- --nocapture`
- `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-npx-spawn-fix -RunSmoke`
- Packaged daemon live check against the real persisted Brave Search MCP config
  from `%APPDATA%\ArtNexus\mcp_servers.json`:
  - `success = true`
  - `toolCount = 8`
  - `firstTool = brave_web_search`
  - `command = npx`

## Release outputs

- Loom:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-mcp-npx-spawn-fix`
- Hook:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260729-mcp-npx-spawn-fix\hook.exe`
