# Offline Loom peer control plane

Status: manual trust, mutual connectivity proof, signed remote Hook directory and
offline Hook delivery are implemented. Diagnostic probe/raster responses retain
deliveryAvailable=false because they do not deliver to a Hook. No official account,
server, public relay, discovery service or transitive trust is consulted.

## Administration

GET /v1/projection-peers returns identity {peerId, publicKey}, revision, peers,
and deliveryAvailable. The administrator bearer is required; Hook device sessions
cannot read or change this configuration. The private identity key is never returned.

PUT /v1/projection-peers accepts expectedRevision and peer:

    {"peerId":"loom-<SHA256 of raw Ed25519 key>","publicKey":"<base64 32-byte key>",
     "name":"Loom B","origin":"https://loom-b.example.test","enabled":true}

The key-derived peerId is the fingerprint. Obtain the public identity from the
other Loom administrator over an independently trusted channel. Register both
directions explicitly. An endpoint or display-name change preserves the pinned
key; key rotation requires removing the old peer and registering the new identity.
enabled=false disables verification without deleting the entry.

DELETE /v1/projection-peers accepts expectedRevision and peerId. Writes compare
the current revision, atomically persist the next document, then publish it in
memory. A stale revision returns peer_revision_conflict; reload before retrying.
No network response is allowed to mutate the trust list.

POST /v1/projection-peers/probe accepts peerId. A successful result contains
verified=true, the pinned peerId, configuration revision, and deliveryAvailable=false.
This only proves a current signed round trip to a mutually trusted peer. It grants
no Hook impersonation, image read, device-list or content-receipt authority.

## Executable setup helper

Run scripts/Invoke-LoomOfflinePeer.ps1 on an administrator workstation with
PowerShell 5.1 or newer. Credentials are SecureString parameters, not stored in
identity exports. The identity output path must not already exist.

    $token = Read-Host 'Local Loom administrator token' -AsSecureString
    ./scripts/Invoke-LoomOfflinePeer.ps1 -Action Status -LoomOrigin https://loom-a.example.test -AdminToken $token -IdentityOutputPath ./loom-a-public.json
    ./scripts/Invoke-LoomOfflinePeer.ps1 -Action Trust -LoomOrigin https://loom-a.example.test -AdminToken $token -PeerOrigin https://loom-b.example.test -IdentityPath ./loom-b-public.json -PeerName 'Loom B'
    ./scripts/Invoke-LoomOfflinePeer.ps1 -Action Probe -LoomOrigin https://loom-a.example.test -AdminToken $token -PeerId 'loom-<peer fingerprint>'
    ./scripts/Invoke-LoomOfflinePeer.ps1 -Action Remove -LoomOrigin https://loom-a.example.test -AdminToken $token -PeerId 'loom-<peer fingerprint>'

Export B and register A on B in the same way before probing. Never exchange
administrator tokens or the private offline-projection-peers.json file with a peer.
The helper is an operator interface; a desktop settings panel is not included yet.

## Signed probe

POST /v1/projection-peer/handshake has independent pinned-key authentication.
It accepts sourceId, targetId, nonce, timestampMs and signature. The request
signature is Ed25519 over newline-separated UTF-8 fields:

    loom.offline-peer.v1
    request
    <sourceId>
    <targetId>
    <nonce>
    <timestampMs>

The response echoes the exact challenge and carries a detached signature over
the same fields with purpose=response. The requester checks the pinned public
key, exact challenge, response size, current timestamp and unchanged local trust
revision. There is no request-signature/response-signature reflection shortcut.
The configured peer must exist and be enabled on both sides. A wrong endpoint
cannot silently replace a pinned identity.

Limits: 16 peers, 128-byte UTF-8 names, 256-byte origins, 16 KiB request/response,
one outbound probe per daemon, five-second HTTP timeout, no redirects. HTTPS with
normal certificate validation is required outside loopback; private-network HTTPS
is allowed. Install the appropriate LAN CA rather than bypass certificate checks.
Clocks must be within 30 seconds. At most 128 accepted nonces are retained until
their timestamp ceases to be valid; this cache is in memory, so restart can permit
a repeated still-valid connectivity challenge. It cannot authorize data transfer.

## Raster transport check

POST /v1/projection-peers/raster-check is administrator-only and accepts peerId
and snapshot {imageBase64, width, height}. It validates a PNG locally, then sends
it to the configured peer endpoint /v1/projection-peer/raster-check. This is an
explicit operator transport diagnostic, not a device delivery API.

The signed peer request contains challenge, raster {digest, width, height,
byteLength}, and snapshot. Its challenge purpose is the following newline-joined
string, followed by the usual sourceId, targetId, nonce and timestampMs fields:

    raster-request
    <lowercase SHA256 of PNG bytes>
    <width>
    <height>
    <byteLength>
    not-delivered
    not-retained

