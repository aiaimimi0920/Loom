# Phase 28 Python Source Import Helper Audit

## Scope

Phase 28 restores old ArtLoom's Python source-file import helpers in Loom.

Restored capabilities:

- daemon source read helper equivalent to old `read_python_file`
- daemon arbitrary `art.json` read helper equivalent to old `read_art_json`
- daemon nearby `art.json` detection equivalent to old
  `check_art_json_nearby`
- daemon Python code port inference equivalent to old
  `inferPortsFromPythonCode`
- desktop daemon API helpers for the source import flow
- desktop Registry UI for Python source import
- desktop local inference/mapping helpers for old AddArtModal-style behavior
- release smoke evidence that source read, nearby `art.json`, inference,
  save-as-script-tool, and script execution work in a packaged release

Visible product names remain:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old ArtLoom Python source import sources:

- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\python_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx`

Old `python_engine.rs` exposed:

```text
read_art_json
list_installed_arts
read_python_file
check_art_json_nearby
```

Old `AddArtModal.tsx` used:

```text
list_installed_arts
read_art_json
read_python_file
check_art_json_nearby
inferPortsFromPythonCode
inferTypeFromName
execution.pythonPath
```

Old inference behavior:

- detects inputs from:
  - `args.get("name")`
  - `args["name"]`
- detects outputs from:
  - `return {"name": ...}`
- infers image/path-like names as `image_path`
- infers strength/ratio-like names as numeric
- maps nearby `art.json` signatures and variables into UI ports

## Loom state before Phase 28

Before this phase, Loom already had Phase 24 installed Python Art catalog
parity:

```text
GET /v1/python-arts
GET /v1/python-arts/{artId}
execution.type = "python_art"
Import as Loom tool
packaged python/Arts/Art_LoomEcho
```

Missing relative to old ArtLoom:

- no helper for reading a user-selected Python source file
- no helper for reading an arbitrary Art directory or `art.json` path
- no helper for detecting sibling `art.json` next to a Python file
- no daemon-backed Python source port inference
- no desktop source import panel that can save an inferred Python source as a
  script-backed Loom tool

## Phase 28 implementation

### Contract-first RED

Updated:

```text
scripts/tests/test-loom-artloom-parity-contract.ps1
```

New contract assertions require:

```text
POST /v1/python-arts/source/read
POST /v1/python-arts/source/read-art-json
POST /v1/python-arts/source/check-art-json
POST /v1/python-arts/source/infer-ports
Test-LoomPythonArtSourceImport
pythonArtSourceImport
readPythonArtSource
readPythonArtJson
checkPythonArtJsonNearby
inferPythonArtPorts
Python source import
Check nearby art.json
Infer ports from Python source
Import source as script tool
inferPortsFromPythonCode
mapArtJsonPorts
```

The contract failed before implementation because the desktop source inference
service did not exist:

```text
Missing required path: ...\Loom\apps\desktop\src\services\pythonArtSource.ts
```

The targeted daemon test then failed before implementation with:

```text
HTTP/1.1 404 Not Found
{"error":{"code":"not_found","message":"Loom endpoint was not found"}}
```

### Daemon source helper APIs

Updated:

```text
Loom/apps/daemon/src/lib.rs
```

New daemon help/API entries:

```text
POST /v1/python-arts/source/read
POST /v1/python-arts/source/read-art-json
POST /v1/python-arts/source/check-art-json
POST /v1/python-arts/source/infer-ports
```

Behavior:

- `/source/read`
  - accepts `{ "path": "..." }` or old-style `{ "filePath": "..." }`
  - reads UTF-8 `.py` source
  - enforces a 512 KiB size cap
- `/source/read-art-json`
  - accepts `{ "artPath": "..." }`, `{ "art_path": "..." }`, or
    `{ "path": "..." }`
  - accepts either an Art directory or an `art.json` file
  - parses and returns JSON
  - enforces a 512 KiB size cap
- `/source/check-art-json`
  - accepts `{ "pythonPath": "..." }`, `{ "python_path": "..." }`, or
    `{ "path": "..." }`
  - validates the Python file
  - checks for sibling `art.json`
- `/source/infer-ports`
  - accepts `{ "path": "..." }`, `{ "filePath": "..." }`, or inline
    `{ "code": "..." }`
  - infers ports from `args.get`, `args[]`, and `return {...}` patterns

Safety boundaries:

- APIs remain behind the existing daemon loopback/token guard.
- Source reads require `.py`.
- Art JSON reads require `art.json`.
- Source and JSON reads are size capped.
- Returned canonical Windows paths are normalized away from `\\?\` verbatim
  display form.

Targeted daemon test:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_exposes_python_art_source_import_helpers --offline -- --nocapture --test-threads=1
```

