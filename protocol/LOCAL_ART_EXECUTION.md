# Otter Local Art Execution Contract

This specification defines Loom's authorization, signed package preparation, resource brokering, asynchronous Capability service invocation, and Live viewer/trigger integration for Art JavaScript executing locally inside Otter (`otter.art.v1`).

## 1. Ownership and execution boundary

- **Single-side execution (`executionOwner = "otter"`)**: Local Art JavaScript executes inside Otter's managed QuickJS child runtime; Loom owns immutable package storage, publisher trust, dependency lock verification, per-device execution approvals, Ed25519 grant/package signing, and authenticated capability services.
- **Formal workspace results (`resultOwner = "otter_workspace"`)**: scripts render local previews and submit formal outputs via `NeuroSurface.commitResult(outputs)` (`otter.art.result.v1`) only when explicitly granted `workspace_result`, persisted through Otter's native Save/Checkpoint pipeline rather than direct filesystem access.
- **Canonical-only upgrades**: Otter and Loom upgrade together. Unknown or duplicate fields in `loom.art.local.*` envelopes fail closed without downgrading to legacy Surface tokens or older prepare endpoints.

## 2. Canonical package preparation (`POST /v3/art/local/prepare-package`)

The active Device-authenticated preparation endpoint is `POST /v3/art/local/prepare-package` (`16 KiB` request cap, exposed on both local HTTP and the LAN HTTPS allowlist):

- **Request (`loom.art.local.prepare-package.v3`)**: strictly contains `protocolVersion`, `binding` (`origin`, `requestId`, `deviceId`, `workspaceId`, `instanceId`, `generation`, `artId`, `artVersion`, `packageDigest`, `sdkVersion`), `permissions`, `services`, and `knownSourceDigest` (64 lowercase hex chars).
- **Response**: strictly contains `protocolVersion`, `grant` (`loom.art.local.grant.v1` Ed25519 envelope), `source` (standard Base64 UTF-8 JS bytes, or `null` when `knownSourceDigest` matches the verified source), and `package` (`loom.art.local.package.v3` Ed25519 envelope). Even on a source-cache hit (`source: null`), a fresh request-bound `grant` and `package` are required.
- **Signed `loom.art.local.package.v3` payload**: signed under the domain-separated prefix `loom.art.local.package.v3\0` (`schemas/local-art-package.v1.schema.json` + port/visibility extensions), binding:
  - `package`: complete `grant`, required string `devicePublicKey` (empty when offline continuation is unavailable; never omitted or null), and verified `resources` manifest;
  - `ports`: `inputs` and `outputs` (up to 32 entries per side, UTF-8 names <= 128 bytes, types <= 64 bytes, no whitespace/control chars or same-side duplicates, <= 8 KiB combined port/visibility JSON);
  - `visibility`: boolean `inputs`/`outputs` arrays matching `ports` 1-to-1, derived from each immutable manifest port's `defaultVisible` flag (default `true`).

*(Historical `prepare` v1, `prepare.v2` / `prepare-cached`, `prepare-resumable` (`loom.art.local.resume.v1`), and `prepare-package` v1/v2 schemas remain documented in `crates/loom_protocol` for reference, while production clients use `v3` exclusively.)*

## 3. Cryptographic signing, digests, and approval policy

- **Envelope format**: outer JSON `{ algorithm: "ed25519", keyId, payload, signature }` (<= 24 KiB outer, <= 16 KiB decoded payload, standard padded Base64). Signatures cover `<domain-string>\0<raw-payload-bytes>` and are verified against an independently pinned issuer public key before parsing payload JSON (`GET /v1/art/local/issuer` reports issuer metadata up to 4 KiB but never establishes initial trust on its own).
- **Distinct identities**:
  1. `packageDigest`: canonical SHA-256 of the unpacked immutable Loom package (not raw ZIP bytes).
  2. `source.digest`: SHA-256 of the exact UTF-8 JS entry bytes (or bounded `<= 32`-file `.sources.json` concatenation, 1 B..1 MiB, NUL-free), referenced as `sha256:<digest>`.
  3. `Device` session tokens and resource leases remain stateful server credentials and never substitute for a signed execution grant.
