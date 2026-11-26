# Device Session Authentication and Error Contract

Remote and local paired devices authenticate with Ed25519 identities registered via `POST /v1/devices/requests` and explicitly approved by a Loom administrator. Registration never implies approval: newly registered devices start as `approval = "pending"` with `enabled = true`, and re-registering an existing public key never clears an administrator disablement or creates a duplicate identity.

## Public pairing receipts and identity privacy

- Public `POST /v1/devices/requests` responses retain `devices` / `pending` arrays containing only the single receipt matching the submitted public key (`id`, `publicKey`, `name`, `approval`, `enabled`), echoing caller-supplied `name` and `publicKey` rather than administrator-edited labels or addresses.
- New requests, duplicate pending requests, and repeated submissions for approved or disabled keys return the same minimal receipt shape without leaking other devices, host metadata, connection counts, timestamps, or `sessionEpoch` values.
- New device IDs use random v4 UUIDs. Pre-existing legacy IDs are preserved to avoid breaking paired installations without allowing third-party enumeration. Administrator `GET /v1/devices` and `POST /v1/devices` routes return the full management view.

## Pairing request budgets and online state

- In administrator device views, `online` is derived at runtime: the device must be enabled, approved, hold a valid public key, and have an unrevoked, unexpired current-epoch session issued or authenticated via `Device` + nonce within the last 30 seconds (`lastSeenAt` reflects live session activity without writing disk on every request).
- Public registration retains at most **128** `pending` devices. When full, new public keys fail with `429 / device_pairing_capacity` while existing keys can still query their receipt without extending TTL. Direct administrator device creation bypasses the public pending quota.
- Unmanaged pending requests expire **10 minutes** after creation and are lazily pruned and persisted during startup, registration, list reads, and approval operations. Disabled requests, identities with non-zero `sessionEpoch`, and approved devices never auto-expire so public re-registration cannot bypass disablement or revocation.

## Challenge issuance and session redemption

`POST /v1/device-sessions/challenges` issues a one-time challenge (`201 Created`), and `POST /v1/device-sessions` redeems the client's `loom.device-session.v1` Ed25519 signature for a short-lived (15-minute) opaque `Device` session token (`201 Created`). Loom stores only the SHA-256 hash of the session token and verifies current device authorization at both steps.

Unused challenges are bounded to **256** globally and **4** per device, each valid for **60 seconds**. Expired challenges are pruned before checking authorization and capacity under the same registry lock; capacity exhaustion returns `429` without evicting or extending existing valid challenges.

| Condition | HTTP / `error.code` | Required Client Behavior |
| --- | --- | --- |
| Device ID does not exist | `404 / device_not_found` | Abort issuance; do not auto-create a replacement identity |
| Device is disabled (regardless of prior approval) | `403 / device_disabled` | Abort issuance; do not enter pending-approval polling |
| Device is pending approval and not disabled | `403 / device_not_authorized` | Continue bounded approval-wait polling |
| Device record has no pairing public key | `409 / device_key_missing` | Abort issuance |
| Global unused challenges reached 256 | `429 / device_challenge_capacity` | Redeem an existing challenge or retry with bounded backoff after expiry |
| Per-device unused challenges reached 4 | `429 / device_challenge_device_capacity` | Redeem an existing challenge or retry with bounded backoff after expiry |

Authorization errors take precedence over capacity errors (`device_disabled` precedes `device_not_authorized`), and global capacity takes precedence over per-device capacity. Disabling a device that already holds an active Live media WebSocket closes the stream with WebSocket policy code `1008` and reason `live_media_device_revoked`.
