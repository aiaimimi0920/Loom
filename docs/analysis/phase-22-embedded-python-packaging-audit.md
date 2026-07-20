# Phase 22 Embedded Python Packaging Audit

## Scope

Phase 22 restores the release-layer embedded Python contract from old ArtLoom:

- packaged Loom includes a local Python embeddable runtime under
  `bin/python-embed`
- packaged Loom includes a Python Art launcher under `python/Launcher.py`
- `.py` script-backed tools prefer the packaged Python executable when
  `LOOM_PYTHON` is not explicitly set
- release smoke proves the actual interpreter by checking `sys.executable`

The visible Loom product names remain unchanged:

- `loom.exe`
- `loom-daemon.exe`
- `loom-desktop.exe`

The old ArtLoom source remains read-only:
`Z:\project\project\ArtNexus\ArtLoom`.

## Old ArtLoom source evidence

Reviewed old runtime and package layout:

- `Z:\project\project\ArtNexus\ArtLoom\bin\python-embed`
- `Z:\project\project\ArtNexus\ArtLoom\python\Launcher.py`
- `Z:\project\project\ArtNexus\ArtLoom\python\Arts`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\python_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\src-tauri\src\mcp_engine.rs`
- `Z:\project\project\ArtNexus\ArtLoom\dist\desktop\ArtLoom`

Old packaged ArtLoom placed Python beside the executable:

```text
dist/desktop/ArtLoom/
  artloom.exe
  bin/python-embed/
  python/
  resources/
```

The old `bin/python-embed` package included:

- `python.exe`
- `pythonw.exe`
- `python3.dll`
- `python312.dll`
- `python312.zip`
- `python312._pth`
- `LICENSE.txt`
- `vcruntime140.dll`
- `vcruntime140_1.dll`
- common DLLs, `.pyd` files, `Lib`, `Scripts`, and `site-packages`

The old dist copy of `bin/python-embed` was large:

```text
Count 6973, Sum 397432103
```

The old dist copy of `python` was small:

```text
Count 12, Sum 204421
```

`python312._pth` contained:

```text
python312.zip
.
site-packages
import site
```

Old `python_engine.rs` behavior:

- locate the base directory from the current executable parent in release mode
- prefer `bin/python-embed/python.exe`
- require `python/Launcher.py`
- fall back to system `python` only when the embedded runtime is missing
- execute `python.exe Launcher.py <request_json>` with current dir set to the
  packaged base directory

Old `mcp_engine.rs` behavior:

- rewrite `python` and `python3` MCP commands to the bundled Python executable
  when `bin/python-embed/python.exe` exists
- fall back to system Python otherwise

Old `Launcher.py` behavior:

- add common `bin/python-embed/site-packages`
- add plugin-specific `python/Arts/<plugin>/site-packages`
- load plugin `main.py`
- accept JSON request through argv or stdin
- support plugin entrypoints `main`, `entry_point`, and `run`
- return JSON with `request_id`, `status`, `data`, and structured error fields

## Loom state before Phase 22

Before Phase 22, Loom had script-backed tool execution, but `.py` tools were
host-dependent:

```rust
let python = std::env::var("LOOM_PYTHON").unwrap_or_else(|_| "python".to_owned());
```

The packaged release did not include:

- `bin/python-embed/python.exe`
- `bin/python-embed/python312.zip`
- `bin/python-embed/python312._pth`
- `python/Launcher.py`

The Phase 15 release smoke label included "Python", but the release proof used
a PowerShell script fixture. It proved generic script execution, script Art
node image output, script AHRP output, and shader text output, but did not prove
that packaged Loom could run Python scripts without host PATH Python.

## Phase 22 implementation design

### Packaged Python resources

Loom now stages a minimal embedded Python runtime under:

```text
Loom/resources/python-embed/
```

The release package copies it to:

```text
bin/python-embed/
```

The staged subset is intentionally minimal to avoid copying the old 397 MB
site-packages tree:

- `python.exe`
- `pythonw.exe`
- `python3.dll`
- `python312.dll`
- `python312.zip`
- `python312._pth`
- `LICENSE.txt`
- `vcruntime140.dll`
- `vcruntime140_1.dll`
- `site-packages/.loom-keep`

This subset was verified with:

```powershell
.\Loom\resources\python-embed\python.exe -c "import sys,json; print(json.dumps({'executable': sys.executable, 'ok': True}))"
```

### Python Art launcher

Loom now stages a Loom-named launcher under:

```text
Loom/resources/python/Launcher.py
```

The launcher keeps the old request/response and layered dependency behavior,
but removes visible old product naming. It is packaged to:

```text
python/Launcher.py
```

The launcher was verified with a temporary plugin through packaged Python. The
response included:

```json
{"request_id":"launcher-smoke-1","status":200,"data":{"echo":"loom launcher","ok":true}}
```

### Runtime Python resolution

`loom_tool_registry` now resolves `.py` scripts in this order:

1. non-empty `LOOM_PYTHON`
2. current executable sibling `bin/python-embed/python.exe`
3. current directory `bin/python-embed/python.exe`
4. development fallback `Loom/resources/python-embed/python.exe`
5. PATH fallback `python`

This preserves explicit operator override while making packaged releases
self-contained by default.

### Release smoke

The Loom release smoke now:

- clears `LOOM_PYTHON` before starting `loom-daemon.exe`
- registers a `.py` script-backed tool
- executes it through `/v1/tools/fixture-python-script/execute`
- asserts the tool output text
- asserts `sys.executable` equals the package-local
  `bin/python-embed/python.exe`
- records `pythonToolExecution` in the smoke summary

The smoke also asserts `python/Launcher.py` exists in the package, preserving
the old launcher resource contract for later Python Art plugin restoration.

## Validation evidence

TDD red checks were observed before implementation:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

failed on missing `bin\python-embed\python.exe`.

```powershell
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry resolve_python_executable --offline -- --nocapture
```

failed because `resolve_python_executable_from` did not exist.

After implementation, targeted validation passed:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry resolve_python_executable --offline -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom_tool_registry script --offline -- --nocapture
cargo test --manifest-path Loom/Cargo.toml -p loom-daemon script --offline -- --nocapture --test-threads=1
cargo check --manifest-path Loom/Cargo.toml -p loom-daemon --offline
cargo fmt --manifest-path Loom/Cargo.toml --all -- --check
```

Release generation and verification passed:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-embedded-python-phase22 -Force
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-embedded-python-phase22 -Apps Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-embedded-python-phase22 -Apps Loom
```

Formal verification reported:

```text
status = passed
gitHead = 58ad84cf6bf028ad3c963ab898052ce827731633
gitDirty = false
checksumEntries = 29
```

Release smoke summary reported:

```text
pythonToolExecution.text = "python saw release embedded python"
pythonToolExecution.pythonExecutable = "...\release\Loom\loom-embedded-python-phase22\bin\python-embed\python.exe"
pythonToolExecution.packagedPython = true
```

## Non-goals

Phase 22 does not complete the full old Python Art plugin/UI restoration.
Remaining layered gaps still include:

- fuller desktop workflow editor/import/interface-inference UI parity
- final full-source audit against old ArtLoom
- optional later restoration of richer Python Art plugin management surfaces
