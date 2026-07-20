# Phase 31 ArtLoom desktop UX parity audit

Date: 2026-06-13

## Reason for reopening

The Phase 30 completion claim was too narrow. It proved packaged runtime/API
parity, but the user-visible desktop shell still hid or simplified several old
ArtLoom flows:

- old Art Node visual language was not obvious;
- old Add Art entry points were not exposed as a creation workflow;
- manual MCP server linking was not visible enough;
- desktop Hook sync/broadcast semantics were not visible;
- the light-board UI had low-contrast controls and read as a form-heavy admin
  surface rather than an ArtLoom-style workbench.

Phase 31 treats these as product-critical UX parity gaps.

## Old ArtLoom references sampled

Read-only source:

```text
Z:\project\project\ArtNexus\ArtLoom
```

Key files checked:

```text
src\components\AddArtModal.tsx
src\features\art-registry\index.tsx
src\features\art-registry\components\ArtList.tsx
src\features\art-registry\components\ArtRegistryHeader.tsx
src\features\workflow-editor\components\nodes\ArtNode.tsx
src\features\workflow-editor\components\nodes\ArtNode.css
src\features\workflow-editor\components\Sidebar.tsx
src\pages\settings\MCPSettings.tsx
src\features\mcp\marketplace.ts
```

Observed old UX contracts:

- `AddArtModal` offered five primary execution types:
  `cli_wrapper`, `cloud_api`, `script`, `mcp`, `workflow`.
- The script path included installed Python Art selection, `art.json` nearby
  detection, Python file reads, and port inference.
- Workflow editor used an explicit `ArtNode` visual with preview image/result
  text/status and input/output handles.
- Workflow sidebar exposed a searchable Art node library and drag/add loop.
- MCP settings exposed configured servers, marketplace, manual command/args/env
  editing, install, and connection testing.
- Hook desktop behavior depended on visible bridge/sync/broadcast semantics,
  including `art_hook/instantiate` and `art_loom/update_workflow_node`.

## Loom implementation changes

### Add Art wizard

`Loom/apps/desktop/src/App.tsx` now includes an inline `AddArtWizard` inside the
Registry page. It restores visible old creation routes:

- `CLI wrapper Art`
- `Cloud API Art`
- `Script / Python Art`
- `MCP-linked Art`
- `Installed Python Art`
- `Workflow-backed Art`
- `Native Image Art`

The wizard saves daemon-backed tool definitions through
`createArtToolFromWizard` and `saveToolDefinition`, preserving Loom's current
daemon-first architecture instead of copying the old AntD/Tauri command stack.

### Art Node visual surface

Workflow Studio now exposes:

- `Graph view / Art Node canvas`;
- `Art node palette`;
- visible `Add Art node` action, including an empty-state guide when the daemon
  is offline or the registry is empty;
- explicit `art-node-card` rendering with `Preview`, `Inputs`, `Outputs`,
  `Params`, and `Result` labels.

This restores the old ArtLoom Art Node mental model without reintroducing
ReactFlow as a dependency.

### MCP server linking

The MCP page now exposes a `Manual MCP server` card with:

- server id, name, command, args, env, and description fields;
- `Save MCP server`;
- `Connect MCP server`, which saves and then runs `testMcpConnection`.

The existing configured-server management and MCP Marketplace remain in place.

### Hook desktop sync

The Hook Bridge page now exposes a `Hook desktop sync` card with:

- `Sync desktop Hook`;
- `Broadcast hook sync`;
- visible legacy sync method chips:
  - `art_hook/instantiate`
  - `art_loom/update_workflow_node`
  - `art_loom/workflow_updated`

This makes the old desktop Hook synchronization path discoverable in the Loom
desktop UI.

### Visual polish

`Loom/apps/desktop/src/styles.css` was extended for:

- Add Art mode cards;
- Art Node cards and palette;
- manual MCP card;
- Hook sync card;
- stronger ghost-button/card-kicker contrast on the light main board;
- preserved signal-yellow/cyan industrial terminal emphasis.

The visible product name remains `Loom`; no `NeuroLoom` or `Neuro` prefix was
reintroduced.

## UI evidence

Browser validation used the local frontend dev server at:

```text
http://127.0.0.1:1423/
```

The daemon was intentionally offline during browser visual validation. The
console showed expected `ERR_CONNECTION_REFUSED` entries for
`http://127.0.0.1:8765/*`; this did not block UI visibility checks.

Screenshots saved locally:

```text
output\smoke\phase31-ui\loom-phase31-registry-add-art-ui.png
output\smoke\phase31-ui\loom-phase31-workflow-art-node-ui.png
output\smoke\phase31-ui\loom-phase31-hook-ui.png
```

DOM checks confirmed the Workflow Studio page contains:

```text
Art node palette
Add Art node
Art Node
PREVIEW
```

Browser snapshots also showed:

- Registry page with `AddArtWizard`, all seven Add Art routes, and source import
  helpers;
- MCP page with `Manual MCP server`, `Save MCP server`, `Connect MCP server`,
  configured servers, and Marketplace;
- Hook Bridge page with `Sync desktop Hook`, `Broadcast hook sync`, and legacy
  method chips.

## Automated validation

Commands run after implementation:

```powershell
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
git diff --check
```

All passed.

## Release validation

Generated release:

```text
release\Loom\loom-artloom-ux-parity-phase31
```

Packaged executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Zip package:

```text
release\Loom\loom-artloom-ux-parity-phase31\packages\Loom-loom-artloom-ux-parity-phase31-windows-x64.zip
sha256: c031351d04e550f360e3e2c7baf58f7c472786803f326071a8af2d28322aea9f
```

Formal verify:

```text
status: passed
gitHead: 4023968714f0eb4ed27f3dfbf1c7e4ad59323203
gitDirty: false
checksumEntries: 31
```

Post-smoke formal verify was also rerun and passed. This proves the packaged
Python Art smoke no longer leaves `__pycache__` bytecode artifacts in the
release directory.

Release smoke:

```text
output\smoke\runs\20260613-035351-Loom-68028-59d08b8d2e794f5ea6f4c7c517813844\release-local-apps-loom-artloom-ux-parity-phase31-Loom-summary.json
output\smoke\latest\release-local-apps-loom-artloom-ux-parity-phase31-Loom-summary.json
```

Smoke confirmed the packaged release still includes the prior ArtLoom runtime
parity evidence: MCP marketplace/test, Hook WebSocket handshake/broadcast,
Art node execution, AHRP, native image, shared image, OCR, cloud multipart,
embedded Python, Python Art catalog/source import, workflow execution, and
`loom-desktop.exe`.

## Current conclusion

After Phase 31, the user-visible desktop parity gaps called out after Phase 30
are restored in Loom:

- old Add Art routes are visible and actionable;
- old Art Node concept is visible in Workflow Studio;
- manual MCP server linking is visible and actionable;
- desktop Hook sync/broadcast semantics are visible;
- UI hierarchy and contrast are materially improved while preserving the Loom
  naming requirement.
