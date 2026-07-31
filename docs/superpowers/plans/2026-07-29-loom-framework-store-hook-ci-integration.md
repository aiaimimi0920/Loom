# Loom framework/store Hook smoke CI integration plan

## Goal

Integrate the all-framework fake art-store Hook smoke into Loom's existing
formal Windows release verification and CI path without introducing a second
parallel workflow contract.

## Tasks

- [x] Add failing release-contract coverage requiring
  `verify-release.ps1 -RunSmoke` to invoke the framework/store Hook smoke and
  to report its result explicitly.
- [x] Make the new smoke script compatible with the existing standalone smoke
  contract:
  - import `scripts\LoomSmokePorts.ps1`;
  - allocate ports through `Get-LoomSmokePort`;
  - avoid local ad hoc TCP port allocators.
- [x] Extend `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` with package-mode
  execution so `verify-release.ps1` can exercise the packaged
  `runtime\loom-daemon.exe` instead of a source-tree daemon binary.
- [x] Wire `verify-release.ps1 -RunSmoke` to run the new smoke after the
  existing standalone and Hook canvas smokes.
- [x] Validate the new package-mode smoke and rebuild a parent-scoped Loom
  release.
