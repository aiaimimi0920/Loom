# Wall control API

Status: implemented control plane, image/Live presentation, endpoint-scoped
Live input and [declarative Art views/actions](WALL_SURFACE_API.md).
Display controls are specified in [WALL_PRESENTATION_API.md](WALL_PRESENTATION_API.md).
Physical screen metadata and identification are specified in
[WALL_IDENTIFICATION_API.md](WALL_IDENTIFICATION_API.md).
Scene scheduling, media profiles and source recovery are specified in
[WALL_TIMING_MEDIA.md](WALL_TIMING_MEDIA.md); their new runtime tests are deferred.
Final multi-terminal acceptance remains in [the plan](../docs/TILE_WALL_IMPLEMENTATION_PLAN.md).
The [geometry contract](WALL_PROTOCOL.md) defines endpoint and layout objects.

## Authentication and operations

Requests use Loom's existing administrator authentication or approved paired
device sessions. Device requests require a fresh `X-Loom-Device-Nonce`. Remote
access uses the existing HTTPS deployment boundary. JSON mutation bodies reject
unknown fields and are limited to 512 KiB. Methods and paths are matched exactly.

| Method and path | Body | Authority |
| --- | --- | --- |
| `GET /v1/walls/state` | None | Admin: all state; device: own endpoints and member walls |
| `POST /v1/walls/endpoints/register` | `{baseRevision, endpoint}` | Admin or the endpoint's paired device |
| `POST /v1/walls/endpoints/remove` | `{baseRevision, endpointId}` | Admin or the endpoint's paired device |
| `POST /v1/walls/endpoints/identify` | `{endpointId}` | Admin; online identification-capable endpoint |
| `POST /v1/walls/endpoints/identify/report` | `{endpointId, leaseId, requestId, outcome}` | Endpoint's paired device and current lease |
| `PUT /v1/walls/layouts` | `{baseRevision, layout, activationDelayMs?}` | Admin |
| `POST /v1/walls/layouts/remove` | `{baseRevision, wallId}` | Admin |
| `PUT /v1/walls/presentation` | `{baseRevision, wallId, mode}` | Admin |
| `POST /v1/walls/connect` | `{endpointId}` | Endpoint's paired device |
| `POST /v1/walls/heartbeat` | `{endpointId, leaseId, sequence, appliedRevision, presentation?, scene?}` | Endpoint's paired device and current lease |
| `POST /v1/walls/disconnect` | `{endpointId, leaseId}` | Endpoint's paired device and current lease |
| `POST /v1/walls/images/read` | `{endpointId, leaseId, revision, resourceId}` | Current presenter of a wall referencing this image |

Administrators can configure displays, but presentation leases always require a
paired device identity, including on the same computer. Registration requires an
approved, enabled device; a nonlocal device must have a paired public key.
Endpoint IDs cannot be rebound to a different device or physical output. Two
endpoints cannot claim the same `(deviceId, outputId)`, and an endpoint can belong
to at most one wall. Remove an endpoint from its wall before unregistering it.

State and successful catalog mutations return:

```json
{
  "protocolVersion": "loom.wall.v1",
  "revision": 0,
  "endpoints": [],
  "layouts": [],
  "timing": { "clockId": "boot-scoped-opaque-id", "serverTimeMs": 1700000000000, "scenes": [] }
}
```

Each endpoint entry is `{endpoint, online, appliedRevision}`. `appliedRevision`
is null until an online presenter acknowledges the current layout. It is also
null for offline endpoints. Lease credentials are never included in state.
The presentation extension adds optional state `presentations` and endpoint
`presentation` reports; absent fields retain the ordinary state shape.
Identification adds optional endpoint `display` metadata and volatile status
`identification`; its commands do not change the catalog or layout revision.
Timing adds a boot clock, per-layout activation deadlines and optional endpoint
`scene` receipts. These do not persist with the layout document.
A device receives complete geometry for its member walls but only its own
endpoint registrations. This does not grant access to a referenced Live session
or Surface; their existing attachment and controller permissions still apply.

## Revisions and persistence

`baseRevision` compares against the global catalog revision returned by state,
not against an individual layout revision. Every committed catalog change
increments this counter; removing and recreating an ID cannot rewind it. A layout
write requires `layout.revision == baseRevision + 1`. Stale writes return HTTP 409
and must be reconciled with newly read state, not replayed blindly. Registering
an identical endpoint with the current base revision is a no-op.

Changing endpoint capabilities or pixel dimensions revises every affected wall,
clears its presenters' applied acknowledgements and invalidates the changed
endpoint's lease. Editing/removing a layout also clears affected acknowledgements.
An unchanged peer can keep its connection and acknowledge the new layout. These
transitions do not claim a completed media switch or authorize old input events.

The daemon stores one bounded document at
`<control-plane-root>/walls/walls.json`, with storage version 1. Limits are 256
endpoints, 64 walls and 8 MiB serialized JSON, in addition to per-layout limits.
All identities are JSON data; none are interpolated into storage paths. An OS
exclusive `writer.lock` prevents multiple daemon/store owners from writing the
same document. In-process mutation uses a mutex and compare-and-swap revision.
Writes use the existing private-permission atomic replacement and flush helper.

