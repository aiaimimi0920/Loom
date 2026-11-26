# Loom Plugin and Runtime Protocols

This directory contains the public, source-independent wire contracts and JSON Schemas for Framework, Art, Capability Plugin, Surface, Device, Projection, Live Screenshot, and Tile Wall integrations. Rust types live in `crates/loom_protocol`, and the JSON Schema files in `protocol/schemas/` are the language-neutral source of truth. Plugin and companion authors must never depend on private Loom or Otter source files.

## Protocol index

- [Device Session Authentication](DEVICE_SESSIONS.md): Ed25519 pairing requests, pending/challenge budgets, short-lived opaque `Device` sessions, and terminal vs. retryable error codes.
- [Otter LAN Connection](LAN_CONNECTION.md): optional RFC1918 HTTPS listener (`8766`), UDP `49779` broadcast discovery, TOFU certificate pinning, and exact route allowlists.
- [Native Local Bridge Security](LOCAL_BRIDGE_SECURITY.md): loopback TLS 1.3 WebSocket transport, private discovery manifest, and connection-bound exporter proofs.
- [Otter Local Art Execution](LOCAL_ART_EXECUTION.md): `POST /v3/art/local/prepare-package` (`loom.art.local.package.v3`), Ed25519 execution/offline grants, package resources, signed port visibility, async Capability services (JSON, image inputs, Canvas inputs, PNG outputs, host confirmations), Live viewer attach/control, and `onLiveTrigger` callbacks.
- [Live Screenshot Protocol v1](LIVE_SCREENSHOT_PROTOCOL.md): `loom.live.v1` control envelopes, 64-byte `NLLV` binary media frames (`raw_bgra`, negotiated `loom.live.jpeg.v1`, and `loom.live.h264.v1` continuity), in-place legacy buffer adaptation, stage diagnostics, bounded baseline sampling, and observation triggers.
- [QR Projection v1](QR_PROJECTION.md) and [QR Projection v2](QR_PROJECTION_V2.md): signed QR invitations, central account coordination, and Iroh/HTTPS peer image transfer.
- [Offline Projection Peers](OFFLINE_PROJECTION_PEERS.md), [Projection Editing](PROJECTION_EDIT.md), [Projection Settings](PROJECTION_SETTINGS.md), [Sticker Delivery](STICKER_DELIVERY.md), and [Account Login](ACCOUNT_LOGIN.md).
- [Wall Protocol](WALL_PROTOCOL.md), [Wall Control API](WALL_CONTROL_API.md), [Wall Input API](WALL_INPUT_API.md), [Wall Surface API](WALL_SURFACE_API.md), [Wall Presentation API](WALL_PRESENTATION_API.md), [Wall Identification API](WALL_IDENTIFICATION_API.md), and [Wall Scene Timing and Media](WALL_TIMING_MEDIA.md).

## Compatibility and canonical-only rules

LAN discovery is not authorization: TOFU pins a certificate only after explicit target selection; it cannot prevent an attacker impersonating a hub on that first selection. Later broadcasts cannot replace the pin. Previously known devices still require connection activation and signed device-session authentication. This is distinct from the loopback bridge's per-start certificate and TLS exporter proof.

