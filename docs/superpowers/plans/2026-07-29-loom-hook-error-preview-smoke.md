# Loom Hook failed-art preview smoke plan

## Goal

Add a dedicated packaged smoke proving that Loom keeps Hook failed-Art preview
parity and does not regress back to showing the upstream input image.

## Tasks

- [x] Add failing release-contract coverage requiring the new Hook
  failed-preview smoke script to be part of the formal release-smoke chain.
- [x] Add a packaged smoke that starts the packaged daemon with an isolated
  Hook fixture containing:
  - an upstream screenshot node;
  - a failed Art node with its own local preview image; and
  - a link between them.
- [x] Assert the failed Art preview endpoint returns the Art node's own local
  preview bytes, not the upstream screenshot bytes.
- [x] Wire the smoke into `verify-release.ps1 -RunSmoke`.
- [x] Re-run the standalone release contract, the dedicated smoke, and the full
  release verification chain.
- [x] Generate a new parent-scoped Loom release.
