# Surface v1 Acceptance Prototypes

This directory contains three standalone, installable `loom.surface.v1` Art packages:

- `stock-card`: text, buttons, inputs, continuous events, commit events, and streaming price patches.
- `dashboard`: lists, progress indicators, host-brokered image resource updates, and multi-attachment fanout on a single Surface instance.
- `form`: required and optional parameters, validation errors, host-owned high-risk action confirmation, formal result commits, and cancelable execution.

Each package includes its own `manifest.json`, `art.runtime.json`, declarative scene, and PowerShell `process` runtime entry. Runtimes handle only actions declared in their package manifest, and dashboard images are registered through host `resourceUploads` rather than fabricated digests or leases.

Build and validate deterministic ZIPs, `.sha256` sidecars, and `surface-prototypes.catalog.json` with:

```powershell
.\scripts\build-surface-prototypes.ps1
```
