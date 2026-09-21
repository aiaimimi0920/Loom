# Wall endpoint Live input

This extends the [wall control API](WALL_CONTROL_API.md). Art input and final
multi-terminal acceptance remain in [the plan](../docs/TILE_WALL_IMPLEMENTATION_PLAN.md).

## Paired-device routes

| Method and path | Body | Authority |
| --- | --- | --- |
| `POST /v1/walls/control/acquire` | `{binding, pixel, pointerId}` | Current applied presenter hitting interactive Live content |
| `POST /v1/walls/control/renew` | `{binding, controlId}` | Exact endpoint controller |
| `POST /v1/walls/control/release` | `{binding, controlId}` | Exact endpoint controller; idempotent |
| `POST /v1/walls/input` | `{control, sequence, event}` | Exact endpoint controller and next reliable sequence |

`binding` is `{endpointId, leaseId, revision}`. `control` adds the server-issued
`controlId` to that binding. Bodies reject unknown fields and exceed neither
4096 bytes nor the public geometry limits. Administrator credentials cannot act
as a terminal, and a paired device cannot borrow another endpoint's binding.

Acquire accepts an in-bounds native pixel and a `u32` pointer identity. The
presenter must have acknowledged this exact layout revision. Loom uses the same
half-open, rotated and cropped hit test as rendering, including noninteractive
placements blocking input. A grant returns `protocolVersion`, `controlId`,
`endpointId`, `revision`, `placementId`, `sessionId` and `leaseTtlMs: 4000`.
This volatile controller is independent of both the presenter heartbeat
sequence and the device-wide Live sequence.

One endpoint controls one placement at a time. One Live source has one
controller across ordinary viewers and all wall endpoints, including outputs
sharing the source device identity. A contender receives HTTP 409 and must wait
for release or expiry; acquisition never steals another controller. Moving or
duplicating a placement does not start a program or capture session.

## Reliable input

Sequence starts at 1 and advances by exactly one. Supported events are:

- `{kind: "move", pixel, pointerId}`;
- `{kind: "button", pixel, pointerId, button, state, clickCount}` with left,
  middle or right buttons, pressed/released state and click count 1 or 2;
- `{kind: "wheel", pixel, deltaX, deltaY}` with exactly one nonzero axis and
  magnitude at most 1200;
- `{kind: "key", virtualKey, state}` with virtual keys 1 through 254 and at most
  32 held keys. Text composition, IME and native multi-touch/pen are not advertised.

Loom retains the real controller device identity when forwarding into the
existing ordered Live source stream. Pointer identity changes, leaving the
captured placement, bad sequences, unsupported edges and stale layout bindings
revoke the exact owner. Cross-tile gestures cancel rather than guessing a new
pointer identity. Renew validates source freshness, device token/approval,
presenter lease, layout and visibility again. Background pruning releases owners
on layout changes, source disconnect, device revocation and lease expiry without
waiting for terminal cooperation. Release events make the native source attempt
bounded key/button cleanup before another controller is admitted.

The Hook client keeps one serial queue with at most 64 entries, coalesces only
adjacent moves, discards samples waiting over 1500 ms and never retries a rejected
edge. Native input HTTP calls have a two-second timeout and 8192-byte response
limit. Blur, pointer cancellation, unavailable media and terminal shutdown cancel
the queue and release authority. A 500 ms independent output monitor invalidates
display/input while control I/O is stalled; presentation clears after six seconds
without a successful authorization refresh. Authority feedback is noninteractive
and does not obscure the output.

The source's `GET /v1/live/sessions/{id}/events` long-poll runs concurrently with
serialized control mutations. It waits on the Live store's own condition variable
without holding the daemon-wide route lock, so an input/control write can wake
the poll immediately instead of waiting behind its one-second timeout.

## Verification

`cargo test --locked -p loom-daemon --lib wall_input` exercises real loopback
HTTP, signed pairing, two outputs on one device, source-coordinate translation,
sequence/replay rejection, contention, layout replacement, token revocation and
expiry. Native source and physical output evidence is recorded separately; these
protocol tests do not establish multi-computer or physical display acceptance.
