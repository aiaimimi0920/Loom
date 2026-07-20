# Phase 28: Python Source Import Helper Parity

## Goal

Restore old ArtLoom Python source-file import helpers in Loom, including daemon
source read, nearby `art.json` detection, port inference, desktop UI, and
packaged release smoke proof.

## Tasks

- [x] P28.1 Source audit and parity boundary
  - Acceptance: source-backed audit identifies old Python source import helper
    behavior, current Loom gaps, and the Phase 28 recovery boundary.
  - Evidence:
    - `docs/loom/analysis/phase-28-python-source-import-audit.md` records old
      `python_engine.rs`, old `AddArtModal.tsx`, current Loom gaps,
      implementation design, validation evidence, release evidence, and
      non-goals.

- [x] P28.2 Contract-first API and UI requirements
  - Acceptance: ArtLoom parity contract and targeted daemon test fail before
    implementation and pass after implementation.
  - Evidence:
    - `scripts/tests/test-loom-artloom-parity-contract.ps1` now asserts:
      - `POST /v1/python-arts/source/read`
      - `POST /v1/python-arts/source/read-art-json`
      - `POST /v1/python-arts/source/check-art-json`
      - `POST /v1/python-arts/source/infer-ports`
      - `Test-LoomPythonArtSourceImport`
      - `pythonArtSourceImport`
      - desktop API helper names
      - desktop UI labels
      - `inferPortsFromPythonCode`
      - `mapArtJsonPorts`
    - Initial RED contract failure:
      missing `Loom/apps/desktop/src/services/pythonArtSource.ts`.
    - Initial RED daemon test failure:
      `HTTP/1.1 404 Not Found` for the new source API.
    - Final contract run passed:
      `Loom ArtLoom parity release contract passed.`

- [x] P28.3 Daemon Python source helper APIs
  - Acceptance: daemon exposes safe helpers equivalent to old
    `read_python_file`, `read_art_json`, `check_art_json_nearby`, and source
    port inference.
  - Evidence:
    - `Loom/apps/daemon/src/lib.rs` exposes:
      - `POST /v1/python-arts/source/read`
      - `POST /v1/python-arts/source/read-art-json`
      - `POST /v1/python-arts/source/check-art-json`
      - `POST /v1/python-arts/source/infer-ports`
    - `.py` source reads and `art.json` reads are size capped at 512 KiB.
    - `daemon_exposes_python_art_source_import_helpers` verifies source reads,
      nearby `art.json`, arbitrary art JSON read, and inferred inputs/outputs.
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_exposes_python_art_source_import_helpers --offline -- --nocapture --test-threads=1`
      passed with 1 test.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline`
      passed.

- [x] P28.4 Desktop source import API and UI
  - Acceptance: desktop Registry panel exposes old source import flow and can
    save the inferred source as a script-backed Loom tool.
  - Evidence:
    - `Loom/apps/desktop/src/services/loomApi.ts` adds:
      - `readPythonArtSource`
      - `readPythonArtJson`
      - `checkPythonArtJsonNearby`
      - `inferPythonArtPorts`
    - `Loom/apps/desktop/src/services/pythonArtSource.ts` adds:
      - `inferPortsFromPythonCode`
      - `mapArtJsonPorts`
    - `Loom/apps/desktop/src/App.tsx` adds:
      - `Python source import`
      - `Read Python source`
      - `Check nearby art.json`
      - `Read art.json`
      - `Infer ports from Python source`
      - `Import source as script tool`
    - `npm --prefix Loom/apps/desktop run typecheck` passed.
    - `npm --prefix Loom/apps/desktop run build` passed.

- [x] P28.5 Release smoke and regression checks
  - Acceptance: release smoke proves packaged Python source helper flow and all
    prior restored runtime paths remain green.
  - Evidence:
    - `scripts/smoke-release-local-apps.ps1` adds
      `Test-LoomPythonArtSourceImport`.
    - Smoke exercises:
      - `POST /v1/python-arts/source/read`
      - `POST /v1/python-arts/source/check-art-json`
      - `POST /v1/python-arts/source/read-art-json`
      - `POST /v1/python-arts/source/infer-ports`
      - save `fixture-python-source-import`
      - execute the imported script-backed tool
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` passed.
    - `git diff --check` passed.
    - `rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1`
      returned no matches.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-python-source-import-phase28 -Force`
      generated `release\Loom\loom-python-source-import-phase28` with
      `loom.exe`, `loom-daemon.exe`, and `loom-desktop.exe`.
    - `packages\Loom-loom-python-source-import-phase28-windows-x64.zip`
      was generated with size `50095159` bytes and sha256
      `4ca398a3998b46841f3477e70e5caf44282273e2b1435f1fe77c640a9202700b`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-python-source-import-phase28 -Apps Loom`
      passed formal verification with
      `gitHead = a5450b89abc47c719cda5088bed3b591c706f28c`,
      `gitDirty = false`, and 31 checksum entries.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-python-source-import-phase28 -Apps Loom`
      passed. The smoke summary is:
      `output\smoke\runs\20260612-221824-Loom-22056-48007d9c7b0e4bd19799666f4255b11f\release-local-apps-loom-python-source-import-phase28-Loom-summary.json`.
    - Smoke evidence includes:
      - `pythonArtSourceImport.nearbyArtJsonFound = true`
      - `pythonArtSourceImport.nearbyArtJsonLabel = "Source Import Fixture"`
      - `pythonArtSourceImport.inferredInputs = 2`
      - `pythonArtSourceImport.inferredOutputs = 2`
      - `pythonArtSourceImport.scriptToolExecution = "source import saw release source helper"`
      - `mcpMarketplace.connectionTestSuccess = true`
      - `managementCrud.workflowDeleted = true`
      - `pythonArtToolExecution = "python art saw release installed python art"`
      - `cloudMultipartArtNode.multipartSeen = true`
      - `realOcrImage.fullTextLength = 63`
      - `workflowToolExecution = "script saw release workflow runtime"`
      - `workflowArtNode.success = true`
      - `workflowAhrpProcess.status = "Success"`

## Notes

- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Phase 28 restores helper behavior, not the old AntD modal.
- The source read helper is intentionally narrower than old direct Tauri file
  reads: `.py` and `art.json` only, size capped, daemon loopback/token guarded.
- Remaining work is a final full-source audit matrix before claiming the Loom
  migration is彻底 complete.
