# Phase 68: Art plugin platform hardening

## Status

Implementation complete; the final workspace/release matrix and clean-source R4
publication are pending. Phase 67's R3 candidate remains immutable historical
runtime evidence and is not overwritten by this phase.

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
- Workflow Art child dependencies install breadth-first with visited/cycle
  suppression. Each child remains an independent immutable Art package.
- V1 does not yet pin child Art versions in the parent lockfile and does not
  maintain dependency reference counts or automatic orphan GC.

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

- `loom_tool_registry`: 104 tests passed, including retention, tombstone
  recovery, revocation, lockfile version, and rollback tamper coverage.
- `loom-plugin-cli`: 5 tests passed, including real sign/trust/install/
  conformance/revoke E2E.
- `apps/desktop`: 61 tests passed; TypeScript typecheck passed.
- `loom-daemon --lib`: 181 tests passed, including qualified routes,
  redaction/support, durable Hook/AHRP evidence, pre-execution integrity,
  authored Art lifecycle, and permission doctor coverage.
- Standalone release, release-tamper, GitHub Actions, SBOM generation, and the
  existing targeted supply-chain contracts passed before final workspace
  closure.

## Known limits and non-goals

- Windows Job Objects and Unix process groups provide process-tree/resource
  control, not a complete AppContainer, restricted-token, namespace, seccomp,
  or VM sandbox.
- Direct arbitrary plugin network/filesystem access, GPU, and clipboard cannot
  be fully OS-denied in audit mode; strict mode rejects those declarations.
- Hosted marketplace operations, payment/licensing, and remote publisher
  governance are not implemented.
- Legacy execution paths remain for compatibility and are not a preferred path
  for new packages.

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
- [ ] Fresh full Rust/Desktop/PowerShell/malicious-package/release validation.
- [ ] Clean commits for Loom and the one-line Hook contract update.
- [ ] Clean-source R4 build and verifier with `gitDirty=false` and
      `sourceGitDirty=false`.
