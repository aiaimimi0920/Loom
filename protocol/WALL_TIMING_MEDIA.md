# Wall scene timing and media v1

Status: source implementation for the Hook v0.2.30.9 iteration. Behavioral,
desktop, release and physical multi-terminal tests are deferred by the user.
The existing r8 acceptance records do not validate this extension.

## Upgrade boundary

Upgrade the daemon, management UI and terminals together. Strict wall readers
now recognize endpoint `scheduledPresentation`, state `timing`, and endpoint
status/heartbeat `scene`. The JSON additions are defined in
[wall.v1.schema.json](schemas/wall.v1.schema.json) and
[wall-timing.v1.schema.json](schemas/wall-timing.v1.schema.json). An absent
`scheduledPresentation` means false. New readers can parse an absent timing
extension; the new Hook terminal requires its clock for timed Live presentation.

The wall media endpoint now negotiates `loom.wall.media.v1`, with NLWM frames.
It does not accept the old wall-viewer NLLV subprotocol. Ordinary source/viewer
`/v1/live/media` connections continue using `loom.live.v1` and NLLV. This change
does not move pixels into wall JSON, Surface state or durable storage.

## Preparing and activating a scene

`PUT /v1/walls/layouts` accepts `{baseRevision, layout, activationDelayMs?}`.
The optional delay is an integer from 0 through 10000 ms, default 0. A positive
delay requires every assigned endpoint to advertise `scheduledPresentation: true`.
Registration is a capability statement, not evidence of physical synchronization.

The complete desired layout is committed immediately using the existing CAS.
This permits media/image/Art preloading and invalidates previous input mappings.
State includes one scene per accessible current layout, with at most 64 scenes:

```json
{
  "timing": {
    "clockId": "boot-scoped-opaque-id",
    "serverTimeMs": 1700000000100,
    "scenes": [{
      "wallId": "wall-a", "revision": 12,
      "preparedAtMs": 1700000000000, "activateAtMs": 1700000005000
    }]
  }
}
```

The daemon anchors `Instant` to Unix milliseconds at store startup. Wall-clock
adjustments cannot change this clock during a boot. Schedules, receipts and clock
identity are volatile. Restart preserves layouts, assigns a new clock identity,
and recreates their scenes with immediate activation; it does not replay an old
delayed command. Clients discard old clock estimates, media and input generations.

A terminal prepares its complete visible image/Live/Art set while keeping the
content planes hidden and input disabled. It shows the complete revision only
after preparation and the conservative activation deadline. An unavailable source
keeps that terminal unacknowledged. This is a deadline, not a distributed barrier:
one late terminal does not hold all other ready terminals indefinitely.

Heartbeat may include the following `scene`; the same receipt appears in that
endpoint's authorized state. No lease or device credential is included in it.

```json
{
  "revision": 12, "prepared": true,
  "appliedAtMs": 1700000005005, "clockUncertaintyMs": 5
}
```

Before application, `appliedAtMs` is null and `appliedRevision` is null. Unknown
clock uncertainty is null. An applied report requires a prepared scene, a current
revision, an uncertainty in 0..1000 ms, and an application time at/after activation
and no later than server time plus uncertainty. The server also rejects any
non-null applied revision before its activation deadline, even without a receipt.
Geometry/capability changes, display controls, expired leases and revoked-device
state cannot retain a previous scene receipt. A heartbeat reports a local paint
selection; it does not prove a physical panel has scanned out those pixels.

## Clock estimation and frame selection

Hook uses the midpoint of each state request's monotonic send/receive timestamps.
It keeps at most eight samples, rejects RTT above 1000 ms, expires samples after
15 seconds, and chooses the lowest-RTT sample. Its uncertainty is RTT/2 plus a
0.0001 ms/ms drift allowance and 1 ms rounding allowance. These are estimator
assumptions; actual clock error and display skew still require measurement.

Activation requires `estimatedServerTime - uncertainty >= activateAtMs`.
Normal control polling is 3000 ms; a pending running scene polls at 250 ms.
The independent output/auth monitor and six-second authorization expiry remain.

Live selection uses the common daemon receive timestamp with an 80 ms buffer:
`target = estimatedServerTime - uncertainty - 80`. Choose the newest due frame;
discard candidates more than 250 ms behind that target. Retain at most three
pending decoded frames plus one selected frame per source. On overflow, preserve
the earliest deadline and newest two frames, dropping an intermediate frame so
high source FPS cannot starve every activation deadline. A faster render loop
reuses its selected frame. Stop showing a held frame after 1000 ms behind target.
No clock estimate means no selected Live frame. A new source epoch drops old pixels.

This policy accommodates different refresh rates and bounds jitter buffers. It
does not provide hardware Frame Lock/Genlock or promise identical physical scanout.

## Negotiation and transport limits

`GET /v1/walls/live/media` keeps its endpoint, device, lease, revision, visibility,
source freshness and nonce checks. Add required `format=raw_bgra|png`:

