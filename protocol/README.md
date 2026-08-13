# Loom plugin protocol

This directory is the public, source-independent contract for framework and
Art package authors. Rust types live in `crates/loom_protocol`; the JSON Schema
files in `protocol/schemas` are the language-neutral source of truth. Plugin
authors must not depend on private Loom or Hook source code.

## Compatibility rules

- `loom.framework.v1` is stable. Existing field names and one-request/one-result
  JSON semantics will not change incompatibly.
- `loom.hook.v1` is the standardized Loom<->Hook Art bridge contract. It owns
  connection negotiation, capability discovery, Art execution requests and
  results, Surface attachment/resource operations, and structured failures.
  Loom and Hook negotiate from `protocolVersion` plus
  `supportedProtocolVersions`; package authors must use the brokered contract
  rather than calling Hook or Loom implementation APIs directly.
- Hosts negotiate from `protocolVersion` plus `supportedProtocolVersions`.
  Packages must not infer behavior from the Loom application version.
- `loom.art.execute.v1` and `loom.art.result.v1` describe framework execution.
- `loom.art.runtime.v1` describes a package-local Art runtime entry.
- `loom.surface.v1` describes distributed Art Surface manifests, snapshots,
  patches, typed events, content-addressed resources, previews, and atomic
  formal result commits. It is independent of Hook's frontend framework.
- Unknown optional fields must be ignored. Missing optional fields use the
  secure defaults defined by `loom_protocol`.
- Streaming or persistent workers require a separately named protocol and
  explicit negotiation; they cannot silently change v1 framing.

### Loom<->Hook Art contract and legacy retirement

Phase 71 canonical-only cleanup is the current production baseline. Obsolete
wire aliases, persisted forms, package layouts, provider/process fields, and
app-data identities are rejected rather than discovered or migrated.

The canonical cross-application Art path is `loom.hook.v1` together with the
`loom.surface.v1` Surface payload protocol. Loom owns package installation,
execution, resource brokering, formal-result commits, and device/session
authorization; Hook owns capability-driven rendering and user interaction. The
wire contract does not expose private Rust types, Hook DOM handles, Tauri IPC,
or package source paths.

There are no legacy compatibility inputs on the production Loom<->Hook Art
bridge. Requests and ordinary events use only the exact `loom.hook.*` names
defined by `loom.hook.v1`; Surface pushes use only the exact
`loom.surface.snapshot`, `loom.surface.patch`, `loom.surface.generation`,
`loom.surface.action.ack`, `loom.surface.confirmation.request`,
`loom.surface.action.progress`, `loom.surface.preview`, `loom.surface.result`,
`loom.surface.failure`, `loom.surface.lifecycle`, and `loom.surface.dispose`
names. Unnamespaced names such as `surface` and `surface/snapshot`, ArtLoom/AHRP
routes, old direct per-Art methods, and short Art-ID catalog aliases are not
accepted. Bridge Art identity is publisher-qualified, for example
`neuro.official/surface-device-dashboard`.

Hook translates formal `loom.surface.*` pushes into private `surface/...` Tauri
events for its frontend. Those process-internal UI events are not wire aliases.
Framework and Art storage use only publisher-qualified immutable layouts; the
Art Store exposes exact versioned packages; Hook reads only its current app-data
identity; and framework processes accept only the current ABI fields. Hook's
WGC-to-GDI screenshot fallback is a current capture reliability mechanism, not
a package, persistence, or bridge compatibility path.

## Distributed Art Surface v1

Surface v1 uses a small JSON control protocol and keeps large resources outside
control messages. A Hook client first advertises its Surface API version,
runtime kinds, node catalog, input capabilities, and transports. Loom may then
select a declarative or pre-built JavaScript variant, fall back to the package's
declarative scene, or use Hook's default output view.

The first mount uses a `SurfaceSnapshot`. Later updates use ordered
`SurfacePatch` values with `baseRevision` and `revision`. Scene node IDs are
stable across patches. JavaScript source, arbitrary HTML, Hook DOM handles, and
Tauri IPC are not part of the wire contract.

Surface events carry an instance, attachment, event, request generation, and
base revision. Reliable discrete/commit events use stable event IDs for host
deduplication. Continuous events may be coalesced. Preview commits update only
the Art display. `SurfaceResultCommit` atomically publishes all formal outputs;
ordinary downstream links, save, and export consume only those formal values.

Actions execute under their declared timeout and concurrency policy. A
replace-latest or explicit cancellation changes the request to a terminal
cancelled state before any late runtime response can mutate Surface state.
Explicit cancellation is accepted only from the approved device that owns the
action attachment and only for actions declaring `cancelable: true`. Actions
requiring confirmation remain outside the execution queue until Loom's
host-owned confirmation broker records a device-bound approval; Art code cannot
render or bypass that prompt.

