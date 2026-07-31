# Phase 56: Image-compress compat sync guard

## Status

Complete.

## Why this phase exists

Phase 55 rebuilt `图片压缩` as a Loom-managed installable Pingo art, but the live
runtime still had one regression path:

- Hook could reconnect and send an old `sync_user_arts` payload;
- that payload reused the same art id `custom-1770146354922`;
- the daemon accepted the legacy compat definition as the new source of truth;
- the tool registry was rewritten back to the old quoted
  `C:\Users\vmjcv\Downloads\pingo-win64\pingo.exe` command; and
- the next art execution failed before Pingo even started.

This phase closes that regression by keeping Loom-local installed compat arts as
the source of truth when a legacy compat sync collides on the same id.

## Implemented

- Extended ArtLoom compat visibility so Loom-local compat arts
  (`metadata.artloomCompat.source = "loom-local"`) are treated as compat-visible
  by:
  - `list_arts`
  - `get_user_arts`
  - `get_enabled_arts`
  - Hook Bridge `list_arts`
  - compat `get_art` / defaults / enable / disable resolution
- Split compat classification into:
  - compat-visible arts: `artloom-compat` and `loom-local`
  - sync-owned arts: only `artloom-compat`
- Changed `sync_user_arts` import so it only deletes/replaces sync-owned compat
  arts, not Loom-local installed compat arts.
- Added an id-collision guard: when Hook sends a legacy compat art whose id
  matches an existing Loom-local installed compat art, the daemon now preserves
  the Loom-local definition instead of overwriting:
  - CLI command
  - outputs contract
  - params contract
  - installed-binary source path
- Repaired the current live control-plane registry by rerunning
  `scripts/Install-LoomImageCompressArt.ps1`, restoring the installed
  `pingo.exe` command path and the fixed params/output contract.

## Verification

Commands run:

```powershell
cargo test -p loom-daemon `
  daemon_hook_bridge_sync_user_arts_preserves_loom_local_compat_art_on_id_collision `
  -- --nocapture

cargo test -p loom-daemon sync_user_arts -- --nocapture

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageCompressArt.ps1

# Live daemon registry check
Invoke-RestMethod -Uri http://127.0.0.1:8765/v1/tools -Method Get

# Live HTTP compat execution
Invoke-RestMethod `
  -Uri http://127.0.0.1:8765/v1/artloom-compat/ipc/execute-art-node `
  -Method Post `
  -ContentType application/json `
  -Body ...

# Live Hook Bridge execution
ws://127.0.0.1:19820  -> method = "art/process"
```

Results:

- New regression test passed and proved that `sync_user_arts` no longer
  overwrites a Loom-local installed compat art on id collision.
- Full daemon `sync_user_arts` test slice passed:
  - `daemon_hook_bridge_sync_user_arts_imports_hook_payload`
  - `daemon_hook_bridge_sync_user_arts_preserves_loom_local_compat_art_on_id_collision`
  - `daemon_sync_user_arts_imports_payload_and_preserves_non_compat_tools`
- The live control-plane registry was restored to:
  `C:\Users\vmjcv\AppData\Roaming\Loom\control-plane\arts\custom-1770146354922\bin\pingo.exe`
- Live HTTP compat execution succeeded again on
  `082f3a30-d8b1-4687-bc77-f57ebc5545b5_preview.png`.
- Live Hook Bridge `art/process` execution also returned `status = "Success"`.

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-image-compress-compat-sync-guard
```

Release verification completed successfully with:

- `filesChecked = 32`
- `smoke = not-run`
- `hookCanvasSmoke = not-run`
- `hookErrorPreviewSmoke = not-run`
- `frameworkArtStoreHookSmoke = not-run`

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: 0c18bac702e8bf858764da5ef61b2da7891932408ce3d91e7e4bb00453cf4d77
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase fixes the Loom-side compat overwrite regression for installed local
arts. It does not change Hook's own external user-art payload source, and it
does not remove the need to ship/run a Loom build that includes this daemon
patch for restart-safe behavior.
