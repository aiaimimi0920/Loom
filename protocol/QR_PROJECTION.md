# QR projection protocol v1

The QR projection feature lets device A offer one sticker or Art block to device
B. The QR carries only a signed, expiring `neuro.qr-projection.v1` envelope. It
does not carry pixels, credentials, executable paths, or a permission list.
Its signed `serverOrigin` carries a bounded HTTPS origin, with HTTP allowed only
for loopback verification. Paths, credentials, query strings, fragments and
redirects are forbidden. The schema is `schemas/qr-projection.v1.schema.json`.

Device A creates a fresh `projectionId` and one-time `nonce`, stores a bounded
rendezvous record in Loom, and signs the envelope with the paired device key.
The record contains the source device/session identity, source unit ID and
revision, content kind, and a SHA-256 digest. The rendezvous reference is an
opaque identifier and is never resolved by Hook directly.

Device B parses the QR locally and confirms its server before any network request.
The origin applies only to this association. With an approved device session,
`inspect` validates the invitation and returns source identity and a bounded PNG
preview. B separately confirms the content. `accept` atomically binds the invite
to a receiver device, authorization epoch and `receiverUnitId`, then transfers
the latest validated snapshot. A lost response can be recovered only with the
same device, epoch and target unit. Other attempts cannot consume it again.
The receiver creates a linked sticker with source identity and last-applied
revision metadata.

Subsequent source updates are accepted only from the same paired source session,
with a strictly newer revision and a matching digest. A stale or conflicting
update is rejected and reported to both devices. Device B may unlink at any
time; unlinking stops future updates and preserves its last formal content.

The fixed POST routes are `/v1/projections/create`, `inspect`, `accept`, `update`,
`read`, and `unlink` under the same prefix. They require device authentication;
an administrator bearer token does not impersonate a projection device.
`read` accepts `knownRevision` and omits pixels when unchanged. `update` uses
`priorRevision` and its successor; an identical successful retry is idempotent.
Source and receiver epochs prevent reauthorization from restoring revoked links.

An unaccepted invitation expires after at most five minutes. Accepted links
survive invite expiry and daemon restart. The signature includes protocol,
projection ID, origin, source device/session/unit/revision, content kind/digest,
expiry and nonce in that order, separated by newlines.

PNG bytes are capped at 4 MiB; the HTTP body at 6 MiB; dimensions at 8192 each
and 16,777,216 pixels total. Storage is capped at 64 records and 48 MiB, with
eight records per source. Updates are separated by at least 500 ms. Only the
latest snapshot is retained. This transport provides HTTPS rendezvous and
snapshots; LAN discovery, NAT traversal, interactive Art and video are outside v1.

The first implementation must keep the envelope and transfer record bounded,
rate-limited, and auditable. It must reject replay, expiry, signature mismatch,
cross-session use, unknown projection IDs, digest mismatch, and source revision
rollback before any unit is created or modified.
