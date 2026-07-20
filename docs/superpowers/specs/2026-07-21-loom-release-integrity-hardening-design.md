# Loom Release Integrity Hardening Design

## Goal

Harden the Phase 42 single-entry Windows release contract so that a candidate
cannot pass verification when its artifact metadata, checksum sidecars, ZIP
contents, or desktop-root executable layout has been tampered with.

This phase preserves the approved product boundary:

- the desktop user entry is `Loom.exe`;
- the daemon remains `runtime\loom-daemon.exe`;
- daemon-owned resources remain under `runtime\`;
- the CLI remains only in a separate `Loom-CLI-*.zip`;
- Docker remains daemon-first;
- no main-branch merge or formal release tag is created by this phase.

## Observed Gaps

The Phase 42 candidate passed its verifier and full smoke, but the release
tooling still trusted some relationships instead of checking them directly:

1. The shared layout helper checked that a CLI ZIP existed, but did not itself
   require the ZIP to contain exactly one `loom.exe`.
2. The verifier checked checksum sidecar files as files, but did not parse the
   sidecar line and compare its recorded hash and filename with the ZIP.
3. A stale or extra executable at the desktop package root was not rejected as
   an explicit layout violation when the generic checksum set was made
   internally consistent.
4. The verifier checked only selected manifest-to-artifact fields and did not
   enforce the full CLI artifact relationship (name, relative path, byte count,
   and SHA-256).
5. Existing PowerShell contracts were primarily static source assertions and
   did not prove that tampered candidates fail.

## Design

### Shared layout validation

Extend `scripts/LoomReleaseLayout.ps1` with reusable validation helpers:

- enumerate archive entries without directory entries;
- require the desktop package root to contain exactly one executable named
  `Loom.exe`;
- require the CLI artifact manifest fields to be present and internally
  consistent with its artifact record;
- require the CLI ZIP to contain exactly one entry named `loom.exe`;
- validate the CLI ZIP byte count and SHA-256 against the manifest;
- reject a non-empty/stale extraction destination before expanding a CLI ZIP.

The helper must be safe for both the unified Smoke and direct operator calls.

### Verifier validation

Extend `scripts/verify-release.ps1` to:

- use the shared layout validation;
- reject root-level `loom-desktop.exe`, `loom-daemon.exe`, or any other
  executable besides the exact `Loom.exe`;
- validate desktop and CLI ZIP naming, relative paths, byte counts, and hashes;
- parse both `.sha256` sidecars and require exactly one ASCII line in the
  form `<lowercase sha256>  <zip filename>`;
- compare sidecar content with the actual ZIP hash and artifact name;
- keep existing checksum, payload, manifest, and full smoke validation.

The manifest schema remains version 1. Existing artifact kind names remain
accepted where necessary for historical candidate readability; new output does
not require a schema migration.

### Tamper tests

Add `scripts/tests/Test-ReleaseIntegrityTamper.ps1`. It creates small,
self-contained synthetic candidates and verifies:

- a valid candidate passes;
- a root-level legacy/extra executable fails with the explicit boundary
  error;
- a CLI ZIP containing an extra entry fails;
- a CLI manifest path/name/metadata mismatch fails;
- a desktop or CLI checksum sidecar with an incorrect recorded hash fails;
- the shared layout helper rejects the same malformed CLI ZIP before smoke
  extraction.

The fixture updates its own generic checksum file so each failure proves the
specific integrity check rather than only a checksum-count mismatch.

### CI and documentation

Run the tamper contract in the Windows CI contract step. Update the release
contract documentation and progress master with the Phase 42 final candidate
evidence and Phase 43 hardening scope.

## Non-Goals

- Do not merge the feature branch into `main`.
- Do not create a tag or GitHub Release.
- Do not change daemon HTTP behavior, request scheduling, or cancellation
  semantics.
- Do not add signing, installer, auto-update, or certificate management.
- Do not alter the parent repository's unrelated working-tree changes.

## Acceptance Criteria

- The valid synthetic fixture passes the verifier.
- Every listed tamper case fails for the intended reason.
- The real Phase 42 candidate remains verifiable after the hardening.
- Full local Rust, frontend, PowerShell contract, release verifier, and smoke
  checks remain green.
- The new standalone commit is pushed to the feature branch and hosted CI,
  Windows build, and Docker checks pass for its SHA.
- The Neuro parent changes only its Loom gitlink, and an isolated clone can
  initialize the public Loom submodule at the verified SHA.