- **Explicit approvals (`settings/local-art-approvals.json`)**: managed solely by administrators via `GET /v1/art/local/approvals` and `PUT /v1/art/local/approvals` (guarded by `expectedRevision`; missing file denies all, corrupt file fails closed). Approvals bind `deviceId`, `artId` (`publisher/id`), `artVersion`, `packageDigest`, permitted subset of `["local_state", "preview", "loom_service", "workspace_result"]`, explicit `services` allowlist (non-empty <= 32 items iff `loom_service` is granted), `validUntilMs`, and `offlineTtlMs`.
- **Pre-prepare inspection (`POST /v1/art/local/describe`)**: Device-authenticated (`{"artId":"publisher/art"}`, <= 16 KiB), returning `loom.art.local.describe.v1` (`artId`, `artVersion`, `packageDigest`, `sdkVersion`, approved `permissions` and `services`) without exposing host paths or granting execution rights.
- **Package resources (`POST /v1/art/local/resource`)**: fetches verified `metadata.otterExecution.resources` entries (max 16 items: PNG <= 4 MiB each / 8 MiB total / 16 MiB decoded; `text/plain`, `application/json`, `application/octet-stream` <= 256 KiB each within a 512 KiB JSON budget) using `loom.art.local.resource.v1` + active `grant`.
- **Bounded offline continuation**: grants are valid for at most 5 minutes (capped at 15 minutes protocol ceiling and device/approval expiry). When `offlineUntilMs > 0` and all source + resource chunks are verified on disk (32-slot / 32 MiB cache + 32-slot / 1 MiB offline proof store with monotonic + persisted wall-clock high-water checks), Otter may resume local-only execution across process restarts until `offlineUntilMs` without network service access.

## 4. Asynchronous Capability services

Local Arts with `loom_service` and an approved `capability:<publisher>:<plugin.command>` allowlist entry invoke Loom-hosted Capability Plugins asynchronously without exposing general `/v1/invoke` routes (`apps/daemon/src/local_art/service*.rs`, per-daemon task table capped at 128 total / 16 in-flight globally and 32 total / 4 in-flight per device, `timeoutMs` `1..=60000` capped by remaining grant lifetime):

1. **Bounded JSON service (`loom.art.local.service.v1`)**: `POST /v1/art/local/service/{submit,poll,cancel}` (96 KiB request cap, 64 KiB JSON input/output cap, 4 KiB progress cap). Idempotent retries with identical owner/input/timeout return the existing task; mutated inputs return `409`.
2. **Image-handle inputs (`loom.art.local.service.images.v1`)**: `POST /v1/art/local/service/submit-images` (`NeuroSurface.serviceImages` / `NeuroSurface.serviceCanvases`), passing up to 4 verified PNGs (<= 4 MiB each / 8 MiB total for installed handles; <= 2 MiB each / 3 MiB total intrinsic bitmap snapshots for `ArtCanvas`). Requires the target Capability Plugin and command to be `trusted` and granted `hook.unit.image.read`. Staged leases are released immediately upon completion, failure, cancellation, or revocation.
3. **Single-PNG service output (`loom.art.local.service.image-output.v1`)**: `POST /v1/art/local/service/{submit-image-output,poll-image-output}` (`NeuroSurface.serviceImage`), returning `{ protocolVersion, reply, image }` where `image` is a host-decoded/verified PNG (`<= 2 MiB` encoded, `<= 4096` px per side, `<= 16 MiB` decoded, shared 8 MiB daemon terminal-image lease budget) delivered over Otter's binary child pipe without entering JS Base64 strings.
4. **Host-confirmed sensitive commands (`loom.art.local.service.confirmation.v1`)**: `POST /v1/art/local/service/{submit-confirmable,poll-confirmable,confirm}` (`NeuroSurface.serviceWithConfirmation`). Commands requiring `requiresUserGesture` pause behind a host-owned challenge (`nonce`, SHA-256 input/image `fingerprint`, `pluginId`, `commandId`, `packageDigest`, `scopeId`, `expiresAtMs`). Otter's native UI displays full parameters and image thumbnails (defaulting to deny); scripts never see or submit confirmation tokens.

## 5. Local Art Live viewer, input control, and automatic triggers

- **Read-only Live viewer attach (`POST /v1/art/local/live-viewer/attach`)**: `loom.art.local.live-viewer.v1` (32 KiB max) lets an explicit native Otter menu action attach a running Local Art host as a `loom.live.v1` viewer after verifying Device session, signed grant, active approval, and immutable source digest.
- **Native Live input control (`POST /v1/art/local/live-viewer/control`)**: `loom.art.local.live-control.v1` (32 KiB max, `Device` + nonce) supports `acquire`, `release`, and `input` for an attached viewer when permitted by the Live source. Revoked/expired Arts may still call `release` to relinquish their own controller lease cleanly.
- **Automatic Live observation callbacks (`POST /v1/art/local/live-trigger`)**: `loom.art.local.live-trigger.v1` (32 KiB max, `Device` + nonce; implemented in `apps/daemon/src/local_art/live_trigger.rs` and `runtime/live_session_local_trigger*.rs`):
  - Requires explicit native user configuration (`consent: "automatic_local_callback"`, `local-art:` `bindingId`, fresh `authorizationId`, and online viewer membership). `preview` grants or offline tickets never authorize automatic callbacks.
  - Shares Loom's deterministic `evaluate_live_trigger_state` engine and per-session 64-trigger cap, holding at most one 16 KiB single-slot delivery per registration (30-second max freshness capped by observation and grant expiry).
  - Operations: `configure`, `inspect` (metadata-only), `claim` (at-most-once payload handoff transitioning state to `Claimed` before writing the HTTP reply), `ack` (`applied(formalRevision)`, `no_result`, `failed`, or `cancelled`), and `stop`.
