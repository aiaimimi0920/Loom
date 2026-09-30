# Loom account login v1

Status: implemented source boundary; packaged cross-product acceptance is tracked
in the Neuro QR projection plan. Networking/signaling is a later phase.

Loom owns the user's local account session and a separate Ed25519 device key.
Platform remains the account authority and continues using its existing Linux.do
Web login. Hook pairing identities are independent. Hook, Art processes, and
the desktop WebView never receive the account private key or PKCE verifier.

## Authorization

1. The user selects a Platform HTTPS origin and device name in Loom settings.
2. The daemon generates a 32-byte random request ID, a random PKCE verifier,
   and a fresh Ed25519 key. A pending request expires after 10 minutes.
3. Loom opens `/loom/authorize` in the system browser. Its query contains
   `requestId` (64 lowercase hex), `codeChallenge` (SHA-256 of the ASCII verifier,
   unpadded base64url), `publicKey` (32 bytes, canonical standard base64),
   `deviceName` (at most 80 UTF-16 units), and `expiresAtMs`.
   The desktop command re-reads the pending local daemon state and checks the
   request ID, origin, fixed path and parameter set before opening the browser.
   It accepts a request ID rather than an arbitrary external URL from the UI.
4. Platform requires its existing Web login and explicit approval. The page
   displays the account, device name, and the first 16 lowercase hex characters
   of SHA-256(public key bytes), matching Loom. Approval is a CSRF-protected
   Next.js server action. A browser-account change requires a new confirmation.
5. Loom polls the public BFF at most once per five seconds, sending its verifier
   and device signature. Platform binds one device to the approving account.
   An exchange retry returns that same device, including after response loss.

Authorization links are for the user who initiated login; they are not QR
projection invitations. They cannot change Hook's login or server configuration.

## Public Platform BFF

All requests use POST JSON, HTTPS and no redirects. Explicit loopback HTTP is
accepted for isolated development. The BFF proxies only fixed paths to the
internal account API; it never trusts caller user IDs, cookies, or internal
headers as native device authorization. Bodies are limited to 4096 bytes;
responses are `Cache-Control: no-store` and native reads are limited to 16 KiB.

| Route | Body | Result |
| --- | --- | --- |
| `/api/loom/account/exchange` | `requestId`, `codeVerifier`, `signature` | `pending` or `signed_in` plus `session` |
| `/api/loom/account/status` | device proof | `signed_in` plus `session` |
| `/api/loom/account/revoke` | device proof | `signed_out` |

The `session` contains `protocol=neuro.loom-account.v1`, `deviceId` (UUID),
`accountId` (Platform's canonical user ID), `username`, `deviceName`,
`publicKey`, and `expiresAtMs`. These are public identity metadata, not bearer
credentials. Every authenticated request requires possession of the device key.
The BFF never exposes its account-api internal token.

An exchange signs these UTF-8 bytes, with LF separators and no trailing LF:

```text
neuro.loom-account.v1
exchange
<requestId>
<codeChallenge>
```

A device proof contains `deviceId`, `timestampMs`, `nonce` (16 random bytes as
32 lowercase hex), and `signature` (64 bytes, canonical standard base64). Its
signed message is:

```text
neuro.loom-account.v1
<status or revoke>
<deviceId>
<timestampMs as decimal integer>
<nonce>
```

The server admits 60 seconds of clock skew and atomically consumes each nonce.
Signatures are bound to the operation; status signatures cannot revoke a device.
A different device on the same account cannot authenticate for its peer.

## State, expiry and logout

Platform account-domain owns atomic Redis/Valkey grant, session, quota, and
replay state. All keys expire. Each account has at most eight unexpired grants
and sixteen authorized device sessions; a device admits at most sixty verified
proof attempts per minute. Authorization lasts 30 days without silent lifetime
extension. Expiry or lost server state requires another browser authorization.
Server processes may restart while the shared Redis state remains available.

There is no new bearer refresh-token flow: Loom restores the protected device
identity and proves key possession when checking its account session. Future
signaling must use an explicitly scoped proof contract and honor revocation.
Web login and existing Platform account APIs do not accept these device proofs.

On Windows, Loom persists pending verifier and device key using current-user
DPAPI in a separate `account-login` store under the control plane. Plugin
credential APIs continue using their own root. Other OSes fail closed until a
secure account store is implemented. Successful exchange discards the verifier.
Logout attempts server revocation, then removes local credentials. If offline,
Loom reports that the center record remains until expiry; the removed local key
cannot be reused to restore login. No active network transport exists in this
phase. Network transport teardown and durable remote cleanup belong to P6.

## Local Loom API

POST `/v1/account/{start,status,poll,refresh,logout}` uses the existing local
administrator authentication. Hook device sessions cannot invoke these routes.
`start` takes `origin` and `deviceName`; `poll` takes `requestId`; other actions
take `{}`. Responses contain safe UI views only. A new start requires the
previous login to be canceled or logged out. A stale poll cannot consume the
replacement request. Closed UI components stop polling and ignore stale replies.

The daemon's existing serialized route policy protects account transitions.
Outbound requests have an eight-second timeout; there is no account background
thread. The later network worker must not run its connection loop in a route.
