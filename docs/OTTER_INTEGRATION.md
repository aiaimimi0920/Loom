# Otter and Companion Integration

This document consolidates Loom's integration contracts with Otter and companion desktop applications: global connection control, two-level Host -> App Sessions hierarchy, Otter Canvas projection, multi-device workflow sync and history archiving, intra-workflow hybrid terminal relay, Otter display language management, and the Companion App Settings Hub.

## 1. Global Otter<->Loom connection control

Each Otter process connects to one global Loom hub at a time, sharing connection state across all pinned stickers/units (`apps/daemon/src/connection_policy.rs` and `apps/daemon/src/runtime/otter_connection_routes.rs`).

### Identity, approval policies, and connection passwords

- Loom reuses the persistent Ed25519 identity from `offline-projection-peers.json` (`loom-<key-sha256>`) and stores connection policy in `settings/loom-connection.json` (failing closed on mismatch or corruption rather than silently resetting policy or passwords).
- Connection codes are hashed with salted `PBKDF2-HMAC-SHA256` (`600,000` iterations). Public APIs and LAN discovery broadcasts expose only `passwordRequired: bool`, never password hashes, salts, or admin tokens.
- Administrators choose one of three approval policies:
  1. **Auto-approve on LAN**: relies strictly on the daemon's own LAN TLS listener marker, never caller-claimed IPs or headers.
  2. **Auto-approve previously connected devices (`authenticated`)**: requires an active valid `Device` session whose public key matches the request and a prior successful `/activate` record.
  3. **Manual approval**: queues requests for explicit administrator decision.
- Connection requests are independently gated by the connection password when enabled (minimum 8 bytes; disabling clears the stored hash). Disconnected devices retain pairing approval; deleting a device revokes pairing and invalidates active sessions.

### HTTP control routes

| Route | Authorization | Behavior |
| --- | --- | --- |
| `GET /v1/otter/identity` | Public | Returns `loom.connection.v1` public Loom ID and non-secret policy flags. |
| `POST /v1/otter/connections/request` | Public (optional `Device` auth) | Submits device name, Ed25519 public key, and optional password; returns `requestId` and one-time high-entropy `receipt` (5-minute TTL, 64-slot cap with 16 slots reserved for authenticated devices, 20 req/min rate limit). |
| `POST /v1/otter/connections/status` | Private `receipt` | Polls pending/approved/rejected state for that exact request. |
| `POST /v1/otter/connections/cancel` | Private `receipt` | Cancels a pending connection request. |
| `POST /v1/otter/connections/activate` | Matching `Device` session + `receipt` | Activates or renews the 30-second active connection lease (renewed every 5 seconds by Otter). |
| `GET /v1/otter/connections` | Administrator | Lists connection policy and unexpired pending requests (without receipts). |
| `PUT /v1/otter/connections/policy` | Administrator | Updates policy/password and revokes unconsumed pending requests. |
| `POST /v1/otter/connections/decision` | Administrator | Approves or rejects a pending request. |

When LAN discovery is enabled alongside an HTTPS LAN origin (see [LAN connection](../protocol/LAN_CONNECTION.md)), Loom broadcasts a bounded (<= 1,024-byte) `loom.connection.v1` UDP hint to `255.255.255.255:49779` every 2 seconds. Clients treat UDP packets strictly as discovery hints and verify the target identity over TLS.

## 2. Two-level Host Device -> App Sessions hierarchy

To prevent flat device lists from mixing physical machines with multiple running companion apps, `GET /v1/devices/hierarchy` (`apps/daemon/src/runtime/device_hierarchy_routes.rs`) aggregates paired devices and live bridge/sync state into a two-level tree:

1. **Host Devices**: physical or virtual machines identified by their paired identity, online status, approval state, and LAN/local attributes.
2. **App Sessions**: active application sessions (`otter`, bridge subscribers, live workflow sync reporters, and locale receivers) nested beneath their owning Host Device.

## 3. Direct projection and reserved Cloud Relay

Implemented image and object projections use direct Loom-mediated routing (`Otter -> Loom 1 -> Direct Peer -> Loom 2 -> Otter`):

