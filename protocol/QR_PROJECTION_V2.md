# QR projection v2: local Loom ownership and central coordination

Status: central coordination, per-daemon native transport ownership, durable
source/receiver snapshots, background synchronization and Hook v2 integration
are implemented. Focused checks cover real Iroh transfers, lost publish replies,
restart recovery, account isolation and invitation expiry. Self-hosted relay,
two-machine networking and real OAuth acceptance remain open. The existing v1
shared-Loom protocol keeps its original meaning. No v1 origin is interpreted as
an account login endpoint.

## Ownership and transport

Hook sends formal PNG snapshots to its own authenticated Loom. Loom owns source
revisions, persisted associations, account authorization, network recovery and
peer connections. Hook retains image composition and the receiver's local layout.

Platform's account domain owns invitation authorization, receiver binding,
source revision metadata and short-lived endpoint presence. It does not store
or relay plaintext images. Both devices must already belong to the same account.

The selected transport candidate is Iroh 1.2.0 (MIT OR Apache-2.0, Rust 1.91).
Its QUIC TLS identity uses the Loom device's existing Ed25519 key. Direct UDP
paths, NAT traversal and encrypted relay paths share that authenticated identity.
Loom supplies endpoint addresses from Platform, disables public address lookup
and port mapping, and uses only relay URLs supplied by trusted Platform policy.
Self-hosted relay deployment and real Windows networking remain acceptance gates.
The relay can route encrypted packets but cannot decrypt the application stream.

## Signed central requests

POST `/api/loom/projections` is a bounded Web BFF for
`/internal/loom-projections`. It forwards no caller identity headers or cookies.
The request contains the existing device proof fields plus `payload`, a JSON
string no longer than 12 KiB. The whole HTTP body is at most 16 KiB.

The device signs these exact UTF-8 bytes, with LF separators and no trailing LF:

```text
neuro.loom-account.v1
projection
<deviceId>
<timestampMs>
<nonce>
<lowercase hex SHA-256 of the exact payload UTF-8 bytes>
```

The server verifies the live account session, clock window, signature, nonce,
body binding and operation scope before parsing the operation. Projection proofs
have their own bounded request rate; account status and logout budgets remain
separate. A status proof cannot authorize a projection request.

## Invitation and lifecycle

The v2 envelope retains the v1 projection ID, source session/Unit/revision,
content kind/digest, origin, expiry, nonce and signature fields. Its protocol is
`neuro.qr-projection.v2`; source additionally carries `accountId` and `publicKey`.
The source session ID is the immutable source epoch. The signature binds every
field; the public key must match the source's registered Loom account session.
No account credential, private key or plaintext image enters the QR code.

The invitation signs the following exact UTF-8 fields, joined with LF and no
trailing LF. Fields are ASCII, identifiers contain only `A-Za-z0-9._:/-`, and
numbers are positive safe decimal integers. The public key uses canonical
standard base64; the signature value uses canonical unpadded base64url (64 bytes).

```text
neuro.qr-projection.v2
<projectionId>
<serverOrigin>
<source.deviceId>
<source.accountId>
<source.publicKey>
<source.sessionId>
<source.unitId>
<source.revision>
<content.kind>
<content.digest>
<expiresAtMs>
<nonce>
ed25519
<signature.keyId>
```

`projectionId` is `projection:` followed by 32 lowercase hex digits. Nonce is
32 lowercase hex digits. Device and signature key IDs are the same account
device UUID. Content digest is 64 lowercase SHA-256 hex digits. Initial source
revision is 1; the signed invitation remains immutable when the source publishes
later revisions. No JSON reserialization is used for invitation signed bytes.

Central operations are:

- `configuration`: return the trusted relay policy.
- `create`: register one source invitation and its initial image metadata.
- `inspect`: validate the QR and authorize same-account preview while unconsumed.
- `accept`: atomically bind one receiver device and local Unit to the inspected
  revision/digest; exact retries recover the same binding.
- `publish`: source-only compare-and-swap of revision/digest/dimensions. The
  source Loom chooses the next revision and persists content before publishing.
