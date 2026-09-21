# AssetLibrary integration contract

This document defines the target integration between Loom and the independent
AssetLibrary service: catalog reads, authenticated downloads, package
verification, atomic installation, Hook publication, and InstallReceipt
synchronization. It is the implementation contract for the next Loom task; it
does not claim that the adapter is already wired into `loom-daemon` or Desktop.

The authoritative service contract is AssetLibrary OpenAPI `0.2.0` with response
schema version `1.0`. Pin a reviewed AssetLibrary commit or release tag in Loom's
client generation and integration tests rather than following `main` silently.

Authoritative references:

- [AssetLibrary OpenAPI](https://github.com/aiaimimi0920/AssetLibrary/blob/main/contracts/openapi/openapi.yaml)
- [download and Edge contract](https://github.com/aiaimimi0920/AssetLibrary/blob/main/docs/DOWNLOAD_SEARCH_EDGE.md)
- [Loom client and installation contract](https://github.com/aiaimimi0920/AssetLibrary/blob/main/docs/LOOM_CLIENT_AND_PUBLISHER_CLI.md)
- [Hook-side integration guide](https://github.com/aiaimimi0920/Hook/blob/main/docs/ASSET_LIBRARY_INTEGRATION.md)

## Current status and non-goals

AssetLibrary already provides public catalog contracts, restricted download
sessions, install challenges/receipts, and a reusable Rust client under
`crates/loom-client`. Loom currently has a separate local Art-store contract:

- Desktop reads `/v1/arts/store/catalog` from the Loom daemon;
- Desktop requests `/v1/arts/store/install`;
- a local ZIP uses `/v1/arts/install`;
- `LOOM_ART_STORE_URL` selects the existing official/local fixture contract;
- the daemon intentionally rejects arbitrary third-party store URLs.

AssetLibrary is **not** wire-compatible with that existing store. Do not point
`LOOM_ART_STORE_URL` at AssetLibrary, weaken the third-party-store rejection, or
translate AssetLibrary into the old fixture shape inside the renderer. Add a
dedicated AssetLibrary adapter alongside the existing Loom service instead.

This integration excludes:

- publisher upload and moderation UI;
- direct database, Valkey, NATS, OpenSearch, R2, or Cloudflare management access;
- storing user accounts in Loom or AssetLibrary;
- App Update, which remains disabled here and requires the separate TUF and host
  rollback design;
- distribution of Loom framework packages by AssetLibrary (current public kinds
  are only `art`, `capability`, and disabled `app_update`).

## Runtime architecture

```text
Loom Desktop/WebView
  -> authenticated loopback Loom API
    -> AssetLibrary adapter in loom-daemon/native Rust
      -> independent Account Service (opaque bearer)
      -> AssetLibrary metadata API (JSON control plane)
      -> AssetLibrary Edge (ZIP byte plane, direct and resumable)
      -> Loom InstallTarget (control-plane package roots)
    -> active Loom registry
      -> loom.hook.v1 Art definitions
      -> loom.extension.v1 Capability contributions
```

The daemon/native layer is the sole owner of:

- API/Edge network clients and origin policy;
- Account bearer and short-lived Edge ticket;
- resumable partial files and verified cache;
- signature, manifest, compatibility, permission, and revocation checks;
- installation staging, activation, rollback, and crash recovery;
- ephemeral receipt key and pending receipt queue.

The Desktop receives catalog DTOs, sanitized progress/errors, and installed
state. It must never receive a bearer, ticket, raw private key, presigned URL,
object-store key, or unrestricted package filesystem path.

## Configuration contract

The following names are reserved recommendations for the Loom implementation;
they are **not current Loom environment variables** until code and tests add them:

| Setting | Meaning | Production rule |
| --- | --- | --- |
| `LOOM_ASSET_LIBRARY_API_URL` | AssetLibrary control-plane base URL. | HTTPS, no credentials/query/fragment. |
| `LOOM_ASSET_LIBRARY_DOWNLOAD_ORIGINS` | Comma-separated exact Edge origins. | Explicit scheme/host/port allowlist; never `*`. |
| `LOOM_ASSET_LIBRARY_ALLOW_LOOPBACK_HTTP` | Enables exact loopback HTTP for tests. | Absent/false outside development. |
| `LOOM_ASSET_LIBRARY_REQUEST_TIMEOUT_SECONDS` | Metadata request timeout. | Positive and bounded; default 30. |
| `LOOM_ASSET_LIBRARY_STALL_TIMEOUT_SECONDS` | No-progress download timeout. | Positive and bounded; default 30. |
| `LOOM_ASSET_LIBRARY_RETRY_LIMIT` | Retry limit for safe/resumable work. | Default 3, maximum 8. |

Configuration parsing must fail before use when an origin is malformed, embeds
credentials, contains a query/fragment/path, uses an unsupported scheme, or
enables non-loopback HTTP. The returned download URL must still match the exact
allowlist; configuration does not authorize arbitrary redirects.

Do not store the Account bearer in these variables for production. Loom obtains
an opaque bearer from the independent Account Service/session broker and keeps it
in native process memory. AssetLibrary validates the bearer through its external
identity adapter; it is not a login or account database.

## Current local development snapshot

On the shared workstation, verified on 2026-09-05:

| Service | Address | Current use |
| --- | --- | --- |
| AssetLibrary Web | `http://127.0.0.1:3000` | Manual catalog/browser inspection. |
| AssetLibrary API | `http://127.0.0.1:18080` | Healthy metadata API and public catalog. |
| AssetLibrary metrics | `http://127.0.0.1:19090` | Operator telemetry only. |
| Local Edge worker | `http://127.0.0.1:8787` | Started by the P7 test path when needed; not currently permanent. |

These are development addresses, not stable defaults or production endpoints.
The API and Web processes currently run directly on the host; supporting
PostgreSQL, Valkey, NATS, OpenSearch, object storage, ClamAV, ClickHouse, and
telemetry dependencies run in local containers.

Safe public probes:

```powershell
$base = 'http://127.0.0.1:18080'
Invoke-RestMethod "$base/healthz"
Invoke-RestMethod "$base/readyz"
Invoke-RestMethod "$base/v1/public/packages?kind=art&limit=5"
Invoke-RestMethod "$base/v1/public/packages?kind=capability&limit=5"
```

`/healthz` proves process liveness. `/readyz` proves required dependencies are
usable. Neither proves that the separate Edge byte path or Account Service is
available.

## Public catalog contract

Public reads require no bearer:

| Purpose | Request | Response |
| --- | --- | --- |
| List | `GET /v1/public/packages?kind={kind}&publisher={slug}&cursor={cursor}&limit={1..100}` | `PackagePage` |
| Search | `GET /v1/public/search?q={text}&kind={kind}&tag={tag}&cursor={cursor}&limit={1..100}` | `PackagePage` |
| Detail | `GET /v1/public/packages/{slug}` | `Package` |
| Releases | `GET /v1/public/packages/{slug}/releases?cursor={cursor}&limit={1..100}` | `PublishedReleasePage` |
| Publisher | `GET /v1/public/publishers/{slug}` | `PublisherProfile` |

Rules for the adapter:

- require `schema_version == "1.0"` where the schema supplies it;
- use strict deserialization and fail closed on an unsupported shape;
- do not decode, edit, sort, or synthesize an opaque `next_cursor`;
- validate limits, slug/query/tag lengths, UUIDs, digests, sizes, and timestamps
  before data reaches Desktop;
- return an explicit unavailable error for search `503`; never convert it to an
  empty successful page;
- preserve `kind` so the UI and installer cannot confuse Art and Capability;
- use release results for version/compatibility/artifact selection rather than
  guessing a latest version from package summary data.

`Package` includes `id`, `slug`, `name`, `kind`, publisher summary, status, and
optional summary. A public release includes immutable `id`, `version`,
`published_at`, compatibility products/version requirements, permissions, and
verified artifacts. Each artifact includes:

- `artifact_id` and `release_id` UUIDs;
- lowercase 64-character SHA-256 `digest`;
- positive `size_bytes`;
- `media_type` and safe `file_name`;
- `signing_key_id`.

Desktop may render these fields. It must not claim an item is installable until
the daemon evaluates the exact release against the current host profile.

## Managed install sequence

A product install uses the authenticated flow even for a public Art. The public
Art download route exists for anonymous/manual byte access; it does not supply
the full authorization/trust snapshot or installation receipt.

### 1. Select an exact artifact

Resolve the chosen package release and artifact from the public release response.
Carry `package_id`, `release_id`, `artifact_id`, expected kind/version/digest, and
displayed permissions through the local request. The daemon must compare them
with server/challenge and manifest values; UI input is never authoritative.

### 2. Create a restricted download session

```http
POST /v1/me/artifacts/{artifact_id}/download-sessions
Authorization: Bearer <opaque Account Service bearer>
Idempotency-Key: <stable key for this logical mutation>
Content-Type: application/json

{"client_type":"loom"}
```

Expected status is `201` and `Cache-Control` must contain `no-store`. The strict
response contains:

```json
{
  "schema_version": "1.0",
  "session_id": "<uuid>",
  "artifact": {
    "artifact_id": "<uuid>",
    "release_id": "<uuid>",
    "digest": "<lowercase sha256>",
    "size_bytes": 1,
    "media_type": "application/zip",
    "file_name": "package.zip"
  },
  "download_url": "https://<allowlisted-edge>/restricted/...",
  "access_token": "<sensitive short-lived Edge ticket>",
  "expires_at": "<RFC 3339>"
}
```

The Account bearer is sent only to AssetLibrary. The returned `access_token` is
sent only to the exact Edge `download_url` as `Authorization: Bearer`; it never
appears in a query string, log, diagnostic event, UI payload, or disk state.

### 3. Create the install challenge

Generate a new receipt UUID, stable client-instance UUID, and ephemeral Ed25519
key pair. Keep the private key only in memory. Then call:

```http
POST /v1/me/download-sessions/{session_id}/install-challenge
Authorization: Bearer <opaque Account Service bearer>
Idempotency-Key: <stable challenge key>
Content-Type: application/json
```

The body is:

```json
{
  "receipt_id": "<uuid>",
  "client_instance_id": "<uuid>",
  "receipt_public_key_base64": "<ephemeral Ed25519 public key>",
  "host": {
    "loom_version": "<current Loom version>",
    "hook_version": "<current detected Hook semantic version>",
    "platform": "windows-x64",
    "loom_capability_api": {"version":"1.0","features":[]},
    "hook_extension_api": {"version":"1.0","features":[]},
    "surface_api": {"version":"1.0","features":[]},
    "surface_nodes": [],
    "frameworks": [
      {"id":"neuro.official/process","version":"<installed version>","ready":true}
    ]
  }
}
```

The example values are illustrative. The implementation must derive versions,
features, nodes, and frameworks from live registries. Both product version fields
must be valid semantic versions. If the paired Hook version cannot be determined,
fail host-profile construction rather than inventing compatibility. Do not
advertise support merely to pass compatibility. Framework IDs are canonical
`publisher/id`, and `ready` reflects current readiness rather than installation
alone.

Expected status is `201`; `Cache-Control` must contain `private, no-store`. The
challenge binds the exact session, package, artifact, archive digest, currently
trusted signing key, host-profile digest, nonce, client instance, and expiry.

### 4. Download directly from Edge

The API never proxies ZIP bytes. Download from the response URL with:

- exact scheme/host/port allowlist validation before every request;
- redirects disabled;
- HTTPS except an explicitly enabled exact loopback test origin;
- `Accept-Encoding: identity`;
- the Edge ticket in the Authorization header only;
- single-range resume semantics;
- request and no-progress timeouts, bounded buffers/retries, progress, and
  cancellation.

A resumed response must be `206` with the exact requested `Content-Range`, total
size, and selected length. Malformed, multiple, or unsatisfiable ranges are `416`.
Never append an unexpected `200` body to a partial file.

Partial content and its binding state are private. Bind it to the exact session,
artifact, digest, size, and URL. After completion, fsync as appropriate, validate
the final size and raw SHA-256, and promote to a digest-addressed verified cache
without overwriting an existing entry. Reject symlink/reparse traversal and
verify again at the final cache path.

### 5. Verify before activation

Before calling Loom's install target, verify all of:

- session, challenge, Release, Artifact, client-instance, and digest binding;
- challenge is still unexpired;
- raw archive SHA-256 and canonical ZIP SHA-256;
- trusted publisher-key fingerprint and Ed25519 signature;
- strict Art or Capability manifest shape;
- publisher/package identity, kind, version, permissions, and entrypoint;
- platform, Loom/Hook API versions, required features, Surface nodes, and
  framework readiness.

Do not weaken validation based on package popularity, official-looking names, a
previous install, or renderer input. App Update packages are rejected by this
client path.

### 6. Install atomically into Loom-owned roots

Map AssetLibrary's verified package into the existing Loom control plane:

```text
<control-plane>/arts/<publisher>/<id>/...
<control-plane>/capabilities/<publisher>/<id>/...
```

Installation must use the existing immutable-version, active-pointer, registry,
lock, lifecycle, and recovery model. It must never write into the Loom or Hook
source repository.

The AssetLibrary client calls a Loom-owned `InstallTarget` transaction:

```text
prepare(package)
  -> commit(transaction)
       -> finalize(transaction)       # success
       -> rollback(transaction)       # commit/check/finalize failure
  -> rollback(transaction)            # cancellation after prepare
```

Required semantics:

- `prepare` securely extracts immutable staged content and does not change the
  active version;
- `commit` atomically activates the new version while retaining the old version;
- post-commit checks prove registry/runtime behavior and that the Hook protocol
  projection can be generated and validated;
- `finalize` removes staging and retained backup only after checks and receipt
  signing succeed;
- `rollback` is idempotent and restores the previous active version;
- rollback failure is a terminal `RollbackFailed`, never success.

If an Art requires a missing or non-ready framework, resolve it through Loom's
existing framework catalog/install contract before commit, or fail explicitly
with `framework_not_ready`. AssetLibrary currently does not publish a framework
package kind, so do not invent a framework download URL from its catalog.

### 7. Publish active behavior

After activation:

- Art packages enter Loom's tool registry and are exposed to Hook through
  `loom.hook.v1` `ArtCapability` metadata;
- Capability packages enter Loom's capability registry and are projected as a
  `loom.extension.v1` contribution snapshot;
- Hook receives digest-bound trust and permission-grant metadata, not a package
  directory or executable;
- all actual Art/Capability execution remains in Loom and produces normal durable
  run evidence.

The local install result is not successful until the active registry can reload
and the relevant Hook protocol projection validates. A live Hook acknowledgement
is additionally required when Hook initiated the install; Loom Desktop installs
must not otherwise require Hook to be running.

### 8. Sign and synchronize the InstallReceipt

After successful post-commit checks, sign the challenge-defined receipt payload
with the ephemeral private key and finalize the local transaction. Then call:

```http
POST /v1/me/install-receipts/{receipt_id}/verify
Authorization: Bearer <opaque Account Service bearer>
Idempotency-Key: <stable receipt key>
Content-Type: application/json

{
  "installed_at_epoch_seconds": 1,
  "signature_base64": "<Ed25519 signature>"
}
```

Expected status is `200` with `private, no-store`; the response must match the
receipt, release, artifact, digest, and `verified` status. Replay or expiry is a
`409`, not success.

Receipt network synchronization happens after the local install is finalized. A
temporary 401, 408, 429, 5xx, or transport failure produces a durable pending
receipt for bounded retry. Queue data may contain the signed non-secret receipt
facts, but never the Account bearer, Edge ticket, presigned URL, or proof private
key. A rejected receipt remains explicitly rejected for user/operator handling.

## Offline behavior

| State | Required behavior |
| --- | --- |
| No session/challenge | Block a new install; online authorization is required. |
| Unexpired current challenge plus verified cache in the same flow | Installation may continue without downloading bytes again. |
| Expired challenge | Fail closed even when cached bytes exist. |
| Download interruption | Preserve bound `.part` state and resume with a valid single Range. |
| Account/API outage after finalized install | Keep the local install and enqueue the signed receipt. |
| Loom restart during activation | Use existing journal/recovery rules to restore a coherent active version. |

This is short-lived preauthorized continuation, not indefinite offline trust.

## Errors exposed to Desktop and Hook

The native adapter should normalize failures into bounded fields such as:

```json
{
  "code": "asset_library_search_unavailable",
  "message": "Asset search is temporarily unavailable.",
  "retryable": true,
  "phase": "catalog"
}
```

Do not forward arbitrary remote bodies. Useful phases are `catalog`,
`authorization`, `challenge`, `download`, `verification`, `dependency`,
`staging`, `activation`, `rollback`, and `receipt`. Preserve important protocol
distinctions:

- `400`: invalid local/contract data; normally not retryable;
- `401`: Account Service session missing/expired;
- `403`: authorization/signature policy failed; no bypass;
- `404`: package, artifact, session, or current access unavailable;
- `409`: incompatibility, expiry, replay, lifecycle, or idempotency conflict;
- `429`: bounded retry using server guidance;
- `503`: dependency/projection unavailable; not an empty catalog;
- cancellation: explicit terminal state, followed by rollback after prepare.

Use one stable `Idempotency-Key` for retries of the same logical mutation. A new
user action gets a new key. Do not turn an ambiguous result into a second
mutation by automatically generating a fresh key.

## Reusing the AssetLibrary Rust client

`crates/loom-client` currently exports `ClientConfig`, `DownloadOrigin`,
`AccountBearer`, `LoomApiClient`, `ResumableDownloader`, `LoomInstaller`,
`InstallTarget`, `ReceiptQueue`, and related verified types. Reuse this
implementation or generate an equivalent strict client; do not rewrite the
security-sensitive protocol casually.

Because AssetLibrary and Loom are independent Git repositories, formal Loom
builds must not use a machine-relative dependency such as `../AssetLibrary`.
Choose one reviewed distribution mechanism:

1. publish a versioned crate/package and pin its exact version plus lockfile; or
2. use a Git dependency pinned to a full reviewed commit and preserve it in
   `Cargo.lock`.

Whichever mechanism is selected, CI must detect OpenAPI/schema drift and run the
AssetLibrary client contract fixtures. A passing upstream P7 suite alone does not
prove Loom's `InstallTarget`, registries, UI bridge, or Hook publication.

## Local full-flow verification

Catalog browsing can use the always-running API at `127.0.0.1:18080`. Managed
download requires both a valid external/test identity and the Edge byte path. The
AssetLibrary composite gate starts and owns those local test conditions:

```powershell
Set-Location C:\Users\Public\nas_home\AI\GameEditor\Neuro\AssetLibrary
.\scripts\Test-P7Runtime.ps1 -StartDependencies
```

The gate covers the real local API, persistence, object storage, scanner,
signature, public/restricted download, revocation, receipt anti-replay, and
library projection. It uses process-local test credentials. Never copy a test
bearer into Loom source, `.env`, Desktop storage, screenshots, or logs.

After the adapter exists, Loom must add a separate runtime gate that starts Loom
against the P7 harness and proves:

1. catalog list/search/detail/release pagination;
2. exact host-profile negotiation;
3. clean and resumed downloads, corruption rejection, cancellation, and expiry;
4. Art install, active registry reload, execution, upgrade, and rollback;
5. Capability install and digest/trust/grant-bound Hook snapshot;
6. pending receipt recovery after Account/API restoration;
7. absence of secrets in daemon/Desktop/Hook logs and events;
8. no writes outside configured Loom control-plane and cache roots.

## Implementation order and acceptance gate

This is one production architecture, not a disposable prototype. Implement it in
reviewable slices without changing the final ownership model:

1. pin the AssetLibrary contract/client and add configuration validation;
2. add daemon catalog methods and strict Desktop DTOs;
3. implement Account Service bearer injection at the native boundary;
4. adapt verified packages to Loom's Art and Capability installation targets;
5. expose bounded progress, cancellation, safe errors, and installed state;
6. publish active results through existing Hook protocols;
7. integrate receipt queue/retry and restart recovery;
8. add Neuro-style store UI only after the service boundary is tested;
9. pass focused unit/contract tests, the cross-process runtime gate, formatter,
   workspace compile checks, repository line checker, and `git diff --check`;
10. keep App Update disabled until its independent signed TUF release gate passes.