Result:

```text
1 passed
```

### Desktop API and source inference helpers

Updated:

```text
Loom/apps/desktop/src/services/loomApi.ts
Loom/apps/desktop/src/services/pythonArtSource.ts
```

Added daemon helpers:

```text
readPythonArtSource
readPythonArtJson
checkPythonArtJsonNearby
inferPythonArtPorts
```

Added desktop source helpers:

```text
inferPortsFromPythonCode
mapArtJsonPorts
```

`inferPortsFromPythonCode` mirrors the old AddArtModal regex behavior.
`mapArtJsonPorts` maps `signature.inputs`, `signature.outputs`, and
`variables` into Loom tool port definitions.

### Desktop Registry UI

Updated:

```text
Loom/apps/desktop/src/App.tsx
```

The Registry panel now includes:

- `Python source import`
- Python source path input
- `art.json` path / Art directory input
- `Read Python source`
- `Check nearby art.json`
- `Read art.json`
- `Infer ports from Python source`
- source preview
- tool id/name/description fields
- port preview
- `Import source as script tool`

The import action saves a normal Loom registry tool:

```json
{
  "execution": {
    "type": "script",
    "path": "..."
  }
}
```

This matches the current Loom runtime, where `.py` script tools use
`LOOM_PYTHON`, then packaged `bin/python-embed/python.exe`, then development
fallbacks.

## Validation evidence

Local validation passed:

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon daemon_exposes_python_art_source_import_helpers --offline -- --nocapture --test-threads=1
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
git diff --check
rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1
```

The prefix regression scan returned no matches.

## Release evidence

Generated release:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-python-source-import-phase28 -Force
```

Release directory:

```text
release\Loom\loom-python-source-import-phase28
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
packages\Loom-loom-python-source-import-phase28-windows-x64.zip
size = 50095159 bytes
sha256 = 4ca398a3998b46841f3477e70e5caf44282273e2b1435f1fe77c640a9202700b
```

Formal verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-python-source-import-phase28 -Apps Loom
```

Result:

```text
status = passed
gitHead = a5450b89abc47c719cda5088bed3b591c706f28c
gitDirty = false
checksumEntries = 31
```

Release smoke:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-python-source-import-phase28 -Apps Loom
```

Smoke summary:

```text
output\smoke\runs\20260612-221824-Loom-22056-48007d9c7b0e4bd19799666f4255b11f\release-local-apps-loom-python-source-import-phase28-Loom-summary.json
output\smoke\latest\release-local-apps-loom-python-source-import-phase28-Loom-summary.json
```

Smoke source import evidence:

```text
pythonArtSourceImport.nearbyArtJsonFound = true
pythonArtSourceImport.nearbyArtJsonLabel = "Source Import Fixture"
pythonArtSourceImport.inferredInputs = 2
pythonArtSourceImport.inferredOutputs = 2
pythonArtSourceImport.scriptToolExecution = "source import saw release source helper"
```

Smoke also kept prior restored runtime paths green, including:

- packaged `loom-desktop.exe`
- MCP marketplace discovery and connection testing
- management CRUD
- embedded Python
- installed Python Art catalog and execution
- cloud multipart Art node execution
- real OCR
- workflow-backed direct tool execution
- workflow Art node execution
- workflow AHRP execution
- Hook Bridge WebSocket handshake and broadcast

## Non-goals

- This phase does not install arbitrary Python packages.
- This phase does not clone the old AntD AddArtModal UI.
- This phase does not permit non-`.py` source reads through the Python source
  helper.
- This phase does not remove the installed Python Art catalog path restored in
  Phase 24; it adds source import helpers beside it.
