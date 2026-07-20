# Loom Single-Entry Release Implementation Plan

> Scope: implement the approved design in
> `docs/superpowers/specs/2026-07-21-loom-single-entry-release-design.md`.

## Goal

Publish a desktop package with one visible `Loom.exe`, a daemon sidecar and
its resources under `runtime/`, and a separate CLI ZIP. Preserve the daemon
process boundary and existing API behavior.

## Baseline

- Repository: `https://github.com/aiaimimi0920/Loom`
- Starting commit: `42d516b40ace382e6c5f1d83a4c8460b628bc395`
- Development branch: `feat/single-entry-release`
- Release output boundary: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`

## Tasks

### 1. Red contracts and unit tests

- Update desktop sidecar tests to require `runtime/loom-daemon.exe` beside
  `Loom.exe` and rename the test descriptions to the new product layout.
- Extend the standalone release contract to require `Loom.exe`,
  `runtime/loom-daemon.exe`, a CLI artifact catalog, and runtime-prefixed
  support paths; reject root-level `loom.exe` and `loom-desktop.exe`.
- Add smoke/release contract assertions for the separate CLI ZIP.
- Run the focused tests and contracts and record the expected failures before
  changing implementation code.

### 2. Desktop runtime resolution

- Change the packaged daemon candidate to `runtime/loom-daemon.exe` beside the
  desktop executable.
- Keep explicit `LOOM_DAEMON_EXECUTABLE` first and development target fallback
  last.
- Preserve loopback URL, process environment, and Windows no-console behavior.
- Run the desktop Rust library tests to reach green.

### 3. Standalone release builder

- Keep source Cargo/Tauri target names unchanged.
- Map the Tauri output to release name `Loom.exe`.
- Map daemon output to `runtime/loom-daemon.exe`.
- Copy OCR, embedded Python, and Python Art resources under `runtime/` so the
  daemon's executable-relative discovery continues to work without duplicate
  resources or production path heuristics.
- Build a separate CLI ZIP containing only `loom.exe`.
- Extend dry-run, manifest, build-info, checksum, and artifact records with
  explicit desktop and CLI roles.
- Keep `-OutputRoot`, `-NoZip`, and `-DryRun` behavior compatible.

### 4. Verifier and smoke suite

- Verify the two desktop executable paths and all runtime support files.
- Verify the desktop ZIP payload independently from the CLI ZIP payload.
- Verify both ZIP sidecar hashes and include both artifacts in checksums.
- Update unified, persistence, Gateway, concurrency, OCR, Python, and CLI
  smokes to resolve the daemon from `runtime/` and the desktop from `Loom.exe`.
- Ensure cleanup checks inspect the runtime daemon path and leave no processes.

### 5. CI, docs, and operator surface

- Update Build Windows and Release Tag workflows to upload/publish the desktop
  ZIP and CLI ZIP with their checksum sidecars.
- Update README, architecture, contributing, progress, and migration docs to
  describe one user entry and separate CLI/server artifacts.
- Add contract checks preventing accidental reintroduction of three root-level
  executables.

### 6. Validation and delivery

- Run formatting, desktop tests, focused Rust tests, all PowerShell contracts,
  and dry-run checks.
- Build a fresh Windows candidate under the mandated release directory.
- Run the release verifier and full smoke matrix, including persistence,
  Gateway, concurrency, real OCR, embedded Python, desktop auto-start, and CLI
  artifact checks.
- Check ZIP contents, manifest/checksum consistency, credentials, and process
  cleanup.
- Commit and push the standalone changes, wait for CI/Build Windows/Docker, then
  update the Neuro parent submodule to the verified standalone commit without
  touching unrelated parent changes.

## Acceptance Criteria

- A user can launch the desktop experience from exactly `Loom.exe`.
- `runtime/loom-daemon.exe` starts automatically and serves the same API.
- No `loom.exe` or `loom-desktop.exe` exists at the desktop package root.
- CLI remains available through its own ZIP and is not lost.
- Both artifacts are independently verifiable and checksummed.
- Existing runtime behavior and parent-project boundaries remain intact.