Resources use immutable `sha256:<hex>` IDs. Same-host clients may negotiate
shared memory, while remote clients use an authenticated Loom resource service.
Large Base64 payloads do not belong in snapshots or patches.

Remote Hook devices generate an Ed25519 key pair and submit only the public key
as a pending pairing request. Human approval is required before Loom issues a
one-time challenge. The client signs the canonical
`loom.device-session.v1` challenge envelope and receives a short-lived opaque
session token; Loom stores only its SHA-256 hash. Every authenticated request
also carries a unique nonce. Replayed nonces fail closed, and disabling or
deleting a device immediately invalidates its active sessions. Device sessions
are route-scoped and cannot call Loom administration APIs or impersonate a
different Surface attachment device. Remote deployments expose Loom only
through an authenticated TLS endpoint; direct non-loopback plaintext transport
is not a supported deployment boundary.

Remote Surface pushes use the resumable `/v1/surfaces/stream` cursor protocol.
The bounded replay window reports `reset: true` after cursor loss and includes
current snapshots on initial or reset polls. Local same-host clients may keep
using the shared-memory and loopback WebSocket fast paths.

Package-local declarative entries use
`schemas/surface-scene.v1.schema.json`. They may contain a bare root node or a
document with `protocolVersion`, `scene`, initial authoritative state, and
content-addressed resource descriptors.

Surface instance upgrades are explicit. Installing or activating a newer Art
package never mutates a running instance. A migration request names the exact
target semantic version and package digest. Loom executes the target package's
bounded, package-local JSON merge-patch chain, increments the instance
generation, remounts every live attachment, and keeps an eight-entry checkpoint
history for exact rollback. A missing migration step, stale generation, pending
action, unhealthy target entry, or failed remount rejects the change without
silently switching the instance to the active package.

Snapshot and patch resource references are brokered by Loom. A plugin cannot
invent a content digest, lease ID, expiry, or transport descriptor: every
descriptor must match a verified object in the Surface resource store and every
lease must exactly match a live host-issued lease.

## Canonical identities

Package-local IDs use ASCII letters, digits, `.`, `_`, and `-`. Signed or
publisher-owned packages have the canonical identity `publisher/id`.

- Storage, activation, upgrade, rollback, uninstall, and HTTP routes operate on
  the qualified identity.
- A bare ID is accepted only when exactly one installed publisher owns it.
- An encoded path ID uses `%2F`, for example
  `publisher.example%2Fimage-tool`; raw slashes are not accepted inside one path
  parameter.
- A package from one publisher cannot upgrade or replace another publisher's
  package with the same local ID.

## Normative v1 process ABI

1. Loom resolves the framework entry inside the immutable framework package.
2. Loom starts one process for the execution.
3. Loom writes exactly one UTF-8 JSON request followed by one newline, then
   closes stdin.
4. The process writes exactly one UTF-8 JSON response to stdout. Protocol data
   must not be mixed with banners or logs.
5. Logs and diagnostics belong on stderr.
6. A successful framework-process response uses status `success`; no alternate
   status spelling is accepted.
7. A failed response uses the structured `error.code`, `error.message`, and
   optional `error.detail` fields. A non-zero exit, timeout, invalid JSON, or
   output limit is also a structured host failure.
8. Loom bounds timeout, stdout, stderr, memory, and descendant process count
   from the manifest resource limits. Termination applies to the managed process
   tree, not only the direct child.

The request carries only brokered context: Art/package directories, writable
state/cache/output/temp directories, declared permission metadata, and explicit
credential grants. Host environment secrets are not part of the ABI.

## Framework package ZIP

A framework ZIP contains, at minimum:

```text
framework.manifest.json
<entry.command>
```

The installer rejects absolute paths, traversal, links, duplicate or
case-colliding entries, Windows reserved names and alternate data streams,
excess path length, file-count/expanded-size limits, and suspicious compression
ratios. The process entry must remain inside the package.

Framework versions are installed as:

```text
frameworks/<publisher>/<id>/
  active.json
  lifecycle.json            # present only during an interrupted transition
  versions/<version>-<digest>/
  locks/<package-digest>.json
```

The publisher directory is mandatory. Version directories are read-only after
activation. `active.json` changes atomically. Startup recovers
or quarantines lifecycle journals, verifies the active lockfile, and refuses a
tampered package.

## Art package ZIP

An independently executable Art ZIP contains:

