# Phase 47: Framework/store Hook smoke release and CI integration

## Status

Complete.

## Why this phase exists

Phase 46 added the all-framework fake art-store Hook smoke as a standalone
repo-owned proof path, but it was not yet part of Loom's existing formal
Windows release verification path. The release and GitHub workflow surfaces
already converged on one narrow integration point: `verify-release.ps1
-RunSmoke`.

This phase wires the new smoke into that path so the Windows release workflow
and tag workflow inherit it automatically.

## Implemented

- Added release-contract coverage requiring `verify-release.ps1 -RunSmoke` to:
  - invoke `Invoke-LoomFrameworkArtStoreHookSmoke.ps1`;
  - pass `-PackageDir`; and
  - report `frameworkArtStoreHookSmoke` in its JSON result.
- Updated `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` so it now:
  - imports `scripts\LoomSmokePorts.ps1`;
  - allocates isolated ports through `Get-LoomSmokePort`;
  - supports package-mode execution through `-PackageDir`; and
  - uses the packaged `runtime\loom-daemon.exe` when running from
    `verify-release.ps1`.
- Extended `verify-release.ps1 -RunSmoke` to run the framework/store Hook
  smoke after the existing standalone release smoke and Hook canvas UI smoke.
- Kept the existing GitHub Actions YAML stable because:
  - `build-windows.yml` already runs `verify-release.ps1 -RunSmoke`;
  - `release-tag.yml` already runs `verify-release.ps1 -RunSmoke`.

## Verification

Commands run:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\tests\Test-GitHubActionsContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-framework-art-store-hook-smoke `
  -Configuration Release `
  -EvidenceRoot .\target\runtime-smoke\framework-art-store-hook
```

Results:

- `Test-StandaloneReleaseContract.ps1`: passed.
- `Test-GitHubActionsContract.ps1`: passed.
- Package-mode `Invoke-LoomFrameworkArtStoreHookSmoke.ps1`: passed and proved
  the packaged daemon can still:
  - install the `python_art` and `mcp` frameworks;
  - install one Art for each framework id from the fake store;
  - instantiate six Hook nodes; and
  - execute all six nodes successfully.

## Current caveat

While validating the full `verify-release.ps1 -RunSmoke` path on
July 29, 2026, the pre-existing Hook canvas UI smoke failed first on the
current release candidate before the new framework/store smoke step was
reached. The failure was in `Invoke-LoomHookCanvasUiSmoke.ps1` waiting for the
desktop Hook canvas revision to advance after opening the visual workflow.

That failure is outside the framework/store smoke integration itself; the new
package-mode framework/store smoke passes independently and is now wired into
the same release-smoke entrypoint that the Windows workflows already execute.

## Evidence

- Package-mode framework/store Hook smoke summary:

```text
target\runtime-smoke\framework-art-store-hook\20260729-131646-framework-store-9960-d7902ac0110442eaba36a7233c6bae38\summary.json
```

## Release

This phase changes the release verification and CI contract, not the packaged
runtime payload layout. A fresh parent-scoped Loom release should therefore be
generated after these script and contract changes so the milestone remains
tracked at the project level.