- Loom and Otter upgrade together on canonical wire contracts. Obsolete wire aliases, unnamespaced methods, ArtLoom/AHRP routes, short Art-ID catalog aliases, and legacy app-data paths are rejected rather than translated.
- `loom.framework.v1` is stable: existing field names and single-request/single-response JSON framing do not change incompatibly.
- `loom.hook.v1` is the Loom<->Otter bridge contract governing capability discovery, Art execution requests/results, Surface attachment/resource operations, workflow synchronization, and structured failures. Hosts negotiate via `protocolVersion` plus `supportedProtocolVersions`; packages must never infer behavior from application version strings.
- `loom.art.execute.v1`, `loom.art.result.v1`, and `loom.art.runtime.v1` define Framework and package-local Art execution entries.
- `loom.surface.v1` defines distributed Art Surface manifests, `SurfaceSnapshot`, ordered `SurfacePatch` revisions, typed events, content-addressed `sha256:<hex>` resources, previews, and atomic `SurfaceResultCommit` publication.
- `otter.art.v1` and `loom.art.local.*` define single-side Otter QuickJS Art execution authorized by Loom's Ed25519 grants and `POST /v3/art/local/prepare-package` packages (see [LOCAL_ART_EXECUTION.md](LOCAL_ART_EXECUTION.md)).
- `loom.wall.v1` defines physical display endpoints, wall layouts, source placements, ephemeral Surface views, freeze/blackout/resume controls, output identification, and `loom.wall.media.v1` (`NLWM`) streaming.
- Unknown optional fields are ignored where schema-extensible; strict security envelopes (`local_art`, `live-control`, `qr-projection.v2`) reject unknown or duplicate fields fail-closed.

## Loom<->Otter bridge and Surface v1 transport

The local Otter bridge uses native-only TLS 1.3 with a pinned per-start certificate, private discovery manifest, and connection-bound TLS exporter proof ([LOCAL_BRIDGE_SECURITY.md](LOCAL_BRIDGE_SECURITY.md)). Daemon HTTP administrator tokens are independent and are never sent to the WebSocket bridge; WebViews use narrow native IPC rather than opening direct bridge WebSockets.

- Bridge Art identities are always publisher-qualified (for example `neuro.official/surface-device-dashboard`).
- Every `loom.hook.art.execute` request declares a non-empty `outputTransports` array (`shared_memory` and `websocket` for native clients; `websocket` for browser clients). After consuming shared-memory handles, clients send `loom.hook.art.resources.release` bound to the exact `executionRequestId`, device, node, generation, and handle set.
- Surface pushes use only exact `loom.surface.*` names (`snapshot`, `patch`, `generation`, `action.ack`, `confirmation.request`, `action.progress`, `preview`, `result`, `failure`, `lifecycle`, `dispose`).
- Actions execute under their declared timeout and concurrency policy. Actions declaring `cancelable: true` can be cancelled only by the approved owning attachment device; high-risk actions wait in Loom's host-owned confirmation broker until device-bound user approval is recorded.
- Remote Otter devices pair via Ed25519 public keys (`POST /v1/devices/requests`), exchange one-time challenges (`loom.device-session.v1`), send unique per-request nonces, and poll `/v1/surfaces/stream` (`schemas/surface-stream.v1.schema.json`) over authenticated HTTPS.
- Surface instance version upgrades are explicit: Loom runs the target package's bounded JSON merge-patch migration chain, increments generation, remounts live attachments, and retains an 8-entry checkpoint ring for exact rollback.
- Capability results may emit `external.openUrl` only when both package and command declare `hook.external.open`, a real user gesture is consumed, and the target is a credential-free `https://` URL revalidated by the native host.

## Canonical package identities

Package-local IDs use ASCII letters, digits, `.`, `_`, and `-`. Publisher-qualified packages use `publisher/id`:

- Storage, activation, upgrade, rollback, uninstall, and HTTP APIs operate on `publisher/id` (URL-encoded as `%2F` inside single path segments, e.g., `publisher.example%2Fimage-tool`).
- Bare IDs resolve only when unambiguous across installed publishers. One publisher's package can never upgrade or overwrite another publisher's package.

## Capability runtime ABI and extension authorization

Capability Plugins speak `loom.capability.runtime.v1` over stdin/stdout: each message is one UTF-8 JSON document prefixed by a 4-byte unsigned big-endian byte length (4 MiB max frame), supporting `initialize`, `activate`, `command`, `cancel`, `health`, and `deactivate`. Runtime diagnostics go exclusively to `stderr`.

Otter enforces `resource.authorization.v1` before reading Unit images or attachments:

