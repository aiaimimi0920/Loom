# Phase 32: Advanced Add Art Editor Parity

## Status

Complete.

## Why this phase exists

The post-Phase 31 ArtLoom re-audit found that the visible Add Art routes were
restored, but the old ArtLoom `AddArtModal` still had advanced editing
capabilities that were only weakly represented in Loom:

- raw CLI command parsing;
- MCP tool schema-to-port import;
- editable input and output ports;
- old output capture modes;
- script shader toggle;
- persisted port metadata after saving an Art definition.

## Implemented

- Added `parseRawCommand` in
  `Loom/apps/desktop/src/services/workflowStudio.ts`.
- Added `portsFromMcpToolSchema` in
  `Loom/apps/desktop/src/services/workflowStudio.ts`, supporting both
  `input_schema` and `inputSchema` MCP tool payloads.
- Extended desktop `AddArtWizard` with:
  - raw CLI import;
  - Cloud API Smart Import inside Add Art;
  - `Discover MCP tools`;
  - `Use MCP tool schema`;
  - `Advanced port editor`;
  - editable `Input ports`;
  - editable `Output ports`;
  - `Capture mode` values `explicit_path`, `fixed_filename`,
    `derived_template`, and `stdout`;
  - script `Shader mode`.
- Changed `createArtToolFromWizard` so saved tool definitions use the advanced
  editor's `inputs`, `outputs`, and script shader metadata.
- Extended `loom_tool_registry::ToolDefinition` with persisted `inputs`,
  `outputs`, and `params` metadata instead of silently dropping desktop fields.
- Added a Rust round-trip test proving advanced Add Art port metadata survives
  deserialize/serialize.
- Extended `scripts/tests/test-loom-artloom-parity-contract.ps1` so future
  regressions catch the old AddArtModal advanced parity surfaces.

## Evidence

Commands run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
npm run typecheck --prefix Loom/apps/desktop
cargo test -p loom_tool_registry
npm run build --prefix Loom/apps/desktop
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -Apps Loom -Smoke -SmokeApps Loom -VersionId loom-advanced-add-art-phase32
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-advanced-add-art-phase32 -Apps Loom
```

Results:

- `Loom ArtLoom parity release contract passed.`
- `tsc --noEmit -p tsconfig.json` exited 0.
- `cargo test -p loom_tool_registry`: 18 passed, 0 failed.
- `rsbuild build` completed and emitted the desktop web bundle.
- Loom release build and release smoke passed for
  `loom-advanced-add-art-phase32`.
- Formal release verification passed with `gitDirty=false`.

Browser/UI evidence:

- Served `Loom/apps/desktop/dist` on `http://127.0.0.1:1424/` with a temporary
  local Python HTTP server.
- Opened the built Loom UI, navigated to Registry, and confirmed:
  - `AddArtWizard`;
  - `Discover MCP tools`;
  - `Use MCP tool schema`;
  - `Advanced port editor`;
  - `Input ports`;
  - `Output ports`;
  - `Capture mode`;
  - `explicit_path`, `fixed_filename`, `derived_template`, `stdout`.
- Screenshot:
  `output/smoke/phase32-ui/loom-phase32-advanced-add-art-editor.png`.
- Temporary HTTP server was stopped and `http://127.0.0.1:1424/` no longer
  responded afterward.

Note: browser console fetch errors during this UI check were expected because
the static desktop shell was opened without a running `loom-daemon`; the goal of
this check was verifying built UI visibility.

## Release

Generated:

```text
release\Loom\loom-advanced-add-art-phase32
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
release\Loom\loom-advanced-add-art-phase32\packages\Loom-loom-advanced-add-art-phase32-windows-x64.zip
sha256: 394f31c87e7b9c2a33dfcd0f3bfa9e4cd8447be345a0db3274234bf61733f46b
```

Formal verification:

```text
status: passed
gitHead: c936ccbd67915444654c032323b3fb0a51d3d87c
gitDirty: false
checksumEntries: 31
```

Release smoke summary:

```text
output\smoke\runs\20260613-045325-Loom-56708-8eb15ee985d0491bb362bca25bdbc40a\release-local-apps-loom-advanced-add-art-phase32-Loom-summary.json
output\smoke\latest\release-local-apps-loom-advanced-add-art-phase32-Loom-summary.json
```