The receiver authenticates before PNG decoding, then independently recomputes
the digest, dimensions and byte count. It signs the same metadata with purpose
raster-response and returns challenge, raster, signature, deliveryAvailable=false
and retained=false. The sender verifies the pinned peer signature, exact request
binding, freshness and unchanged local trust revision before reporting
rasterVerified=true. A received PNG is discarded, never added to a Hook inbox,
written to storage or counted as displayed. No source device is impersonated.

Limits: 4 MiB PNG, 6 MiB JSON, 8192 per dimension, 16,777,216 pixels, bounded
PNG decoder allocation, one incoming raster check and one outbound probe per
daemon, 10-second network timeout, 16 KiB response. Both directions fence trust
changes; signed failed requests consume their nonce as well. No lock is held
across network I/O or PNG decoding. Standard TLS/loopback policy remains intact.

Operator example (after mutual Trust):

    ./scripts/Invoke-LoomOfflinePeer.ps1 -Action RasterCheck -LoomOrigin https://loom-a.example.test -AdminToken $token -PeerId 'loom-<peer fingerprint>' -ImagePath ./sample.png

## Persistence and lifetime

The private document lives at settings/offline-projection-peers.json under the
control-plane root, with a 64 KiB read limit, schema and key-pair validation, and
the existing private ACL/atomic-write guarantees. Corruption fails startup rather
than silently regenerating an identity. A sibling lock file prevents two daemons
from concurrently owning the same trust state. Identity and trust survive restart.
Never copy the private document between different Loom machines.

Probe I/O runs outside the trust mutex; configuration changes during a probe
invalidate its success. Remote revocation can only be observed on the next check;
verified is a point-in-time result, not a perpetual authorization or online flag.

## Signed remote directory

POST /v1/projection-peer/catalog uses the pinned-key challenge with purpose
catalog-request. The response signs the echoed challenge with purpose
catalog-response followed by a newline and the SHA256 digest of the JSON tuple
[devices, expiresAtMs, transferProtocol]. The supported transferProtocol is
loom.offline-transfer.v1. Expiry is the requester timestamp plus 5000 ms.
Only approved, current-epoch local Hooks with fresh receive presence and policy
confirm or auto are exported. Catalogs never include transitive peer devices.

Authenticated POST /v1/projections/targets combines local targets with verified
remote entries. Remote entries have route=offline_peer, peerId, peerName,
remoteDeviceId, a peer-target SHA256 composite ID, deliveryAvailable=true and
transferProtocol=loom.offline-transfer.v1. Hook displays the source Loom and sends
through its own paired Loom. These IDs grant no shared-Loom v1 create authority.

The response advertises offlinePeerDirectory=true and offlinePeers=true.
peerDirectory.status is complete, partial, busy or unavailable. Local results
survive peer failures. Limits: 64 combined targets, 192 KiB catalog responses,
two-second peer timeout, at most 16 parallel peer workers, one directory fetch
per daemon. Network I/O holds neither the device registry nor trust mutex.
An unchanged local trust revision and unexpired proof are required at publication.

## Offline delivery lifecycle

Device-authenticated POST /v1/offline-projections/{create,inbox,inspect,accept,read,
update,receipt,unlink} runs on the Hook own paired Loom. Create additionally takes
peerId and remoteDeviceId; targetDeviceId must be the signed-directory composite
hash. Subsequent operations resolve the persisted route by projectionId. The
original source-signed v1 envelope, including serverOrigin, remains unchanged.
Hook persists offlineTransport.origin separately; it never accepts this routing
field from a network response. Offline links do not generate shared-Loom QR codes.

POST /v1/projection-peer/transfer uses a pinned-key challenge and the SHA256 of
the JSON payload, with distinct transfer-request and transfer-response purposes.
The response binds the exact challenge and signs success or failure. The source
Loom attests its paired Hook identity, signature and session epoch. Foreign Hook
identities remain scoped to that trusted Loom and are never registered locally.
Trusting a Loom therefore trusts its assertions about its own devices; it does
not grant it access to another peer namespace or local source records.

Source Loom owns image revisions, acceptance, receipts and stop. Receiver Loom
stores an incoming shadow and rechecks its own Hook epoch before publication.
Confirm and auto policies use the same explicit acceptance operation; displayed
is emitted by Hook only after visible image rendering and session persistence.
Idempotent create/accept/receipt retries recover lost responses. Source stop is
durable before its best-effort peer notification, and receiver polling observes
that stop after reconnect. Accepted records survive both daemon restarts.

Records are private atomic JSON files under offline-projections, bounded to 64
records and 8 per peer/source/direction. Image limits match raster validation.
Unchanged read/inspect polls do not rewrite records. Network I/O and PNG decoding
run outside trust/device/store locks; independent inbound and outbound operation
gates avoid cross-call deadlock.

Each record binds the local global trust revision. Any peer configuration change
(including a name/origin change or delete followed by re-add) permanently
invalidates existing transfers on that Loom. Re-send after finishing trust setup.
Device disable/re-enable also changes its epoch and cannot revive an old transfer.
This conservative policy avoids accidentally restoring revoked access.

Official authentication, friends, rendezvous/NAT traversal and member-only server
relay remain reserved interfaces. They are not involved in offline delivery.