| Profile | Endpoint capability | Size and rate |
| --- | --- | --- |
| `raw_bgra` | `raw_bgra` | Unscaled, <=16 MiB payload; default 30 fps, `maxFps` 1..60 |
| `png` | `image` | RGB8 PNG, default <=640 x 360 at 10 fps; max 1280 x 720 / 4 MiB |

PNG supports optional positive `maxWidth`, `maxHeight`, `maxFps`, bounded by the
PNG maxima above. Scaling preserves aspect ratio and never upscales. Raw rejects
scaling parameters. Unsupported/absent format or invalid limits return 400;
missing endpoint capability returns 409 `wall_live_codec_unavailable`.

The source remains its existing raw BGRA/sRGB Live capture. PNG uses Loom's existing
PNG dependency and at most two simultaneous encoders across the daemon. Admission
does not wait for another encoder. Each viewer coalesces to the latest source frame
at its negotiated rate; it has no unbounded encode or network queue. A slow socket
keeps the existing 250 ms write deadline. Wall viewers share the 65-media-worker
budget and accept only bounded WebSocket liveness control messages in return.

Missing, explicitly closed and disconnected/stalled sources are distinguished as
`wall_live_source_missing` (404), `wall_live_source_closed` (410), and
`wall_live_source_unavailable` (409). Error bodies and close reasons expose fixed
codes; clients do not display arbitrary server messages or credentials.

## NLWM binary envelope

All integer fields are big-endian. Header size is exactly 80 bytes. Payload length
must match exactly; all timestamps fit JavaScript's positive safe-integer range
(source capture/encode time may be zero). Epoch and frame ID are nonzero u64 values.

| Offset | Bytes | Field |
| --- | --- | --- |
| 0 | 8 | `4e 4c 57 4d 01 01 00 50` (NLWM, v1, keyframe, header length) |
| 8 / 16 | 8 each | Source epoch / frame ID |
| 24 / 32 | 8 each | Source capture / encode milliseconds (source clock) |
| 40 / 44 | 4 each | Width / height |
| 48 / 52 | 4 each | Source dropped frames / payload length |
| 56 / 57 | 1 each | Color 1 (sRGB); codec 1 (BGRA) or 2 (PNG) |
| 58 | 6 | Reserved, all zero |
| 64 / 72 | 8 each | Common daemon receive / per-viewer encode-complete send time |
| 80 | Variable | Pixel payload |

The receive time comes from the original shared `StoredLiveFrame`, so it is
identical for raw and PNG viewers of that source frame. Send time must not precede
receive time; it excludes subsequent socket transmission. Source timestamps must
not be subtracted from daemon timestamps to claim latency across different clocks.

Raw dimensions must exactly match `width * height * 4`. PNG is RGB8, non-interlaced,
and permits only IHDR, IDAT and IEND chunks. Native and TypeScript readers validate
the actual IHDR dimensions and bounded chunk lengths before browser decoding;
decoded bitmap dimensions are checked again. Payload CRC/deflate decoding remains
the image decoder's responsibility. Unknown codecs, reserved bits, bad lengths,
stale identities and regressing common receive time fail closed.

[wall-media.v1.json](fixtures/wall-media.v1.json) contains raw/PNG golden packets.
Hook retains a byte-identical fixture for its independent Rust and TypeScript
readers. The added conformance tests have not yet been executed.

## Ownership, recovery and diagnostics

A normal Hook capture Unit can publish without an unrelated Surface. Source create,
source observation and source control revoke may omit both Surface binding fields
only for the exact authenticated paired source device, including loopback. Fields
may be omitted or null together; partial bindings fail. Surface-bound sources and
viewer acquire/release/input still require their existing attachment authorization.
Session-store ownership validation also applies to standalone sources.

The Hook runtime owns publication, coalesces in-flight requests per capture, and
stops a late returned relay after runtime disposal. Closing a Unit parameter panel
does not stop publication. Recovery reauthorizes the same Loom origin and paired
device, checks discovery, joins old workers, revokes authority and rebases the
event cursor without replaying input. It reconnects a matching existing source or
recreates a missing source with the original capture and public Live ID. Closed,
foreign or ended captures report `source_recovery_unavailable`; they are not
silently replaced. Concurrent user stop prevents publishing a replacement owner.

Hook admits at most four unfinished media consumers, including obsolete layout
generations, with one open/read/decode operation per consumer. Native latest-frame
slots, completed-error slots, decoded queues and retries are all bounded. This
defines retained resources, not a measured process-memory ceiling or decode deadline.
OS DNS and browser decode retain their platform cancellation limitations.

`wall_live_stats` reports process-local validated NLWM frames/bytes, IPC reads and
native replaced frames. Output `data-media-stats` exposes at most four source
snapshots: selected epoch/frame ID, common receive time, estimated selection time,
clock uncertainty, pending/skipped/late/stale frames and repeated selections.
Selection counters include render-loop calls; they do not count physical refreshes.
Encoded bytes exclude WebSocket/TLS/IP overhead and do not measure NIC traffic.
