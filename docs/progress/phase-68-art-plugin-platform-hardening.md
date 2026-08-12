# Phase 68: Art plugin platform hardening

## Status

Implementation and clean-source R4 publication are complete. Phase 67's R3
candidate remains immutable historical runtime evidence and was not overwritten
by this phase.

## Objective

Turn the Phase 67 framework/Art package boundary into a public, supportable
plugin platform while preserving `loom.framework.v1` compatibility and the
source immutability invariant:

- a framework or Art author builds and signs an independent package;
- Loom installs, runs, upgrades, rolls back, and removes it from control-plane
  storage;
- Hook consumes generic capability/result metadata only;
- no package operation edits Loom or Hook source.

## Public protocol and SDK

- `crates/loom_protocol` owns the Rust protocol types and embeds the public v1
  Schemas.
- `protocol/README.md` defines the process+JSON ABI, package layouts,
  publisher-qualified IDs, lifecycle, diagnostics, and compatibility rules.
- `protocol/schemas/` contains five language-neutral v1 JSON Schemas.
- `loom-plugin` supports scaffolding, key generation, signing, validation,
  packing, conformance, trust add/revoke, and embedded Schema output.
- Release builds publish an independent
  `Loom-Plugin-SDK-<version>-windows-x64.zip` with exact-payload verification.

## Identity, signing, and trust

- Canonical package identity is `publisher/id`; HTTP path IDs encode the slash
  as `%2F` and reject raw slash/traversal forms.
- Ed25519 package signatures bind the canonical package digest.
- `Verified` means cryptographically valid; `Trusted` additionally means the
  publisher/key is present and not revoked in the local trust store.
- The compatibility default accepts unsigned packages. Production can select
  `require-signed` or `require-trusted`; revoked keys are rejected in every
  policy.
- Install, readiness, execution, and rollback re-evaluate trust and revocation.

## Permissions, credentials, and network

- Framework manifests declare network, filesystem, process, GPU, clipboard,
  credential, and resource requirements.
- The Desktop displays identity, trust, requested permissions, and resource
  limits before install/uninstall.
- The Desktop also manages publisher trust and write-only, framework/Art-scoped
  credentials. Credential values are never returned by the daemon.
- Credential files and token-bearing loopback discovery manifests use
  create-new temporaries, atomic replacement, and owner-only permissions;
  Windows credential values additionally use current-user DPAPI.
- Trust-store replacement is atomic on Unix and Windows rather than deleting
  the previous document before rename.
- `LOOM_PLUGIN_PERMISSION_MODE=audit` is the compatible default.
- `LOOM_PLUGIN_PERMISSION_MODE=strict` fails closed when a package requests
  direct network, arbitrary filesystem, GPU, or clipboard access that the
  current host cannot OS-enforce.
- Doctor output includes the selected mode, per-package findings, and an
  enforcement matrix.
- Secure host HTTP/download paths validate scheme, redirect, domain, DNS/IP,
  loopback/private/special ranges, size, and digest policy.

## Package and lifecycle safety

- ZIP extraction rejects traversal, absolute/rooted/drive paths, symlinks,
  duplicate/case-colliding names, Windows reserved names, excessive expansion,
  and bounded-size violations.
- Framework and Art code is immutable under
  `versions/<version>-<digest-prefix>/`.
- Mutable Art `state/`, `cache/`, and `outputs/` remain outside immutable code.
- Activation pointers, dependency lockfiles, lifecycle journals, and startup
  verification cover install, upgrade, restart, and rollback.
- Uninstall atomically renames the live tree to a same-parent tombstone before
  registry removal. Startup restores a pre-commit tombstone or finishes a
  committed deletion from durable registry state.
- Version retention preserves active and previous versions and bounds retained
  history; rollback rejects tampered or revoked versions.

## Execution and diagnostics

- `loom_process` bounds timeout and stdout/stderr and terminates the child
  process tree on timeout/cancellation/failure. Windows Job Objects additionally
  enforce memory and active-process count; those two declarations are advisory
  on Unix process groups.
- HTTP, ArtLoom compatibility HTTP, Hook Bridge WebSocket, and AHRP execution
  all create durable run evidence.
- Every package-backed Art execution revalidates activation, identity, version,
  digest, lockfile, framework dependency, signature, trust, and revocation
  before process launch.
- Diagnostics/support-bundle APIs expose bounded execution data and redact
  tokens, passwords, authorization/cookies, API keys, credentials, private
  keys, URL userinfo/query/fragment secrets, and oversized values.

## Dependencies and runtimes

- Framework dependencies resolve compatible SemVer candidates with optional
  SHA-256 pins and persist exact runtime lock records.
- Runtime registry startup prunes entries whose immutable directories no longer
  exist.
- Workflow Art child dependencies install dependency-first; cycles and fetched
  identity mismatches are rejected before parent activation.
- Parent Art lockfiles pin each direct child to its publisher-qualified ID,
  exact version, and canonical digest. Readiness, execution, and rollback
  recursively reject missing locks, child upgrades/rollbacks, activation or
  payload tampering, revocation, and uninstall until the exact child state is
  restored or the parent is explicitly reinstalled to refresh its lock.
- Children remain independent immutable Art packages. Reference counting and
  automatic orphan garbage collection are not implemented.