- `read`: return source/receiver state after checking both device authorizations.
- `unlink`: stop the association; an old invitation cannot resurrect it.
- `sync`: refresh endpoint presence and return bounded associations owned by the
  caller, including the authorized peer's current address. This tells A about B.
- `peer`: let the source verify an incoming transport identity for a preview or
  accepted receiver. TLS peer public key must match the returned device identity.

An invitation lasts five minutes. Accepted associations retain source epoch and
revision across process restart. Server records expire no later than the source
account session. Revoked/expired devices cannot reconnect or receive new content.
Local logout immediately invalidates the network generation and closes peers;
remote authorization is revalidated on bounded leases. Account replacement never
adopts associations from the old device identity.

## Central operation contract and bounds

The exact TypeScript union is `Platform/packages/contracts/src/loom-projection.ts`.
Every operation has `kind`; no unknown fields or image bytes are accepted.

| Kind | Additional input | Result |
| --- | --- | --- |
| `configuration` | none | `kind=configuration`, `policy` |
| `create` | `envelope`, `width`, `height`, `byteLength` | `kind=projection`, `view` |
| `inspect` | `envelope` | projection view with source identity/address |
| `accept` | `envelope`, `receiverUnitId`, `expectedRevision`, `expectedDigest`, `confirmed=true` | projection view |
| `publish` | `projectionId`, `sourceSessionId`, `priorRevision`, `revision`, `digest`, `width`, `height`, `byteLength` | projection view |
| `read`, `unlink` | `projectionId` | projection view |
| `sync` | `endpoint` | `kind=sync`, bounded `views` |
| `peer` | `projectionId`, `peerDeviceId`, `peerPublicKey`; `envelope` required for unaccepted preview | `kind=peer`, `peer`, `authorizedUntilMs` |

A view contains the immutable invitation, initial image dimensions/byte length,
latest image metadata, `invited|linked|stopped` status, receiver device/Unit and
original accepted revision/digest, record expiry, update time, availability,
authorization lease end, and optional peer. It contains no PNG or account secret.
Exact accept retries retain the original accepted revision/digest even after
later source changes or invitation expiry. All mutations use record CAS;
account-session fences in the same transaction reject concurrent revocation.
Unlink keeps a tombstone until source-session expiry, including when the peer
was revoked. Existing source records retain their original expiry.

`endpoint` contains `endpointId` (32 public-key bytes as lowercase hex), up to
eight `{ip,port}` addresses and nullable `relayUrl`. Hostnames, multicast,
unspecified and IPv4-mapped IPv6 addresses are rejected; loopback addresses are
limited to an explicitly configured loopback HTTP development origin. Only the
authenticated device may advertise its endpoint ID. A peer's actual QUIC TLS
public key must be passed to `peer`, never copied from an untrusted stream body.

The service uses 15-second sync intervals, 45-second presence expiry, and at
most 30-second authorization leases, further capped by device and invitation
expiry. Expired presence returns no address. Revoked peer sessions make their
associations unavailable while other associations continue to sync. Native
workers must discard expired leases and reauthorize before any content transfer.

Each device has 240 projection proof attempts/minute, separate from 60 account
status/logout attempts/minute; nonce replay protection is shared. Each account
has at most 64 unexpired records including stopped tombstones. Retention ends
with the source's 30-day session, preventing unlimited create/unlink churn.
Payloads are at most 12 KiB, HTTP bodies 16 KiB; native sync response reads must
be capped at 256 KiB. Each device has one presence record. Per-request session
and presence lookups are cached by device and bounded by the record/device limits.

`401 device_session_unavailable` means the caller's own session is unavailable.
`409 projection_peer_unavailable` only stops/pauses the affected projection;
it must not log out the surviving device. A known stopped record returns an
unavailable view and cannot be recreated or rebound. Unknown IDs return 404;
revision, binding or CAS contention returns 409; exhausted budgets return 429.

Deployment policy requires `LOOM_PROJECTION_PUBLIC_ORIGIN`, a canonical HTTPS
origin (loopback HTTP only for development), and `LOOM_PROJECTION_RELAY_URLS`,
a JSON array of up to four canonical HTTPS relay origin URLs ending in `/`.
An empty relay list permits direct-only development. Invalid/missing policy
returns `503 projection_not_configured`; no public relay/discovery default is
installed. Relay policy is rechecked when returning cached presence.

