# Shared-Loom sticker delivery

This extension of QR projection v1 targets a paired Hook without official login.
It sends a continuously synchronized PNG projection, not a one-shot copy. Existing
untargeted QR invitations and their signature format remain unchanged.

## Device-scoped API

All routes are POST, use the existing Device session and fresh nonce, and reject
administrator-only authentication. Only approved, enabled, keyed devices qualify.
These routes do not grant wall/shared-image administration privileges.

- /v1/projections/inbox: policy is confirm, auto, or disabled.
  Refreshes receiver presence and returns at most 64 metadata-only invitations.
  Presence expires after 30 seconds and is not restored after a daemon restart.
  Disabled removes presence. The receiver chooses the policy; automatic acceptance
  runs in Hook and still calls the normal authenticated acceptance API.
- /v1/projections/targets: empty object. Returns at most 64 other active receivers
  with deviceId, name, policy, and route=shared_loom. Private keys, addresses,
  tokens and the complete managed-device registry are never returned. Signed
  offline peer directory entries may also appear with route=offline_peer and
  deliveryAvailable=true and transferProtocol=loom.offline-transfer.v1; send them
  through /v1/offline-projections/create, never shared-Loom v1 create. See
  OFFLINE_PROJECTION_PEERS.md for metadata, freshness and partial-result status.
- /v1/projections/create: existing envelope/snapshot plus optional targetDeviceId.
  The destination must have active presence. Creation durably and atomically binds
  the target ID and authorization epoch. An omitted target keeps the QR invitation
  behavior; a targeted invitation cannot be claimed by outsiders.
- /v1/projections/receipt: projectionId and status=displayed or rejected.
  Only the epoch-bound receiver can submit receipts. Display requires an accepted
  binding; rejection requires a pending unexpired invitation and prevents later
  acceptance. Identical retries are idempotent. Rejected source reads return
  projection_rejected rather than claiming successful delivery.

Content responses optionally include delivery with targetDeviceId, targetEpoch
and status: awaiting_confirmation, accepted, displayed, or rejected. Existing
revision, snapshot validation, update rate, source quotas, revocation, unlink and
expiry rules still apply. Accepted invitations remain in the inbox until a display
receipt, enabling recovery after a lost response.

## Receiver lifecycle

Hook explicitly enables a single shared Loom origin and receive policy. Startup
polling waits for session restoration. It uses bounded sequential work, finite
backoff and round-robin acceptance so one failed display cannot starve others.
Confirmation prompts do not repeatedly reopen after dismissal. The existing
receive panel lists outstanding invitations and provides accept/reject actions.

The projection ID determines the local sticker ID. Retries reuse existing content
and preserve receiver layout. Workspace replacement suspends polling until settings
are saved again. Late responses after workspace replacement, deletion or disposal
cannot attach stickers; abandoned accepted links are queued for unlink.

Hook reports displayed only after its save/sync barrier completes and the matching
image element has loaded and has a visible nonzero frame. This reports application
rendering, not proof that a person saw the physical monitor or that no other window
covered it. Save/render failure leaves the invitation recoverable as accepted.

## Reserved official integration

Targets expose capability flags: sharedLoom=true, offlinePeerDirectory=true, offlinePeers=true,
officialAccount=false, officialRelay=false. Unsupported routes are not silently
selected. Offline peer trust, signed discovery and cross-Loom image delivery are
implemented. Official account/friend providers remain reserved integration work.
Official relay requires server-issued entitlement/session
authorization and resource accounting; no client flag enables premium relay.

## Verification

The daemon projection HTTP suite exercises real signed pairing sessions, outsider
denial, receipt ordering, idempotent acceptance and receipts, rejection, disabled
presence, and restart recovery. Hook tests exercise target selection, confirm/auto,
late-response cleanup, save failure, hidden images, and duplicate suppression.
This source-level evidence does not close the two-native-Hook/PC3 release gate.