## CI and supply chain

- Clean-host CI covers the Plugin CLI E2E and embedded Schemas.
- A malicious-package matrix covers archive, signature, dependency, network,
  process, and lifecycle cases.
- Formal release builds can require a clean source tree before creating any
  output.
- Release assets include CycloneDX 1.6 and SPDX 2.3 SBOMs, build provenance,
  checksums, the Plugin SDK ZIP/sidecar, and the normal Desktop/CLI artifacts.
- GitHub release workflows request build/SBOM attestations.
- Docker CI uses Buildx provenance/SBOM generation and Trivy HIGH/CRITICAL
  scanning.

## Current verification snapshot

- `loom_tool_registry`: 111 tests passed, including exact child Art locks,
  child upgrade/rollback/tamper/uninstall rejection, publisher-qualified child
  identity, retention, tombstone recovery, revocation, rollback tamper,
  Windows ACL/long-path replacement, and versioned Python runtime coverage.
- `loom-art-store`: 9 tests passed, including persisted and legacy-synthesized
  Art package SHA-256 sidecars.
- `loom-plugin-cli`: 5 tests passed, including real sign/trust/install/
  conformance/revoke E2E.
- `apps/desktop`: 61 tests passed; TypeScript typecheck passed.
- Hook: 801 frontend tests and 144 Rust library tests passed; production
  TypeScript typecheck, static build, Cargo format, and connector contracts
  passed after removing the direct package Art executor.
- `loom-daemon --lib`: 182 tests passed, including atomic owner-only discovery
  manifest replacement, qualified routes,
  redaction/support, durable Hook/AHRP evidence, pre-execution integrity,
  authored Art lifecycle, and permission doctor coverage.
- Full locked workspace check/test, Desktop build/typecheck/tests, standalone
  release, release-tamper, GitHub Actions, SBOM generation, malicious-package,
  and targeted supply-chain contracts passed.
- Clean-source R4 was built and verified at
  `release/Loom/20260801-art-plugin-platform-hardening-r4`; standalone, Hook
  canvas, Hook error preview, Framework Art Store Hook, and Plugin Boundary
  smoke all passed with `gitDirty=false` and `sourceGitDirty=false`.

## Known limits and non-goals

- Windows Job Objects and Unix process groups provide process-tree/resource
  control, not a complete AppContainer, restricted-token, namespace, seccomp,
  or VM sandbox.
- Direct arbitrary plugin network/filesystem access, GPU, and clipboard cannot
  be fully OS-denied in audit mode; strict mode rejects those declarations.
- Unix persistent credential protection is an owner-only local-file fallback,
  not an OS keyring. Its `local-file-base64` protection label is intentionally
  explicit; deployments requiring hardware/OS secret storage should use an
  external credential broker.
- Hosted marketplace operations, payment/licensing, and remote publisher
  governance are not implemented.
- Legacy protocol names remain for compatibility, but Hook no longer owns a
  direct package Art command executor; CLI-backed Arts use Loom's supervised
  AHRP execution path like every other package execution type.

## Final closure checklist

- [x] Public protocol/Schemas and Plugin SDK CLI.
- [x] Publisher namespace, signatures, trust, and revocation.
- [x] Permission audit/strict mode, credentials, and Desktop security UI.
- [x] Secure ZIP/network, immutable versions, lockfiles, rollback, journals,
      tombstones, and startup recovery.
- [x] Process-tree/resource controls, durable evidence, diagnostics, and
      redaction.
- [x] Dependency/runtime registry and workflow child dependency contract.
- [x] Malicious-package CI, SBOM, provenance, and attestation workflows.
- [x] Fresh full Rust/Desktop/PowerShell/malicious-package/release validation.
- [x] Clean commits for Loom and Hook execution-boundary updates.
- [x] Clean-source R4 build and verifier with `gitDirty=false` and
      `sourceGitDirty=false`.

## 2026-08-12 hardening follow-up

The Art-framework completion audit found three gaps not covered by the older R4
snapshot:

- Windows framework processes failed with `ERROR_DIRECTORY` when both the
  executable and working directory lived below a traditional `MAX_PATH`
  boundary. `loom_process` now prepares both launch paths through one shared
  command constructor and uses the existing DOS short path for deep Windows
  paths; a Windows-only regression executes a copied framework runtime from a
  path longer than 260 characters.
- Immutable package resolution preserved `enabled` but dropped mutable
  `artUserSettings`. Credential bindings therefore disappeared immediately
  before framework execution. The resolved immutable Tool now overlays only the
  registered mutable settings metadata, retaining package digest/path integrity
  while restoring scoped credential grants.
- Workflow Art ZIP installation did not register the bundled `workflow.yaml`.
  Direct, bundled-catalog, store, upgrade, rollback, and auto-update activation
  paths now synchronize the active packaged workflow definition. Automatic
  orphan workflow garbage collection remains intentionally out of scope.

Fresh regression evidence includes `loom_process` (5 tests),
`loom_tool_registry` (120 tests), `loom_workflow_runtime` (17 tests), the
framework runtime host (4 tests), and `loom-daemon` (220 tests), plus the six
malicious-package cases and six-package real install/execution smoke. The clean
formal candidate is
`release/Loom/20260812-art-framework-refactor-audit-clean-r10`.
