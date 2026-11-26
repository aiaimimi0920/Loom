# Release Provenance

A publishable Loom release must be built from a clean Git worktree and pass all dependency-security, layout, integrity, and smoke gates.

## Internal development candidates vs. formal releases

Internal manual-testing candidates use the same `build-release.ps1` entrypoint without ad-hoc packaging scripts:

```powershell
.\scripts\build-release.ps1 -VersionId internal-YYYYMMDD-test -OutputRoot C:/work/output `
  -DevelopmentOnly -LibAria2BuildRoot C:/work/pinned-libaria2
```

- `-DevelopmentOnly` rebuilds current source (permitting a dirty worktree), requires an `internal-` `VersionId` prefix, cannot be combined with `-RequireCleanSource` or `-PreparedPayloadRoot`, and stamps `formalReleaseApproved=false` in `manifest.json` and provenance outputs.
- On Windows desktop builds, `libaria2` still validates the pinned DLL SHA-256, build receipt, and current `bridge.cpp` digest without altering `releaseApproved` in `security/libaria2-runtime.json`.
- Server editions (`-Edition web` or `-Edition api`) delegate to `scripts/Build-LoomServerEdition.ps1` to produce `<VersionId>-web` or `<VersionId>-api` bundles containing `loom-daemon` and `loom`.

Before building a formal release tag, run the lockfile inventory contract and live OSV vulnerability gate (see `docs/DEPENDENCY_SECURITY.md`):

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-DependencySecurityContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-DependencySecurityScan.ps1
```

Then build and verify from a clean worktree:

```powershell
.\scripts\build-release.ps1 `
  -VersionId Vx.y.z `
  -OutputRoot .\release\Loom `
  -RequireCleanSource

.\scripts\verify-release.ps1 `
  -PackageDir .\release\Loom\Vx.y.z `
  -RunSmoke `
  -RequireCleanSource
```

Formal manifests must record `gitDirty=false`, `sourceGitDirty=false`, and `sourcePaths: ["."]`.

## Package layout and integrity

- **Desktop package (`-Edition desktop`)**: contains exactly one root executable (`Loom.exe`), the sidecar daemon (`runtime/loom-daemon.exe`), runtime support files, and independent ZIP catalogs under `packages/frameworks`, `packages/mcp-servers`, and `packages/arts`. Optional capability models (such as OCR) are never bundled into the default runtime tree.
- **CLI & Plugin SDK ZIPs**: `Loom-CLI-<versionId>-windows-x64.zip` contains only `loom.exe`; `Loom-Plugin-SDK-<versionId>-windows-x64.zip` contains `loom-plugin.exe`, public JSON Schemas, SDK templates, conformance harnesses, and plugin docs.
- **Container & server editions (`-Edition web | api`)**: daemon-first (`loom-daemon` + `loom`), without the Tauri desktop shell. `web` embeds the compiled management UI served under `/ui/`; `api` omits static UI assets.

`verify-release.ps1` binds artifact names, relative paths, byte lengths, and SHA-256 digests to `manifest.json`, checks every `.zip.sha256` sidecar, rejects unexpected root executables, and verifies that `checksums.sha256` covers every release file except itself. Negative tamper coverage lives in `scripts/tests/Test-ReleaseIntegrityTamper.ps1`, `Test-StandaloneReleaseContract.ps1`, and `Test-StandaloneLayout.ps1`.

## Bounded smoke diagnostics and teardown

- **Plugin boundary diagnostics**: failed Windows candidate smoke runs emit a sanitized `plugin-boundary-diagnostic.json` (capped at 4 KiB: phase name, fixed request ID, elapsed ms, socket state, message counts, and exception types/lines without raw payloads, paths, or logs), protected by `Test-PluginBoundaryDiagnostics.ps1`, `Test-PluginBoundarySmokeContract.ps1`, and `Test-PluginBoundaryCleanup.ps1`.
- **MCP smoke diagnostics**: failed PR candidate smoke runs export only `mcp-smoke-diagnostic.json` (max 8 files, 16 KiB/file, 32 records/file, no reparse points or raw logs).
- **Packaged QR projection smoke**: `scripts/Invoke-LoomQrProjectionSmoke.ps1 -PackageDir .\release\Loom\Vx.y.z` verifies Ed25519 invitation confirmation, image updates, retries, restart recovery, unlink, and revocation against the packaged daemon.
- **Smoke process teardown**: `Invoke-LoomFrameworkArtStoreOtterSmoke.ps1` pins parent/child OS process handles (verifying parent PID and creation-time ordering) and enforces a shared monotonic 5-second exit budget, verified by `Test-FrameworkSmokeCleanup.ps1`.

## GitHub Actions attestations, draft verification, and recovery

- `.github/workflows/release-tag.yml` triggers only on canonical uppercase numeric tags (`V[0-9]+.[0-9]+.[0-9]+`, rejecting suffix tags), runs the reusable dependency-security scan on the exact tag ref first, and shares a non-cancelling per-tag concurrency group that refuses pre-existing drafts or published releases.
- Tag builds generate CycloneDX/SPDX SBOMs and `provenance/build-provenance.json`, sign GitHub OIDC attestations (`actions/attest-build-provenance` and `actions/attest-sbom`), and upload public assets (`Loom-<tag>-windows-x64.zip`, `Loom-Plugin-SDK-<tag>-windows-x64.zip`, and their `.sha256` sidecars) into a draft release.
- Trusted repository verification fetches the draft by exact release ID, confirms asset completeness, byte counts, and streaming SHA-256 digests, and only then flips `draft` to `false`.
- `.github/workflows/release-recovery.yml` opens or updates a tracking issue on failed tag runs and permits at most one automatic failed-job rerun strictly for transient infrastructure steps (checkout, toolchain setup, cache, or read-only preflight)—never for security, compile, test, smoke, or digest mismatches.

## Joint release acceptance

Before claiming full cross-product Otter + Loom joint release acceptance, retain commit-bound evidence for clean-source builds in both repositories, strict effective-code-line compliance, OSV dependency scans, packaged `verify-release.ps1 -RunSmoke -RequireCleanSource`, native Otter Surface/Bridge runs, and any applicable [Tile Wall multi-machine checks](TILE_WALL.md). Dirty worktree candidates remain local diagnostic evidence and never substitute for a clean formal release.
