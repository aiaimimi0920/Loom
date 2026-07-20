# Phase 31: ArtLoom desktop UX parity

Status: complete

## Trigger

The user rejected the prior Phase 30 completion because the Loom desktop UI did
not visibly restore old ArtLoom workflows:

- Art Node was not obvious.
- Add Art had no old-style multi-route entry point.
- MCP server linking was not visible enough.
- desktop Hook sync was not visible.
- the UI looked too much like a plain admin shell.

## Tasks

- [x] Re-audit old ArtLoom `AddArtModal`, Art registry, workflow `ArtNode`,
      workflow sidebar, MCP settings, and Hook sync surfaces.
- [x] Add a red UI parity contract for missing desktop-visible flows.
- [x] Restore an inline `AddArtWizard` in the Registry page.
- [x] Restore visible Add Art routes:
  - CLI wrapper Art
  - Cloud API Art
  - Script / Python Art
  - MCP-linked Art
  - Installed Python Art
  - Workflow-backed Art
  - Native Image Art
- [x] Save wizard output through existing daemon-backed tool definitions.
- [x] Restore Art Node language in Workflow Studio with:
  - Art node palette
  - Add Art node
  - art-node-card
  - Preview
  - Inputs
  - Outputs
  - Params
  - Result
- [x] Restore manual MCP linking UI with:
  - Manual MCP server
  - Save MCP server
  - Connect MCP server
  - connection test
- [x] Restore visible desktop Hook sync/broadcast UI with:
  - Sync desktop Hook
  - Broadcast hook sync
  - `art_hook/instantiate`
  - `art_loom/update_workflow_node`
- [x] Improve UI contrast and visual hierarchy using the modern-gradient
      industrial terminal baseline while keeping product naming as `Loom`.
- [x] Rebuild and validate the release package.

## Code changes

- `Loom/apps/desktop/src/App.tsx`
  - Added `AddArtWizard`.
  - Added `createArtToolFromWizard`.
  - Added Art Node card/palette UI to Workflow Studio.
  - Added manual MCP server form and save/test actions.
  - Added Hook desktop sync/broadcast card.
- `Loom/apps/desktop/src/styles.css`
  - Added Add Art, Art Node, MCP manual server, Hook sync, and contrast styles.
- `scripts/tests/test-loom-artloom-parity-contract.ps1`
  - Added source contract assertions for the new user-visible UI parity
    requirements.

## Validation

Passed:

```powershell
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-desktop-shell-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
git diff --check
```

Browser evidence:

```text
output\smoke\phase31-ui\loom-phase31-registry-add-art-ui.png
output\smoke\phase31-ui\loom-phase31-workflow-art-node-ui.png
output\smoke\phase31-ui\loom-phase31-hook-ui.png
```

The browser validation was intentionally run with the daemon offline to verify
that the desktop UI still exposes the expected flows before runtime data loads.

## Release

Generated:

```text
release\Loom\loom-artloom-ux-parity-phase31
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
release\Loom\loom-artloom-ux-parity-phase31\packages\Loom-loom-artloom-ux-parity-phase31-windows-x64.zip
sha256: c031351d04e550f360e3e2c7baf58f7c472786803f326071a8af2d28322aea9f
```

Formal verification:

```text
status: passed
gitHead: 4023968714f0eb4ed27f3dfbf1c7e4ad59323203
gitDirty: false
checksumEntries: 31
```

The same formal verification was rerun after release smoke and still passed,
covering the Python Art bytecode/no-`__pycache__` release immutability fix.

Release smoke summary:

```text
output\smoke\runs\20260613-035351-Loom-68028-59d08b8d2e794f5ea6f4c7c517813844\release-local-apps-loom-artloom-ux-parity-phase31-Loom-summary.json
output\smoke\latest\release-local-apps-loom-artloom-ux-parity-phase31-Loom-summary.json
```

## Result

Phase 31 closes the user-visible ArtLoom desktop UX parity gaps found after the
Phase 30 release. Loom now exposes the old Add Art, Art Node, MCP linking, and
Hook sync flows directly in the desktop UI while keeping the package and product
names consistent as `Loom`.
