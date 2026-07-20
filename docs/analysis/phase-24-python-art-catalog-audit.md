# Phase 24 Python Art Catalog Audit

## Scope

Phase 24 restores the old ArtLoom installed Python Art catalog and execution
path at the Loom layer.

Restored capabilities:

- packaged `python/Arts` resources in the Loom release
- daemon discovery API for installed Python Arts
- registry-backed `execution.type = "python_art"`
- launcher-backed Python Art execution through packaged embedded Python
- desktop Registry panel surface for inspecting and importing installed
  Python Arts as Loom tools
- release smoke evidence that the packaged catalog and execution path work in
  the generated release

The visible product names remain unchanged:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old source evidence

Reviewed old ArtLoom Python Art sources:

- `Z:\project\project\ArtNexus\ArtLoom\python\Launcher.py`
- `Z:\project\project\ArtNexus\ArtLoom\python\README.md`
- `Z:\project\project\ArtNexus\ArtLoom\python\Arts\Art_ColorTransfer\art.json`
- `Z:\project\project\ArtNexus\ArtLoom\python\Arts\Art_ColorTransfer\main.py`
- `Z:\project\project\ArtNexus\ArtLoom\python\Arts\Art_Pingo\art.json`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\python_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src\components\AddArtModal.tsx`

Old `python_engine.rs` exposed these relevant Tauri commands:

```text
execute_python_art
python_engine_status
python_process_image
read_art_json
list_installed_arts
read_python_file
check_art_json_nearby
prefetch_shader
```

Old `AddArtModal.tsx` used those commands to:

- load installed Python Arts from `python/Arts`
- select an installed Python Art
- read nearby or selected `art.json`
- map `signature.inputs`, `signature.outputs`, and `variables`
- infer ports from Python source
- create Python Art entries from installed or imported Python code

The important old contract was not only `python/Launcher.py`; it also included
an installed Art catalog under `python/Arts` and a UI path for turning those
installed Arts into user-visible tools.

## Loom state before Phase 24

Before this phase, Loom had restored embedded Python packaging and script-backed
`.py` execution, including:

```text
Loom/resources/python/Launcher.py
Loom/resources/python-embed/
```

Missing relative to old ArtLoom:

- no packaged `python/Arts` catalog
- no daemon `/v1/python-arts` API
- no registry `python_art` execution type
- no release smoke proof that a packaged Python Art can be discovered and run
- no desktop UI surface for installed Python Art import

## Phase 24 implementation

### Packaged Python Art fixture

Added:

```text
Loom/resources/python/Arts/Art_LoomEcho/art.json
Loom/resources/python/Arts/Art_LoomEcho/main.py
```

`art.json` defines:

```json
{
  "art_id": "loom_echo",
  "label": "Loom Echo",
  "execution": {
    "engine": "python",
    "entry": "main.py"
  }
}
```

`main.py` returns text content plus `sys.executable`, so release smoke can prove
that execution used packaged embedded Python instead of a host Python fallback.

Updated:

```text
scripts/build-release-exes.ps1
```

The generated release now includes:

```text
python\Arts\Art_LoomEcho\art.json
python\Arts\Art_LoomEcho\main.py
```

### Registry execution type

Updated:

```text
Loom/crates/loom_tool_registry/src/lib.rs
```

New tool execution variant:

```json
{
  "type": "python_art",
  "artId": "loom_echo",
  "artPath": "<optional installed art directory>"
}
```

The executor resolves:

1. `python/Launcher.py` beside the current executable
2. `python/Launcher.py` under the current working directory
3. development fallback `Loom/resources/python/Launcher.py`

For Art discovery it scans:

1. `python/Arts` beside the current executable
2. `python/Arts` under the current working directory
3. development fallback `Loom/resources/python/Arts`

Execution uses the same packaged Python preference restored in Phase 22:

```text
LOOM_PYTHON
package-local bin\python-embed\python.exe
development embedded Python
PATH python fallback
```

Launcher responses are normalized into the existing Loom tool result content
shape. Existing ArtLoom-compatible output fields are recognized:

- `content`
- `text`
- `output_base64`
- `image_base64`
- `image`
- `output_path`

### Daemon catalog API

Updated:

```text
Loom/apps/daemon/src/lib.rs
```

New daemon routes:

```text
GET /v1/python-arts
GET /v1/python-arts/{artId}
```

The catalog response contains old-like installed Art records:

```json
{
  "arts": [
    {
      "path": "...",
      "art_json_path": "...",
      "art_id": "loom_echo",
      "label": "Loom Echo",
      "description": "...",
      "version": "1.0.0",
      "definition": {}
    }
  ]
}
```

The new endpoint is also covered by the existing non-loopback bearer-token
guard in release smoke.

### Desktop API snapshot

Updated:

```text
Loom/apps/desktop/src/services/loomApi.ts
Loom/apps/desktop/src-tauri/src/lib.rs
```

The desktop snapshot now includes:

```ts
pythonArts: LoomPythonArt[];
```

Browser and Tauri snapshot paths both fetch `/v1/python-arts`.

### Desktop Registry UI

Updated:

```text
Loom/apps/desktop/src/App.tsx
```

The Registry panel now exposes:

- `Python Art Catalog`
- `Installed Python Arts`
- `Refresh Python Arts`
- `Inspect Python Art catalog JSON`
- installed Art cards
- `Import as Loom tool`

Import creates a tool definition with:

```json
{
  "execution": {
    "type": "python_art",
    "artId": "<art id>",
    "artPath": "<installed art path>"
  }
}
```

This is intentionally a Loom-native desktop surface rather than an AntD clone of
old `AddArtModal.tsx`.

### Parity contract

Updated:

```text
scripts/tests/test-loom-artloom-parity-contract.ps1
```

New source-level assertions require:

- packaged `python\Arts\Art_LoomEcho\art.json`
- `/v1/python-arts`
- `Test-LoomPythonArtCatalog`
- `pythonArtCatalog`
- `pythonArtToolExecution`
- `PythonArt`

The initial RED failure was:

```text
Loom release builder must package installed Python Art catalog fixtures. Missing=[python\Arts\Art_LoomEcho\art.json]
```

## Validation evidence

Targeted validation passed:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry python_art --offline -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script --offline -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script --offline -- --nocapture --test-threads=1
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
cargo check --manifest-path Loom/apps/desktop/src-tauri/Cargo.toml --offline
npm --prefix Loom/apps/desktop run typecheck
npm --prefix Loom/apps/desktop run build
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
```