```text
manifest.json               # ToolDefinition
art.runtime.json            # loom.art.runtime.v1
<runtime entry and resources>
```

Installed layout:

```text
arts/<publisher>/<id>/
  active.json
  lifecycle.json
  versions/<version>-<digest>/   # read-only code/resources
  locks/<package-digest>.json
  state/                         # writable
  cache/                         # writable
  outputs/                       # writable
```

Install, upgrade, and rollback verify publisher identity, signature/trust,
canonical digest, framework version/digest, binary hashes, and lockfile schema.
The active package is verified again before daemon execution. Failed activation
restores the prior pointer; code is never written into Loom or Hook source.

## Dependencies and locks

`dependencies` uses semantic-version requirements and optional SHA-256 pins.
The resolver selects the highest compatible candidate that also satisfies the
pin. The lockfile records exact kind, ID, version, and digest. Runtime registry
entries whose directories no longer exist are pruned. Art-to-Art dependencies
are installed dependency-first. Each parent lock records every direct child as
`kind: "art"` with its publisher-qualified ID, exact version, and canonical
digest; the entire child lock graph is revalidated before execution and
rollback. Missing locks, cycles, child upgrades, activation changes, payload
tampering, and uninstall all fail closed until the parent package is explicitly
reinstalled/upgraded to refresh its lock. Children remain separate immutable
packages rather than being copied into the parent Art.

## Signing and trust

Packages use Ed25519 and a canonical SHA-256 digest. Signature metadata is in
the manifest and the detached document defaults to `signature.json`.

Trust states are `unsigned`, `verified`, `trusted`, `invalid`, and `revoked`.
The current development default is `LOOM_PLUGIN_TRUST_POLICY=allow-unsigned`.
Production installations should use `require-trusted`. Revoked keys are always
rejected by `loom-plugin validate --trust-store ...` and are rechecked during
rollback and execution readiness.

## Permissions and credentials

The manifest permission policy declares network, filesystem, process, GPU,
clipboard, and named credential requirements. Loom currently enforces process
tree/resource limits, package path containment, writable-directory separation,
credential scoping, and host-brokered HTTP/download policy. Raw credential
values are encrypted/protected at rest and are never returned by list,
diagnostic, or support-bundle APIs.

Windows Job Objects and Unix process groups are resource/process boundaries;
they are not a complete AppContainer/namespace sandbox. Direct network,
filesystem, GPU, and clipboard access by an arbitrary external executable
cannot be described as fully OS-denied yet. See `docs/plugin-permissions.md` for
the enforcement matrix and production restrictions. Use brokered Cloud API/MCP
paths when hard network mediation is required.

Timeout and stdout/stderr bounds plus process-tree termination apply on Windows
and Unix. Memory and active-process-count limits are enforced by Windows Job
Objects; the current Unix process-group backend reports those declarations as
advisory.

`LOOM_PLUGIN_PERMISSION_MODE=audit` is the current default.
`LOOM_PLUGIN_PERMISSION_MODE=strict` rejects a framework before self-test or Art
execution when it requests those direct, currently unenforceable capabilities.
Doctor output reports the selected mode, enforcement matrix, and per-framework
findings.

## Diagnostics

HTTP and `loom.hook.v1` Art executions create durable `art.execute` run
evidence with started/completed/failed events. Diagnostics and support bundles
redact tokens, passwords, private keys, authorization headers, cookies,
credential values, URL credentials/query/fragment, and oversized strings.

## Schemas and tools

- `schemas/framework-manifest.v1.schema.json`
- `schemas/framework-execute-request.v1.schema.json`
- `schemas/framework-execute-response.v1.schema.json`
- `schemas/framework-authoring.v1.schema.json`
- `schemas/art-runtime.v1.schema.json`

Use the independently released `loom-plugin.exe`:

```text
loom-plugin init framework <DIR> <ID> <PUBLISHER>
loom-plugin init art <DIR> <ID> <FRAMEWORK> <PUBLISHER>
loom-plugin validate <PATH> [--trust-store <STORE>]
loom-plugin pack <SOURCE_DIR> <OUTPUT_ZIP>
loom-plugin conformance <EXE> <FRAMEWORK> <ART_DIR>
loom-plugin keygen <KEY_FILE> <KEY_ID>
loom-plugin sign <PACKAGE_DIR> <KEY_FILE> <PUBLISHER>
loom-plugin trust add <STORE> <PUBLISHER> <KEY_FILE>
loom-plugin trust revoke <STORE> <PUBLISHER> <KEY_ID>
```

The Plugin SDK ZIP contains this CLI, all schemas, and the developer/security
documents. It does not contain Loom or Hook source.