## Local API and image transfer

New `/v1/projections/v2/*` APIs require existing local Hook device authorization.
They preserve create/inspect/accept/update/read/unlink semantics while resolving
account and remote transport in Loom. Context reports account/login readiness.
An invitation origin must match the already trusted account origin before any
outbound request; scanning a new QR does not start an account login.

Each local request includes `kind`, matching the final route segment. Unknown
fields are rejected, including on the empty `context` operation. The local
operations are deliberately separate from central coordination operations:

| Kind | Additional local input |
| --- | --- |
| `context` | none; returns signed-in account/device identity, central origin, projection protocol and policy |
| `create` | `unitId`, `contentKind`, `snapshot` |
| `inspect` | `envelope` |
| `accept` | `envelope`, `receiverUnitId`, `expectedRevision`, `expectedDigest`, `confirmed=true` |
| `update` | `projectionId`, `sourceSessionId`, `priorRevision`, `revision`, `snapshot` |
| `read` | `projectionId`, `knownRevision` |
| `unlink` | `projectionId` |

`snapshot` contains `imageBase64`, `width` and `height`; the local HTTP body is
limited to 6 MiB. Loom validates the actual PNG, computes its digest, signs a
source invitation and persists the image before publishing metadata. Hook
never supplies account proof or endpoint addresses. Raw central `configuration`,
`publish`, `sync` and `peer` operations are not exposed as local routes.

Content replies retain `envelope`, `revision`, `digest`, `linked`,
`receiverDeviceId`, `receiverUnitId`, nullable `snapshot`, `transport` and
nullable `error`. Transport is `direct`, `relay`, `reconnecting` or `offline`;
direct/relay comes from the selected Iroh connection path. Cached reads at the
known revision omit the PNG. Hook v2 invokes only the loopback Loom advertised
by its local manifest; the QR central origin is never a Hook HTTP destination.
New invitations use v2 and require a Loom account. Saved v1 invitations and
associations keep the shared-origin v1 command, including deferred unlink.

One account generation owns an endpoint, a bounded receive task set and one
background synchronization loop. The daemon reconciles account state every
five seconds and can restore networking before Hook opens a projection panel.
Logout closes the generation before attempting remote revocation. Namespace
selection binds the central origin, account, device ID and public key; a new
identity cannot adopt another identity's persisted associations. Local entries
also bind the authenticated Hook actor and Unit.

The durable store retains only the current and optional pending PNG, with
64 entries, eight active sources, a 48 MiB PNG budget and a 12 MiB serialized
entry budget. A stopped entry retains its final bounded snapshot and tombstone.
Publish retries first reconcile the central revision/digest; an acknowledged
publish with a lost reply is committed locally without a second publication.
Sync responses cannot roll metadata back over a newer publish or acceptance;
unavailable/stopped authorization still takes precedence. Expired invitations
are re-read centrally before retirement so an acceptance just before expiry is
preserved, and the sender can create a fresh invitation after an unused expiry.

Peer streams negotiate `neuro/qr-projection/2`. Each bounded request identifies
the projection and receiver; the source authorizes the actual TLS peer identity
before returning an image. Responses bind projection ID, source epoch, revision,
digest and dimensions. PNG limits remain 4 MiB, 8192 per dimension and 16,777,216
pixels. The receiver independently verifies decoded image properties and digest.
Transfers have a 15-second deadline, at most four incoming handlers and a
per-projection cancellation signal. Authorization lease expiry fences writes;
unlink cancels active source sends. Transport changes cannot roll back a
revision, change its digest or bypass a stopped association.

## Implementation and acceptance order

1. Central signed operations and atomic lifecycle, with isolated Redis tests.
2. Local Loom ownership, durable source/receiver state and Hook v2 integration.
3. Iroh Windows direct and self-hosted relay tests, restart/cancellation and
   blocked-UDP fallback; then actual two-machine NAT acceptance.
4. Fresh Hook/Loom candidates, Platform deployment and real OAuth/UI acceptance.

Passing an earlier step does not mark later steps or the complete QR goal done.
