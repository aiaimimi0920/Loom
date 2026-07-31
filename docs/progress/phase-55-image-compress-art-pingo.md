# Phase 55: Image-compress Art Pingo packaging

## Status

Complete.

## Why this phase exists

`图片压缩` had already been exposed in Loom as a Hook-facing Art node, but its
runtime contract was still fragile:

- the tool pointed at a dead developer-local `pingo.exe` path;
- the binary was not packaged as a Loom-managed installable Art;
- the params did not match the real Pingo CLI surface; and
- there was no repo-owned operator path for rebuilding the tool from the
  official portable package.

This phase closes that gap by turning `图片压缩` into a formal installable
`cli_wrapper` Art backed by the official portable Windows x64 Pingo package,
while keeping the existing production Art id so previously-created Hook/Loom
nodes continue to resolve.

## Implemented

- Added `scripts/Install-LoomImageCompressArt.ps1`.
- The installer now:
  - downloads the official portable package from
    `https://css-ig.net/bin/pingo-win64.zip`;
  - extracts `pingo.exe`;
  - computes the bundled binary SHA-256;
  - builds a formal Loom Art ZIP for `custom-1770146354922`;
  - publishes that ZIP into the local store root
    `.loom-art-store-data\arts\custom-1770146354922.zip`; and
  - installs the Art locally into
    `%APPDATA%\Loom\control-plane\arts\custom-1770146354922`.
- The installed tool definition is rewritten to use the Loom-managed binary path
  under the control-plane root instead of a stale downloads folder.
- The `图片压缩` params were normalized to the real Pingo contract:
  - `level_num`: slider `1..4`
  - `quality_num`: slider `60..100`
  - `lossless`: bool checkbox
- Kept the install flow on `cli_wrapper`, not a custom native/cloud runtime, so
  the Hook/Loom execution path stays aligned with Loom's installable framework
  model.
- Updated `README.md` with the repo-owned rebuild/install command and the
  local-vs-store install note.

## Verification

Commands run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Install-LoomImageCompressArt.ps1 `
  -ForceDownload

& "$env:APPDATA\Loom\control-plane\arts\custom-1770146354922\bin\pingo.exe"

$resp = Invoke-RestMethod -Uri http://127.0.0.1:8765/v1/tools -Method Get
$resp.tools | Where-Object { $_.id -eq "custom-1770146354922" } |
  ConvertTo-Json -Depth 20

# HTTP compat execute path
Invoke-RestMethod `
  -Uri http://127.0.0.1:8765/v1/artloom-compat/ipc/execute-art-node `
  -Method Post `
  -ContentType application/json `
  -Body ...

# Hook Bridge WebSocket AHRP path
ws://127.0.0.1:19820  -> method = "art/process"

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-image-compress-art-pingo `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-image-compress-art-pingo
```

Results:

- Installer successfully re-downloaded the official portable package and
  rebuilt the Art ZIP.
- Installed binary help output reported `pingo v1.28.4`.
- Daemon tool registry now resolves `custom-1770146354922` to:
  `C:\Users\vmjcv\AppData\Roaming\Loom\control-plane\arts\custom-1770146354922\bin\pingo.exe`.
- HTTP compat execution succeeded on a real sample image:
  - input:
    `C:\Users\vmjcv\AppData\Roaming\com.vmjcv.arthook-next\images\082f3a30-d8b1-4687-bc77-f57ebc5545b5.png`
  - input bytes: `399012`
  - output bytes: `179622`
  - saved bytes: `219390`
  - saved ratio: `54.98%`
- Hook Bridge WebSocket `art/process` also succeeded and returned a base64 image
  output.
- Parent-scoped release build completed successfully.
- Release verification completed successfully with:
  - `filesChecked = 32`
  - `smoke = not-run`
  - `hookCanvasSmoke = not-run`
  - `hookErrorPreviewSmoke = not-run`
  - `frameworkArtStoreHookSmoke = not-run`

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-image-compress-art-pingo
```

Release manifest summary:

```text
gitHead: fbd4a50ebc98d985912092116f6fbfa776587531
gitDirty: true
checksumEntries: 32
desktop zip sha256: dccb9d37415fbcd624954a6793133a41de398f837b41bc96608c7c96e2e76c1b
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase packages and operationalizes the existing `图片压缩` Art around the
official portable Pingo binary. It does not change the daemon execution model,
does not remove the daemon's existing request-body limit for large direct ZIP
uploads, and does not yet add a dedicated hosted/public Art marketplace for
binary-backed Arts.
