# HTTP request body admission

For routes permitting bodies above the ordinary request limit, the daemon's
read worker validates complete HTTP headers before accepting more body bytes.
Host, Origin, browser-context, and JSON content-type policy use the same checks
as route dispatch. Small-body and WebSocket routes retain their existing
dispatch-owned policy. At most one 64 KiB read chunk may already contain body
bytes when the header terminator is found.

Routes with a body allowance larger than the ordinary request limit additionally
require header-only authentication before upload:

- A valid administrator bearer or administrator cookie admits bodies on routes
  that accept administrator authority. Ordinary and offline projection routes
  that require a device actor must pass device preflight even with admin credentials.
- A device credential must name a currently valid, enabled, approved session,
  carry a fresh nonce, and be scoped to the requested route. In particular,
  device sessions do not gain package-install privileges.
- A session whose nonce budget is exhausted is rejected during read-only
  admission too; no new upload can park a worker using that exhausted session.
- Missing/invalid credentials fail without waiting for `Content-Length` bytes.

The two inbound `/v1/projection-peer/raster-check` and
`/v1/projection-peer/transfer` routes instead carry pinned-peer signatures in
their bodies. They retain that separate authentication protocol and their 6 MiB
body limit; they are not converted into administrator/session routes. Their
buffers also grow only with received bytes. Local offline projection transfers
retain their existing device-session scope. Peer trust, replay and content
checks remain mandatory at dispatch.

Device preflight is read-only. It does not consume a nonce or cache authority for
dispatch. After the body arrives, dispatch rechecks expiry, revocation, scope,
and nonce freshness, then consumes the nonce exactly once. Concurrent requests
with the same nonce cannot both execute. Admin credentials retain precedence
over stale device credentials only on routes that accept administrator authority.
Preflight uses nonblocking registry acquisition: contention or a poisoned registry
returns `503 / device_registry_unavailable` without waiting in a socket-read worker.
Clients may retry; no nonce is consumed and no device authorization is changed.

The reader no longer reserves a declared `Content-Length`. Buffer growth follows
bytes actually received, with fallible incremental allocation and existing route
size limits. Amortized growth is capped at the validated declared end rather
than doubling a full 96 MiB upload to 128 MiB; excess framing can add at most one
read chunk before rejection. The bounded read worker pool, per-peer admission, queue limits,
per-read timeout and 30-second total request deadline remain in force. Shutdown
and refusal drains also no longer preallocate a declared upload; their existing
short time budgets remain unchanged.

An early admission rejection is written and write-half-closed immediately.
The read worker then discards at most the remaining declared body using a fixed
64 KiB buffer and a 500 ms total budget (200 ms write timeout). This keeps eager
uploads from turning an ordinary 401/403 into a TCP reset that truncates the
response, without buffering the upload or parking the accept loop. A malicious
peer that outlasts the drain budget is still closed; delivery is not guaranteed
to a peer that refuses to make bounded progress.

Tests in `apps/daemon/src/tests/http_body_admission.rs` exercise real sockets
that withhold the entire large body, invalid Host/Origin, device scope, valid
admin requests, and nonce preservation across preflight and dispatch.
