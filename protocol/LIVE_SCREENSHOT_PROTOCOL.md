# Loom Live Screenshot Protocol v1

`loom.live.v1` is the shared control and binary media contract for live window/region sessions across Loom and Otter. Loom owns session identity, device roles, single-controller authority, recovery ordering, observation normalization, trigger auditing, and transport authorization; Otter owns source capture, viewer rendering, transparent input injection, and local capability reporting.

`loom.surface.v1` remains the declarative UI/state protocol: `/v1/surfaces/stream` never carries video frame bytes or per-frame Base64. Live media flows over a dedicated binary WebSocket (`/v1/live/media` or `/v1/walls/live/media`) or negotiated same-host transport.

## Shared-Loom designated viewer

An authenticated standalone source may include `directViewerDeviceId` in `POST /v1/live/sessions` outside the control envelope as an immutable grant to one paired recipient device (`directView: true` in snapshots; input and window observations disabled). `POST /v1/live/sessions/{sessionId}/direct-viewer` accepts a `session_ack` envelope from that exact paired `Device` + nonce actor before appending `viewer_joined`.

## Control envelope and recovery ordering

Every JSON control message conforms to `protocol/schemas/live-control.v1.schema.json` (identifiers use `1..=160` ASCII `[A-Za-z0-9_.:/-]`; unknown fields or versions fail closed):

```json
{
  "protocolVersion": "loom.live.v1",
  "sessionId": "live-session-1",
  "epoch": 1,
  "sequence": 1,
  "messageType": "session_ack",
  "payload": {
    "accepted": true,
    "responderDeviceId": "device-b"
  }
}
```

Ordered control message types within an `epoch`:

| Message | Purpose |
| --- | --- |
| `session_start` | Immutable source identity plus initial session snapshot |
| `session_ack` | Explicit peer accept/reject |
| `session_state` | Revisioned visibility, viewers, controller, and reason |
| `frame_notice` | Observable frame metadata (no encoded pixel bytes) |
| `input_event` | Ordered source input edge or coalescible pointer move |
| `observation` | Source, confidence, state, timestamp, sequence, and optional value |
| `trigger_condition` | Revisioned deterministic trigger definition |
| `trigger_event` | Authorized, idempotent trigger audit record |
| `control_transfer` | Single-controller authority revision and lease expiry |
| `resume_request` | Last accepted control, frame, and input sequence positions |
| `session_end` | Terminal close, revoke, permission, timeout, or error event |

- Control and input sequences never skip or repeat within an epoch; `resume_request` advances all three sequence baselines into a strictly newer epoch. Frame IDs may skip when replaceable video frames drop.
- Authenticated member `GET /v1/live/sessions/{sessionId}` includes `requesterControl: { deviceId, epoch, controlSequence, inputSequence }` so reconnecting viewers and sources resume cleanly without resetting session epochs on token renewal.
- Viewer token renewal sets `requireExistingMembership: true` on the HTTP attach request (`409 / live_viewer_renewal_unavailable` if no longer a member or holding an active controller lease; clients never silently downgrade a rejected renewal to a fresh join).
- Each session supports up to **32** viewers and **1** active controller. Explicit device disablement/deletion closes admitted Device media WebSockets with code `1008` and exact reason `live_media_device_revoked`. Capture rings retain only the **2–3** newest frames so slow viewers never block capture.

## Binary media frame (`NLLV` v1)

WebSocket media frames start with a 64-byte big-endian header followed by `1..=67,108,864` payload bytes:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | ASCII magic `NLLV` |
| 4 | 1 | Binary version `1` |
| 5 | 1 | Flags (`bit 0` = keyframe) |
| 6 | 2 | Header length `64` |
| 8 | 8 | Session epoch |
| 16 | 8 | Frame ID |
| 24 | 8 | Capture timestamp (ms) |
| 32 | 8 | Encode timestamp (ms) |
| 40 | 4 | Width |
| 44 | 4 | Height |
| 48 | 4 | Cumulative dropped-frame count |
| 52 | 4 | Payload length |
| 56 | 1 | Color (`1 = srgb`, `2 = hdr10`) |
| 57 | 1 | Codec (`1 = raw_bgra`, `2 = h264`, `3 = jpeg`) |
| 58 | 6 | Reserved (all zero) |

### 1. Connection-level JPEG media profile (`loom.live.jpeg.v1`)

`loom.live.jpeg.v1` is an optional WebSocket subprotocol offered alongside `loom.live.v1` (`Sec-WebSocket-Protocol: loom.live.jpeg.v1, loom.live.v1`):

