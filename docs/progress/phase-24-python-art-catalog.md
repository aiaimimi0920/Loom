# Phase 24: Python Art Catalog Parity

## Goal

Restore the old ArtLoom installed Python Art catalog and launcher-backed
execution path in Loom, including a desktop import surface and packaged release
smoke proof, without reintroducing visible `NeuroLoom` or `Neuro` product
prefixes.

## Tasks

- [x] P24.1 Source audit and parity boundary
  - Acceptance: source-backed audit identifies the old Python Art launcher,
    installed Art catalog, Tauri commands, AddArtModal import flow, current
    Loom gaps, and the Phase 24 recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-24-python-art-catalog-audit.md` records old
      `python_engine.rs`, `AddArtModal.tsx`, `python/Launcher.py`,
      `python/Arts/*/art.json`, current Loom gaps, implementation design, and
      non-goals.

- [x] P24.2 Packaged Python Art resources
  - Acceptance: release builder packages a Loom-owned installed Python Art
    fixture under `python/Arts`.
  - Evidence:
    - `Loom/resources/python/Arts/Art_LoomEcho/art.json` defines the
      `loom_echo` Python Art.
    - `Loom/resources/python/Arts/Art_LoomEcho/main.py` returns text content
      and `sys.executable` for smoke verification.
    - `scripts/build-release-exes.ps1` packages
      `python\Arts\Art_LoomEcho\art.json` and
      `python\Arts\Art_LoomEcho\main.py`.

- [x] P24.3 Registry-backed `python_art` execution
  - Acceptance: Loom tools can execute an installed Python Art through
    `python/Launcher.py` and packaged embedded Python.
  - Evidence:
    - `Loom/crates/loom_tool_registry/src/lib.rs` adds
      `ToolExecution::PythonArt`.
    - The executor resolves package-local and development `python/Launcher.py`
      and `python/Arts` directories.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry python_art --offline -- --nocapture`
      passed with 1 test.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script --offline -- --nocapture`
      passed with 3 tests.

- [x] P24.4 Daemon and desktop catalog surfaces
  - Acceptance: daemon exposes installed Python Arts and desktop can import one
    as a Loom tool.
  - Evidence:
    - `Loom/apps/daemon/src/lib.rs` exposes `GET /v1/python-arts` and
      `GET /v1/python-arts/{artId}`.
    - `Loom/apps/desktop/src/services/loomApi.ts` adds
      `LoomPythonArt`, `LoomPythonArtsResponse`, and snapshot
      `pythonArts`.
    - `Loom/apps/desktop/src-tauri/src/lib.rs` fetches `/v1/python-arts`
      into the desktop snapshot.
    - `Loom/apps/desktop/src/App.tsx` adds the Registry panel
      `Python Art Catalog` section and `Import as Loom tool`.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline`
      passed.
    - `cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline`
      passed.
    - `npm --prefix Loom/apps/desktop run typecheck` passed.
    - `npm --prefix Loom/apps/desktop run build` passed.

- [x] P24.5 Contract, release, and smoke
  - Acceptance: parity contract passes; regenerated release contains the
    installed Python Art catalog, keeps all previously restored runtime paths
    green, and proves Python Art execution uses packaged embedded Python.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1`
      passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed
      after applying official formatting.
    - `rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1`
      returned no matches.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-python-art-catalog-phase24 -Force`
      generated `release\Loom\loom-python-art-catalog-phase24` with
      `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`, and packaged
      `python\Arts\Art_LoomEcho`.
    - `packages\Loom-loom-python-art-catalog-phase24-windows-x64.zip`
      was generated with size `50030234` bytes.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-python-art-catalog-phase24 -Apps Loom`
      passed formal verification with
      `gitHead = 730f3938902cbb620daf91225f31c1b2d2d7ab52`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-python-art-catalog-phase24 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-194843-Loom-61708-640a2c462a304242a234c8d6525cc545\release-local-apps-loom-python-art-catalog-phase24-Loom-summary.json`.
    - Smoke evidence includes
      `pythonArtCatalog.artId = "loom_echo"`,
      `pythonArtCatalog.label = "Loom Echo"`,
      `pythonArtCatalog.count = 1`,
      `pythonArtToolExecution = "python art saw release installed python art"`,
      `pythonToolExecution.packagedPython = true`,
      `workflowToolExecution = "script saw release workflow runtime"`,
      `cloudMultipartArtNode.multipartSeen = true`, and
      `realOcrImage.fullTextLength = 63`.

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom:
  `loom.exe`, `loom-daemon.exe`, `loom-desktop.exe`.
- Protocol compatibility names such as `art_loom/*`, `art_hook/*`,
  `art/process`, `shared_memory`, `image_path`, `image_base64`,
  `image_buffer`, and `python_art` remain intentionally supported.
- Phase 24 restores installed catalog discovery, packaged Art resources,
  launcher-backed execution, and desktop import. It does not restore a full
  marketplace/install/uninstall manager, old Python source editing/import
  workflow, or the full old visual graph editor.
- The next recommended step is a final full-source audit against old ArtLoom,
  turning any remaining required omissions into small follow-up phases.