1. Otter sends `loom.extension.command.authorize` (`requestId`, `sessionId`, `pluginId`, `commandId`, `snapshotGeneration`, `target`, optional `checkOnly`).
2. Loom verifies package trust and digest-bound grants in `grants.json`, issuing an opaque single-use `authorizationId` (30-second TTL, max 32 outstanding tickets per connection).
3. `loom.extension.command.invoke` consumes that `authorizationId` before staging uploads, resource references, unit attachments, or OCR contexts, re-verifying grants before and after execution.

Resource authorization denies by default when current digest-bound grants do not cover the command's requested permissions. `checkOnly` returns authorization status without issuing a ticket. Tickets are single-use and connection-bound; reconnecting requires a new ticket. This does not change Framework permission mode's separate `audit` default.

## Normative framework process ABI and ZIP layouts

1. Loom starts one managed child process per `loom.framework.v1` execution inside the immutable framework directory, writes one newline-terminated UTF-8 JSON request to `stdin`, and closes `stdin`.
2. The process writes one UTF-8 JSON response (`status: "success"` or structured `error`) to `stdout` and logs only to `stderr`.
3. Loom enforces timeouts, stdout/stderr byte caps, credential scoping, and full process-tree termination (Windows Job Objects; Linux Bubblewrap + `pidfd`).

Framework and Art ZIPs extract into read-only `versions/<version>-<digest>/` directories with atomic `active.json` pointers, `locks/<package-digest>.json` dependency lockfiles, and separate writable `state/`/`cache/`/`outputs/` roots for Arts. Ed25519 detached signatures (`signature.json`) enforce `unsigned`, `verified`, `trusted`, `invalid`, and `revoked` trust states under `LOOM_PLUGIN_TRUST_POLICY` (`allow-unsigned` default in dev; `require-trusted` recommended in production) and `LOOM_PLUGIN_PERMISSION_MODE` (`audit` vs. `strict`).

## Public JSON Schemas and SDK CLI

Schemas under `protocol/schemas/`:

- `capability-package.v1.schema.json`, `capability-runtime.v1.schema.json`, `extension.v1.schema.json`
- `framework-manifest.v1.schema.json`, `framework-execute-request.v1.schema.json`, `framework-execute-response.v1.schema.json`, `framework-authoring.v1.schema.json`
- `art-runtime.v1.schema.json`, `local-art-grant.v1.schema.json`, `local-art-package.v1.schema.json`, `local-art-package.v2.schema.json`, `local-art-package.v3.schema.json`, `local-art-resume.v1.schema.json` (v3 is the production prepare route; older package schemas are reference-only)
- `surface-manifest.v1.schema.json`, `surface-message.v1.schema.json`, `surface-scene.v1.schema.json`, `surface-stream.v1.schema.json`
- `device-session.v1.schema.json`, `otter-message.v1.schema.json`, `live-control.v1.schema.json`, `qr-projection.v1.schema.json`
- `wall.v1.schema.json`, `wall-presentation.v1.schema.json`, `wall-identification.v1.schema.json`, `wall-timing.v1.schema.json`

Standalone Plugin SDK CLI (`loom-plugin`):

```text
loom-plugin init framework <DIR> <ID> <PUBLISHER>
loom-plugin init art <DIR> <ID> <FRAMEWORK> <PUBLISHER>
loom-plugin init capability <DIR> <ID> <PUBLISHER>
loom-plugin validate <PATH> [--trust-store <STORE>]
loom-plugin pack <SOURCE_DIR> <OUTPUT_ZIP>
loom-plugin conformance <EXE> <FRAMEWORK> <ART_DIR>
loom-plugin keygen <KEY_FILE> <KEY_ID>
loom-plugin sign <PACKAGE_DIR> <KEY_FILE> <PUBLISHER>
loom-plugin catalog sign <PAYLOAD_JSON> <KEY_FILE> <OUTPUT_JSON>
loom-plugin catalog validate <CATALOG_JSON> <TRUST_STORE>
loom-plugin trust add <STORE> <PUBLISHER> <KEY_FILE>
loom-plugin trust revoke <STORE> <PUBLISHER> <KEY_ID>
```