Key results:

```text
Loom ArtLoom parity release contract passed.
loom_tool_registry python_art test: 1 passed.
loom_tool_registry script tests: 3 passed.
loom-daemon script tests: 4 passed.
daemon cargo check finished successfully.
desktop Tauri cargo check finished successfully.
tsc --noEmit passed.
rsbuild build passed.
cargo fmt check passed after formatting.
```

Visible prefix regression check:

```powershell
rg -n "NeuroLoom|Neuro" Loom/apps/desktop/src Loom/apps/desktop/src-tauri/src/lib.rs Loom/resources/python scripts/tests/test-loom-artloom-parity-contract.ps1
```

returned no matches.

## Release evidence

Release generation:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-python-art-catalog-phase24 -Force
```

Generated:

```text
release\Loom\loom-python-art-catalog-phase24
packages\Loom-loom-python-art-catalog-phase24-windows-x64.zip
```

Executable names:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Zip size:

```text
50030234 bytes
```

Formal release verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-python-art-catalog-phase24 -Apps Loom
```

reported:

```text
status = passed
gitHead = 730f3938902cbb620daf91225f31c1b2d2d7ab52
gitDirty = false
checksumEntries = 31
```

Release smoke:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-python-art-catalog-phase24 -Apps Loom
```

passed and wrote:

```text
output\smoke\runs\20260612-194843-Loom-61708-640a2c462a304242a234c8d6525cc545\release-local-apps-loom-python-art-catalog-phase24-Loom-summary.json
output\smoke\latest\release-local-apps-loom-python-art-catalog-phase24-Loom-summary.json
```

Important smoke evidence:

```text
desktopExe = "...release\Loom\loom-python-art-catalog-phase24\loom-desktop.exe"
pythonToolExecution.packagedPython = true
pythonArtCatalog.artId = "loom_echo"
pythonArtCatalog.label = "Loom Echo"
pythonArtCatalog.path = "...release\Loom\loom-python-art-catalog-phase24\python\Arts\Art_LoomEcho"
pythonArtCatalog.count = 1
pythonArtToolExecution = "python art saw release installed python art"
workflowToolExecution = "script saw release workflow runtime"
cloudMultipartArtNode.multipartSeen = true
realOcrImage.fullTextLength = 63
```

The package directory contains:

```text
release\Loom\loom-python-art-catalog-phase24\python\Arts\Art_LoomEcho\art.json
release\Loom\loom-python-art-catalog-phase24\python\Arts\Art_LoomEcho\main.py
```

## Non-goals and remaining gaps

Phase 24 is not the final Loom migration completion point.

Not restored in this phase:

- full Python Art marketplace/install/uninstall manager
- direct desktop Python source editing/import flow from old `AddArtModal.tsx`
- full old visual graph editor
- final full-source audit against old ArtLoom

The next recommended step is the final full-source audit against old ArtLoom.
If that audit finds that old Python Art management surfaces or visual graph
editor behavior are still product-critical, restore them as separate small
phases instead of broad-copying the old frontend.
