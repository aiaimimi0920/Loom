# HTTP request body admission

The daemon's read worker validates complete HTTP headers before accepting more
body bytes. Host, Origin, browser-context, and JSON content-type policy use the
same checks as route dispatch. At most one 64 KiB read chunk may already contain
body bytes when the header terminator is found.

Routes with a body allowance larger than the ordinary request limit additionally
require header-only authentication before upload:

- A valid administrator bearer or administrator cookie admits the body.
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
over stale device credentials.

The reader no longer reserves a declared `Content-Length`. Buffer growth follows
bytes actually received, with fallible incremental allocation and existing route
size limits. Amortized growth is capped at the validated declared end rather
than doubling a full 96 MiB upload to 128 MiB; excess framing can add at most one
read chunk before rejection. The bounded read worker pool, per-peer admission, queue limits,
per-read timeout and 30-second total request deadline remain in force. Shutdown
and refusal drains also no longer preallocate a declared upload; their existing
short time budgets remain unchanged.

Tests in `apps/daemon/src/tests/http_body_admission.rs` exercise real sockets
that withhold the entire large body, invalid Host/Origin, device scope, valid
admin requests, and nonce preservation across preflight and dispatch.
