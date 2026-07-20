# Phase 34: ArtLoom Compatibility Aliases

## Goal

Run another ArtLoom-vs-Loom source audit after Phase 33 and restore old
ArtLoom command surfaces that were still only behaviorally replaced:

- `art_registry::{list_arts,get_art,enable_art,disable_art,update_art_defaults,sync_user_arts,get_user_arts}`
- `ipc_service::get_ipc_status`
- `python_engine::{python_engine_status,prefetch_shader}`
- `shared_memory::{shm_create_buffer,shm_release_buffer,shm_list_buffers,shm_get_buffer_info}`

The implementation keeps Loom internals canonical and exposes old names only at
compatibility boundaries.

## Implemented

### Daemon

Added safe ArtLoom compatibility HTTP aliases:

- Registry aliases:
  - `GET /v1/artloom-compat/arts`
  - `GET /v1/artloom-compat/user-arts`
  - `GET /v1/artloom-compat/arts/{artId}`
  - `POST /v1/artloom-compat/arts/sync`
  - `POST /v1/artloom-compat/arts/{artId}/enable`
  - `POST /v1/artloom-compat/arts/{artId}/disable`
  - `PUT /v1/artloom-compat/arts/{artId}/defaults`
- IPC status alias:
  - `GET /v1/artloom-compat/ipc/status`
- Python engine aliases:
  - `GET /v1/python-arts/engine/status`
  - `POST /v1/python-arts/shader/prefetch`
- Shared-memory aliases:
  - `POST /v1/shared-memory/buffers`
  - `GET /v1/shared-memory/buffers`
  - `GET /v1/shared-memory/buffers/{handle}`
  - `DELETE /v1/shared-memory/buffers/{handle}`

Behavior:

- Registry aliases read/write through the existing Loom tool registry, but
  ArtLoom user Arts are marked as compat-managed entries so native Loom tools do
  not get cleared by old-style sync.
- `sync_user_arts` supports both mirror mode (`sideEffect=false`, no payload)
  and old ArtLoom payload import mode (`sideEffect=true`, `arts=[...]`). Import
  mode replaces only previously sync-managed ArtLoom Arts, preserves native Loom
  tools, persists the new Arts, and broadcasts `art_loom/arts_updated`.
- `get_ipc_status` wraps the existing Hook Bridge status.
- `python_engine_status` reports packaged Python/launcher/Arts availability.
- `prefetch_shader` uses the existing Python Art launcher path and returns the
  Python Art result under `compatCommand = "prefetch_shader"`.
- `shm_*` aliases are backed by Loom shared image buffers and present old
  `handle_name`, `format = "rgba8"`, and `ref_count = 1` metadata.

### Desktop UI

- Registry page now exposes an `ArtLoom registry compatibility` card with:
  - `list_arts`
  - `get_art`
  - `enable_art`
  - `disable_art`
  - `update_art_defaults`
  - `sync_user_arts`
- Registry page now exposes a `Python engine compatibility` card with:
  - `python_engine_status`
  - `prefetch_shader`
- Hook Bridge page now exposes:
  - `get_ipc_status`
  - `Shared memory compatibility`
  - `shm_create_buffer`
  - `shm_list_buffers`
  - `shm_get_buffer_info`
  - `shm_release_buffer`
- UI keeps the `Loom` product name and existing modern-gradient/glass baseline.

### Release smoke

Added package-level smoke coverage:

- `Test-LoomArtLoomRegistryCompat`
- `Test-LoomArtLoomSharedMemoryCompat`
- `Test-LoomPythonEngineCompat`

The Loom smoke summary now includes:

- `artLoomRegistryCompat`
- `sharedMemoryCompat`
- `pythonEngineCompat`

## Verification before release

Commands run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
npm run typecheck --prefix Loom/apps/desktop
npm run build --prefix Loom/apps/desktop
```

```powershell
cd Loom
cargo fmt --check
cargo test -p loom-daemon -- --test-threads=1
```

Observed results:

- Parity contract passed.
- Desktop TypeScript typecheck passed.
- Desktop Rsbuild build passed.
- Rust format check passed.
- `loom-daemon`: 59 lib tests + 2 CLI contract tests passed with
  `--test-threads=1`.

Browser UI smoke:

- Built desktop UI opened at `http://127.0.0.1:1427/` through a temporary Node
  static server.
- Registry snapshot confirmed:
  - `ArtLoom registry compatibility`
  - `list_arts`
  - `get_art`
  - `enable_art`
  - `disable_art`
  - `update_art_defaults`
  - `sync_user_arts`
  - `Python engine compatibility`
  - `python_engine_status`
  - `prefetch_shader`
- Hook Bridge snapshot confirmed:
  - `get_ipc_status`
  - `Shared memory compatibility`
  - `shared_memory`
  - `shm_create_buffer`
  - `shm_list_buffers`
  - `shm_get_buffer_info`
  - `shm_release_buffer`

## Release

Generated release:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-compat-aliases-phase34
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Zip package:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\loom-compat-aliases-phase34\packages\Loom-loom-compat-aliases-phase34-windows-x64.zip
```

Zip sha256:

```text
bac3e97de75f96c721290d8043d7d058045fd04212a9d592b5e619550b769acd
```

Release smoke:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\runs\20260613-072753-Loom-55548-f3780fb7cf3e4877b721bbf4d48b9724\release-local-apps-loom-compat-aliases-phase34-Loom-summary.json
C:\Users\Public\nas_home\AI\GameEditor\Neuro\output\smoke\latest\release-local-apps-loom-compat-aliases-phase34-Loom-summary.json
```

Release smoke evidence included:

- `artLoomRegistryCompat.listCommand = "list_arts"`
- `artLoomRegistryCompat.getCommand = "get_art"`
- `artLoomRegistryCompat.userArtsCommand = "get_user_arts"`
- `artLoomRegistryCompat.disableCommand = "disable_art"`
- `artLoomRegistryCompat.enableCommand = "enable_art"`
- `artLoomRegistryCompat.defaultsCommand = "update_art_defaults"`
- `artLoomRegistryCompat.syncCommand = "sync_user_arts"`
- `artLoomRegistryCompat.ipcCommand = "get_ipc_status"`
- `pythonEngineCompat.statusCommand = "python_engine_status"`
- `pythonEngineCompat.prefetchCommand = "prefetch_shader"`
- `pythonEngineCompat.available = true`
- `sharedMemoryCompat.createCommand = "shm_create_buffer"`
- `sharedMemoryCompat.listCommand = "shm_list_buffers"`
- `sharedMemoryCompat.infoCommand = "shm_get_buffer_info"`
- `sharedMemoryCompat.releaseCommand = "shm_release_buffer"`
- `sharedMemoryCompat.format = "rgba8"`

Formal release verification:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-compat-aliases-phase34 -Apps Loom
```

Observed result:

```text
status: passed
gitHead: 06a0f733fd99b7100f62c588ad7653725ef9d0e7
gitDirty: false
checksumEntries: 31
```