- **JPEG -> JPEG viewers**: zero-reencode `NLLV`/JPEG passthrough (`Arc<Vec<u8>>`).
- **JPEG -> legacy `loom.live.v1` viewers or Tile Walls**: bounded lazy adaptation (`apps/daemon/src/runtime/live_media_representation.rs`), caching at most one decoded raw result per retained frame shared across legacy viewers and wall encoders, capped at **2** concurrent non-blocking conversions (`try_lock`).
- **In-place final-buffer decoding**: legacy JPEG-to-BGRA adaptation preallocates the exact `64 + width * height * 4` output frame, decodes `L8`/`La8`/`Rgb8`/`Rgba8` directly into the payload slice via `ImageDecoder::read_image`, and expands pixels backward in place to BGRA without intermediate `DynamicImage` buffers or `copy_within` header shifts.
- **Bounds**: independent sRGB keyframes, compressed payload <= 16 MiB, dimensions <= 16,384 per side, decoded BGRA <= 64 MiB (`image::Limits::max_alloc = 128 MiB`), with header dimensions cross-checked against the JPEG SOF header before consumer fanout. All JPEG viewers also accept `raw_bgra` fallback frames.

### 2. Connection-level H.264 continuity profile (`loom.live.h264.v1`)

`loom.live.h264.v1` governs codec `2` Annex-B access units (`apps/daemon/src/runtime/live_media_continuity.rs` and `live_video_negotiation.rs`); `loom.live.v1` and `loom.live.jpeg.v1` sockets reject codec `2` fail-closed:

- **Wire bounds**: one Annex-B access unit per `NLLV` frame (3- or 4-byte start codes; AVCC rejected), payload <= 1 MiB, <= 256 NAL units, SDR (`srgb` header / BT.709 limited-range YUV), even dimensions `2..=4096`, total pixels <= 8,294,400. Keyframe flag is set iff the AU contains an IDR preceded by SPS/PPS; delta AUs never carry parameter sets.
- **Cursor continuity**: encoded H.264 frame IDs must be contiguous; initial stream entry, codec/dimension transitions, or source gaps require a fresh SPS/PPS/IDR. Unlike replaceable JPEG/raw frames, H.264 viewers consume the next contiguous AU from the 2–3 frame ring and recover from lag/join/epoch changes only on a retained IDR.
- **Dynamic policy & fallback control**: each viewer and wall socket holds an RAII media capability lease. Only when >= 1 viewer is attached and all attached consumers support H.264 does Loom send `{"type":"video_policy","epoch":1,"h264_allowed":true,"keyframe_sequence":1}` to an H.264-capable source. Viewers on `loom.live.h264.v1` may send bounded (<= 256-byte) `{"type":"keyframe_request","epoch":1}` (coalesced at 250 ms intervals) or sticky `{"type":"video_fallback","epoch":1}` control frames.

## Stage diagnostics and bounded baseline sampler

Authorized member/admin `GET /v1/live/sessions/{sessionId}` snapshots include transient in-memory `mediaDiagnostics` (`receivedBinaryBytes`, `sourceSequenceGaps`, `bufferEvictions`, `forwardedFrames`, `forwardedBinaryBytes`, `viewerSkippedFrames`, `failedWrites`, and latest `lastForward` with `wireCodec`, `adaptationMs`, `queueAgeMs`, `socketWriteMs`, `writeSucceeded`). Note that `queueAgeMs` is sampled after representation adaptation and already includes `adaptationMs`.

Operators can sample live session counters read-only without mutating session state using `scripts/measure-live-relay.mjs` (Node.js 22+, credentials passed via `LOOM_MEASURE_AUTHORIZATION` environment variable only):

```powershell
node .\scripts\measure-live-relay.mjs `
  --base-url https://loom.example.test `
  --session-id live-session-example `
  --duration-seconds 60 `
  --interval-ms 500 `
  --timeout-ms 3000 `
  --output .\live-relay-sample.json
```

Validate the sampler contract with:

```powershell
node --test scripts/tests/live-relay-measurement.test.mjs scripts/tests/measure-live-relay.test.mjs
```

## Observation and trigger trust

- Sources publish up to **256** active observations per session (64 KiB max value JSON each) carrying `source`, `confidence`, `observedAtMs`, and `sequence`. `unknown` sources are capped at `low` confidence, `vision` observations are capped at `high` (`exact` is reserved for validated UI Automation locators, required for high-risk Surface actions), and `unknown`/`stale`/`error` states carry no authoritative value.
- Paired viewers register deterministic conditions (`equals`, `not_equals`, `greater_than`, `greater_or_equal`, `less_than`, `less_or_equal`, `contains`; 4 KiB / 16-depth / 512-node operand limit) via `POST /v1/live/sessions/{sessionId}/triggers` targeting their own mounted Surface attachment. Loom enforces stability windows, rising-edge/re-arm state, online authorizer presence (`authorizer_offline` skip without replay), deterministic idempotency keys, and `SurfaceActionExecutor` confirmation/timeout gates.
- Optional capability-backed live adapters (`hook.live.extensions.v1`) and network transport reports (`hook.live.network.v1`) fail closed (`unavailable`) unless backed by an installed, verified, permission-granted Loom Capability Plugin or configured relay transport. Viewer media sockets answer WebSocket `Ping` frames with exact `Pong` payloads for RTT-based pointer-move coalescing.