- **Same-Loom & LAN Peer**: governed by [QR Projection v2](../protocol/QR_PROJECTION_V2.md), [Offline Projection Peers](../protocol/OFFLINE_PROJECTION_PEERS.md), and [Projection Edit](../protocol/PROJECTION_EDIT.md). Target directories expose connected user and device summaries across the local Loom and directly trusted 1-hop peer Looms without leaking credentials or 2-hop directories.
- **Internet 3-Tier Cloud Relay (reserved)**: configuration/status exist, but send/inbox return `501 cloud_relay_transport_unavailable` after applicable checks. No cloud payload is sent or queued, and the cloud-domain MCP alias is local dispatch, not tunneling (see [AssetLibrary & Cloud Relay](ASSET_LIBRARY_INTEGRATION.md#5-naccount-3-tier-cloud-relay-v1cloud-relay)).

## 4. Otter Canvas projection and preview security

The **Otter Sync** workbench renders live Otter node positions, links, and image previews from `GET /v1/hook-bridge/canvas` (`apps/daemon/src/otter_canvas/`) without creating a competing session authority:

- **Canonical producers**: persisted Otter sessions (`stickers`/`links`, node types `sticker`|`art`) and live `loom.hook.workflow.sync` snapshots (`nodes`/`edges`, node types `sticker`|`artNode`, stored as `hook-live` -> `latest.yaml`). Packaged Art IDs are publisher-qualified (`publisher/id`), distinct from built-in `core.image.*` operators.
- **Viewport & workflow actions**: supports pan, zoom (`5%..=600%`), Home-to-fit, minimap recentering, connected-component selection, saving to Loom workflows (`/v1/hook-bridge/canvas/workflows/*`), desktop instantiation (`reference` mode), and export to the Art creation wizard.
- **Preview security boundary**: `GET /v1/hook-bridge/canvas/nodes/{nodeId}/preview` resolves node IDs against the daemon's active canvas document rather than accepting caller-supplied file paths. Local file previews are canonicalized and confined to the session's `images/` or Otter clipboard-cache roots, capped at 20 MiB (128-byte data-URL header cap, traversal depth 64), and never fetch arbitrary remote HTTP(S) URLs.

## 5. Multi-device Otter Sync and workflow history archive

`apps/daemon/src/otter_sync.rs` tracks live workflow execution across up to 128 paired devices and maintains a persistent 200-entry workflow history archive (`<control-plane>/otter-sync/workflow-history.json`, 512 KiB max per workflow):

- `GET /v1/otter-sync/devices`: lists live per-device execution states (`idle`, `running`, `paused`, `failed`), active workflow/node IDs, node counts, and progress.
- `POST /v1/otter-sync/devices/{deviceId}/state`: updates live execution telemetry for the authenticated device (enforcing actor == `deviceId` for device sessions).
- `GET /v1/otter-sync/history`: lists archived workflow summaries (`entryId`, `workflowId`, `name`, `sourceDeviceId`, `tags`, `archivedAtMs`).
- `POST /v1/otter-sync/history/archive`: archives a reusable workflow snapshot (`201 Created`).
- `POST /v1/otter-sync/history/{entryId}/restore` & `DELETE /v1/otter-sync/history/{entryId}` (admin-only): restores an archived workflow into `WorkflowStore` (broadcasting an Otter Bridge update) or deletes the archive entry.

## 6. Intra-workflow hybrid execution relay

When a `loom.workflow.v1` DAG combines server-side nodes with terminal-bound Otter nodes, `apps/daemon/src/terminal_relay.rs` coordinates remote terminal execution over WebSocket (`loom.hook.workflow.node.relay.request`) or authenticated HTTP polling:

- `POST /v1/otter/relay/register`: records a terminal worker heartbeat for the authenticated device.
- `GET /v1/otter/relay/pending`: polls pending terminal step descriptors scoped to that device.
- `POST /v1/otter/relay/results`: delivers structured node output or failure back to the waiting `loom_workflow_runtime` DAG slot.

## 7. Otter display language control

Administrators can inspect and switch an approved online Otter's UI language independently of Loom's own display language (`apps/daemon/src/otter_locale/`):

- Otter posts `POST /v1/otter/locale-report` every 2 seconds with its `receiverId`, `receiverGeneration`, applied `language`, monotonic `revision`, and `supportedLanguages` (8 KiB max). The daemon claims one active receiver per device (15-second lease) and surfaces the `locale` snapshot on `GET /v1/devices`.
- Administrators submit `POST /v1/devices/{id}/locale-requests` (`requestId`, `receiverGeneration`, `expectedRevision`, `language`), returning `202 Accepted` (`200` on idempotent retry, `409` on revision/generation conflict). Requests expire after 15 seconds with no offline queue; Otter applies the change through the same setting owner as its system tray menu and reports `applied(appliedRevision)` on its next poll.

## 8. Companion App Settings Hub

Loom acts as a centralized configuration hub for Neuro family and companion desktop applications (`apps/daemon/src/companion_config.rs` and `crates/loom_configuration`):

- **Ownership claims**: `GET /v1/configuration/claims?app=<app>` reports whether Loom currently manages `<app>` (`LOOM_MANAGED_CONFIG_APPS` and `LOOM_SETTINGS_BASE_URL`), returning `managed` and `panel_url` so companion apps route settings buttons to Loom and block conflicting local writes.
- **Built-in app configs**: `GET /v1/configuration/apps/{app}` and `PUT /v1/configuration/apps/{app}`.
- **Schema-driven companion apps**:
  - `GET /v1/configuration/companion-apps`: lists registered companion app configuration summaries.
  - `PUT /v1/configuration/companion-apps/{appId}/schema`: registers or updates `displayName`, `schemaVersion`, `UiSection` definitions, and `defaultConfig` (256 KiB document cap; `appId` uses safe lowercase ASCII `[a-z0-9_-]{1,64}` excluding reserved `loom`/`gateway`).
  - `GET /v1/configuration/companion-apps/{appId}` & `PUT /v1/configuration/companion-apps/{appId}`: reads or atomically updates `config` guarded by `expectedRevision` (`409 / revision_conflict` on stale writes).