An absent document starts empty. Corrupt, oversized or unknown-version storage
fails closed without overwriting it. A persistence error can occur after rename,
so the store refuses further access until it is reopened and durable state is
revalidated. It does not continue serving potentially stale in-memory revisions.

## Presence and recovery

Connect returns `{protocolVersion, leaseId, leaseTtlMs}` with a 15,000 ms TTL.
A second presenter cannot take over an unexpired lease. Heartbeat sequences are
strictly increasing positive safe integers; `appliedRevision` is null or the
current member-wall revision. Heartbeats and disconnect return
`{protocolVersion: "loom.wall.v1", accepted: true}`.

Presence uses a monotonic deadline and is not persisted. Heartbeats should run
well before the TTL, for example every 5 seconds. After expiry/restart, read state
and connect again; the previous lease cannot renew or disconnect its successor.
A disabled/revoked device cannot renew its lease and is reported offline. A lost
connect response may require waiting for that lease to expire; clients must not
assume an ambiguous mutation failed and silently repeat it.

Daemon restart restores configuration with every endpoint offline and all leases
discarded. Existing device-session authentication must also be re-established.
Restoring wall references does not assert that an operating-system application,
Live source, Art action or hardware display has already recovered.

## Immutable image presentation

An administrator imports raster bytes through the existing exact
`POST /v1/surfaces/resources` route (32 MiB JSON body limit, 16 MiB decoded
resource limit). Use `kind: "image"`, the raster MIME and `dataBase64`; no shared
memory transport is required. Only the returned `sha256:<64 lowercase hex>` ID
is stored in the wall placement. Imported images and Art formal image outputs
share this path; Art previews do not enter the persistent image inventory.

`POST /v1/walls/images/read` requires paired device authentication even on
loopback. The endpoint must belong to that device, hold the current unexpired
presenter lease, and belong to the requested current layout revision. That wall
must contain an exact image reference to the requested resource. Administrator
authentication alone cannot read through this presenter route. An accepted read
is authorized at the request boundary; later layout changes cannot recall bytes
already delivered. Clients must discard late results after lease/layout loss.

The response is `{protocolVersion: "loom.wall.v1", resource, dataBase64}` with
the existing `SurfaceResourceDescriptor`. PNG, JPEG, WebP, BMP, GIF (first frame)
and `application/x-neuro-rgba8` are admitted MIME types. The daemon checks the
stored length and SHA-256 on each read. The terminal additionally checks format,
dimensions and allocation limits before displaying anything. A missing or
damaged image fails without acknowledging the layout as applied.

This route does not mint a Surface resource lease or disclose a source path.
Wall references retain image resources through the existing GC, including after
the importing Surface lease expires or the original Art is removed. Removing
the last wall reference makes an otherwise unused image eligible for normal GC;
unplaced uploads retain only their temporary resource lease and GC grace period.
Persisted wall references are loaded before startup GC runs.

## Live media presenter grant

`GET /v1/walls/live/media?endpointId=...&leaseId=...&revision=...&sessionId=...&format=raw_bgra`
upgrades to `loom.wall.media.v1`; `format=png` negotiates the bounded image profile.
A fresh
device request nonce and paired Device credential are required, including on
loopback; an administrator bearer alone cannot open this viewer path.

The endpoint must belong to the authenticated device, advertise `raw_bgra` for
raw frames or `image` for PNG, and hold its current presenter lease. The requested Live source must intersect this
tile in the assigned current layout. Merely referencing a source elsewhere in
the same wall does not grant its media to this endpoint. The Live session must
already exist; this route never starts an application or another capture.

The server checks the device token, approval, enabled flag and session epoch,
presenter lease, layout revision and visible source before each wait and again
before each send. Waits and writes are bounded at 250 ms. Revision replacement,
source removal, expired/replaced leases and device revocation close the socket.
Disconnected sources are rejected, and frames older than five seconds by the
daemon's monotonic receive clock cannot be replayed on reconnect. Source-provided
timestamps do not extend this freshness boundary.
Bytes already in flight cannot be recalled; clients invalidate the previous
presentation generation and discard its late results. No catalog lock is held
while waiting or writing to the network.

Frames reuse the shared source buffer and use the NLWM envelope, common receive
time and raw BGRA/PNG limits in [WALL_TIMING_MEDIA.md](WALL_TIMING_MEDIA.md).
The source path remains NLLV. Wall media shares the daemon's 65-media-worker
budget. Frame data, grants and connections are volatile. A wall viewer does not
create a Surface attachment, add a device to the session's Surface viewer list,
consume that device's Live control/input sequence, or acquire controller rights.
Multiple output processes on one device may view the same source, including a
source running on that device, without sharing endpoint presenter leases.
Only WebSocket liveness messages are accepted from wall viewers. The separate
[endpoint input API](WALL_INPUT_API.md) keeps reliable input edges outside media
connections and documents controller acquisition, renewal, release and input.

## Verification

`cargo test --locked -p loom-daemon --lib wall_store::` covers durable/volatile
separation, competing writers, identity/membership, stale layouts, resizing peer
acknowledgements, lease replay/expiry, corrupt storage and uncertain writes.
`cargo test --locked -p loom-daemon --lib wall_http::` uses real loopback TCP
requests, signed pairing, two device identities, admin/device scope, revocation,
bounded payloads and a real daemon stop/rebind. This verifies a headless API path;
it is not a two-physical-computer display acceptance result.
