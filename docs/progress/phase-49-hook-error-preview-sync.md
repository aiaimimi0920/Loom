# Phase 49: Hook failed-art preview sync

## Status

Complete.

## Why this phase exists

After the full framework/store and Hook smoke chain was restored, one more
user-visible Hook sync mismatch remained:

- Hook itself could show an Art node's failed execution preview/reason;
- Loom's Hook canvas view could still show the upstream input image instead.

The user provided a concrete example where the underlying upstream failure was
`402` quota-related and therefore acceptable as a business failure, but Loom
still needed to stay visually synchronized with Hook.

## Root cause

The bug was in Loom's daemon-side Hook preview-source resolution order, not in
framework execution itself.

Current evidence from the real Hook session format showed that Art nodes can
carry:

- their own local `src` image path; and
- no `previewSrc`;

while still having an incoming image link from the upstream screenshot node.

Loom's resolver handled non-screenshot nodes in this order:

1. `previewSrc`
2. upstream connected input image
3. local `src` / `filePath`

That meant a failed Art node with its own local error preview image could be
silently replaced by the upstream input preview before Loom ever considered the
node's own `src`.

## Implemented

- Added a daemon regression test proving a failed Art node with:
  - `status = "error"`
  - a local `src`
  - and a connected upstream input
  must keep its own local preview.
- Changed non-screenshot preview resolution so Loom now prefers:
  1. node-local preview sources (`previewSrc`, `src`, `filePath`)
  2. upstream input fallback only if no node-local preview resolves
- Kept screenshot-node behavior unchanged, because screenshot/sticker preview
  inheritance is intentionally different and already covered by existing tests.

## Verification

Commands run:

```powershell
cargo fmt --all
cargo test -p loom-daemon error_art_preview_prefers_local_src_over_upstream_input -- --nocapture --test-threads=1
cargo test -p loom-daemon hook_canvas -- --nocapture --test-threads=1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-hook-error-preview-sync `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-sync `
  -RunSmoke
```

Results:

- formatter: passed
- targeted regression test: passed
- full daemon Hook canvas test subset: passed (`30` tests)
- parent-scoped release build: passed
- full release smoke chain: passed with
  - `smoke=passed`
  - `hookCanvasSmoke=passed`
  - `frameworkArtStoreHookSmoke=passed`

## Release

Final parent-scoped release for this phase:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-hook-error-preview-sync
```
