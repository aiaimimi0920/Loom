# Native local bridge transport

Implementation status: the transport primitive, daemon lifecycle publication,
and Hook native callers/extension adapter are implemented on the development
branch. Hook browser preview is local-only. Loom Desktop now uses a fixed native
subscription. Operational probes use a native TLS test adapter; packaged applications
and their operational smokes must still be jointly verified before this
document establishes an end-to-end production security guarantee.

## Discovery and ownership

The private `loom.json` discovery manifest contains a `hookBridge` object while
the bridge is running, and `null` while it is stopped. Its fields are:

- `protocol`: exactly `loom.local-bridge.v1`;
- `instanceId`: fresh UUID for each bridge start;
- `endpoint`: canonical `wss://127.0.0.1:<assigned-port>/`;
- `certificateDerBase64`: standard Base64 of the self-signed leaf DER certificate;
- `certificateSha256`: lowercase hexadecimal SHA-256 of that exact DER;
- `authToken`: 32 cryptographically random bytes represented as 64 hex characters.

The existing HTTP `transport` and its administrator credential are independent.
Bridge credentials must never be published through status HTTP, logs, WebView
DTOs, URLs, query strings, or a general-purpose token getter. Private keys stay
in native server memory. A stop/start rotates key, certificate, token and instance.
Private manifest ACLs do not protect against arbitrary same-user process memory
access; that threat is outside this discovery boundary.

## Connection sequence

1. Native code reads a bounded private manifest and validates the descriptor.
2. Connect directly to its loopback literal without DNS or a proxy.
3. Negotiate TLS 1.3 using Rustls with only the manifest certificate as a trust
   anchor. Standard WebPKI validates the `localhost` name, usage and handshake
   signature. Compare the presented leaf DER to the manifest certificate too.
4. Derive 32 bytes with the TLS 1.3 exporter, label
   `EXPORTER-Loom-Local-Bridge-v1`, context UTF-8 `<instanceId>:<authToken>`.
   Only after TLS authentication send the WebSocket upgrade with
   `Authorization: LoomBridgeProof <lowercase-hex-exporter-output>` and
   `X-Loom-Bridge-Instance: <instanceId>`. The server derives and checks the same
   connection-bound proof. The raw token never enters HTTP headers: dependency
   TRACE logging can expose serialized headers regardless of sensitive flags.
   A logged proof cannot be replayed on a different TLS connection.
5. Reject invalid token/instance, browser `Origin`, non-root path and queries
   before upgrading or dispatching any application message.
6. The existing Hook/extension protocol handshakes follow on this authenticated
   socket. They remain protocol negotiation, not a substitute for authentication.

The combined TLS/upgrade handshake has one three-second absolute deadline;
per-read timeouts must not let slow-trickle peers extend it. Received frames
and messages are limited to 64 MiB; write buffering is bounded to 65 MiB.
Native application send entry points must separately enforce their payload limits.
Connection admission, broadcast queues and WebView IPC require their own bounds.

The daemon admits at most 32 tracked bridge connections, including pending
authentication. Each broadcast subscriber queues at most 128 messages and
64 MiB total; producers never wait for a slow subscriber. Exceeding either
budget evicts that subscriber and its next receive reports disconnection,
rather than silently dropping selected messages. Cursor history retains at most
2048 entries and 64 MiB; eviction or an oversized skipped event requires snapshot
recovery. These bounds do not substitute for application operation deadlines.

## Integration requirements

Daemon start must publish the actual bound port atomically before reporting
success. Publication failure must leave no live new listener/credential. Stop
must close and join connections and revoke the manifest object. An unexpected
manifest publication failure must not retain a usable stopped identity.
Daemon shutdown drains request workers before revoking and stopping the bridge,
so a queued start request cannot recreate it after final revocation. Each tracked
connection owns a TCP interrupt handle; stop shuts down all sockets before joining
workers, including authenticated peers holding an incomplete fragmented message.
This stop guarantee is not an application message lifetime deadline.

Hook and Loom Desktop WebViews use narrow native IPC, not browser WebSockets or
an arbitrary URL proxy. Native owns persistent sockets and credentials. IPC
must allowlist methods, bound pending work and payloads, and fence late events
by connection epoch. Hook's disabled startup integration policy must reject
both long-lived listeners and direct one-shot connection entry points.

Loom Desktop reads `%APPDATA%/Neuro/capabilities/loom.json` in native code.
Its main-window-only read command exposes connection state and decimal-string
epoch/revision counters, never discovery credentials or arbitrary bridge methods.
One owned worker authenticates, confirms the Hook handshake, then subscribes only
to workflow/capability updates. Hook handshake and subscription confirmations each
have a three-second absolute deadline. Live reads use 250 ms operation deadlines;
an interrupt handle and joined worker precede daemon shutdown. These are per-read
bounds, not a total lifetime bound for an entire fragmented WebSocket message.
Reconnection rereads the manifest and increments the epoch after confirmation;
the UI refreshes snapshots on epoch change because this subscription does not replay
cursor history. Browser preview has no bridge socket or retry timer.

PowerShell plugin-boundary and framework/store smokes use the test-only
`bridge_probe` Cargo example with an explicit per-run private manifest. Its
sequential send/receive commands retain real message ordering, enforce 1 MiB
payload limits and one absolute receive deadline across all fragments. The parent
owns the process, closes stdin and uses bounded wait/termination on failures.
No probe executable or arbitrary proxy is added to the user-facing desktop package.

Mixed versions fail closed: no anonymous/plaintext fallback. Update Hook and
Loom together; rollback requires rolling both applications back as a pair and
acknowledging that the older transport does not supply this security boundary.
User workspaces and stored assets are unchanged by the transport migration.
