# Loom Live Screenshot Protocol v1

Status: Phase 9 private-network readiness candidate. Public NAT relay remains unavailable. Runtime
availability is negotiated and must not be inferred from the presence of this contract.

## Ownership

`loom.live.v1` is the shared control and media contract for live screenshot
sessions. Loom owns session identity, device roles, controller authority,
recovery ordering, observation normalization, trigger audit, and transport
authorization. Hook owns source capture, viewer rendering, input execution, and
capability reporting. Art execution remains in Loom.

`loom.surface.v1` remains the declarative UI/state protocol. The
`/v1/surfaces/stream` long-poll endpoint must never carry video frame bytes or
per-frame Base64. A live session may reference a Surface attachment, but its
media uses a separate binary WebSocket or a negotiated same-machine transport.

## Control envelope

Every JSON control message contains:

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

The canonical JSON Schema is
`protocol/schemas/live-control.v1.schema.json`. Unknown top-level or payload
fields, unknown message types, invalid identifiers, and other protocol versions
are rejected. Identifiers contain 1-160 ASCII letters, digits, `_`, `-`, `.`,
`:`, or `/`.

Control messages are reliable and exactly ordered within an epoch:

| Message | Purpose |
| --- | --- |
| `session_start` | Full immutable source identity plus initial session snapshot |
| `session_ack` | Explicit accept/reject from a peer |
| `session_state` | Revisioned visibility, viewers, controller, and reason |
| `frame_notice` | Observable metadata; no encoded frame bytes |
| `input_event` | Ordered source input edge or coalescible pointer move |
| `observation` | Source, confidence, state, time, sequence, and optional value |
| `trigger_condition` | Revisioned deterministic condition definition |
| `trigger_event` | Authorized, idempotent trigger audit result |
| `control_transfer` | Single-controller authority revision and expiry |
| `resume_request` | Last accepted control, frame, and input positions |
| `session_end` | Terminal close, revoke, permission, timeout, or error event |

An epoch change is explicit. Within an epoch, control and input sequences cannot
skip or repeat. A resume snapshot moves all three sequence baselines into a
strictly newer epoch. Frame IDs may skip because video is replaceable; stale or
duplicate frame IDs are rejected and the gap is observable.

Device-token renewal does not reset an existing session's epoch or sequence
namespaces. The authenticated member `GET /v1/live/sessions/{sessionId}` adds
`requesterControl: { deviceId, epoch, controlSequence, inputSequence }`, read
under the same lock as the session and observation snapshot. Only the requesting
member's accepted positions are returned; discovery, admin snapshots and attach
responses omit this field. A missing current-epoch position is zero. This read
is not a reservation: concurrent commands can still make the next request fail
with 409, and clients must not guess positions from error text or reset identity.

A rejoining viewer sends `session_ack` at its accepted control position plus
one. Every accepted attachment, including an existing member, emits a new
`viewer_joined` state event and revision for bootstrap; membership remains
deduplicated and controller authority is not granted. Source recovery reads
the member snapshot after stopping its old publishers and continues each
observation ID at its accepted position plus one. Old stability intervals and
input edges must not be replayed. Clients requiring safe recovery fail closed
when a daemon does not provide the actor-scoped cursor.

## Session and authority

`LiveScreenshotSession` carries source device/Hook/window identity, the physical
source region and anchor, frame stream descriptor, input and observation
capabilities, trigger bindings, viewers, the optional controller, visibility
and capture strategies, revision, and timestamps.

The session advertises at most 32 viewers and one controller. Controller changes
carry an authority revision and expiry. Device authentication, attachment
binding, token expiry, nonce replay prevention, and revocation remain Loom
runtime responsibilities; a valid JSON envelope is not authorization.

For a live media connection admitted with a Device session, explicit session revocation
uses WebSocket Close code `1008` (Policy), reason exactly `live_media_device_revoked`.
The daemon retains an in-memory shared revocation marker for that admitted session, marks
it before a successful disable/delete removes active sessions, and never clears it when
the Device is re-enabled. The marker is reclaimed with its session/socket references;
there is no persistent token tombstone history. Failed management persistence rolls back
the Device without deleting or marking its sessions. Expiry and nonce-capacity eviction
invalidate media authorization but do not set this deliberate-revoke marker.

Clients may terminate that local relay and clear its old pixels on receiving this exact
signal. Other Close reasons or generic HTTP errors do not prove deliberate revocation.
An already-evicted session cannot acquire later revoke provenance, and network loss can
prevent delivery of a Close. Authorization checks still fail closed in those cases;
the signal is not a linearized zero-in-flight-frame guarantee or a declaration that the
whole LiveSession is closed. Administrator admission retains its separate behavior.

Capture queues retain exactly two or three newest video frames. Producers never
wait for a slow viewer: the oldest frame is replaced and the next frame notice
reports the drop count. Input button/key edges, permission changes, lifecycle,
and authority events use the reliable control queue and are never dropped with
video.

## Binary media frame

WebSocket media frames begin with a fixed 64-byte, network-byte-order header,
followed by 1 to 67,108,864 payload bytes:

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 4 | ASCII magic `NLLV` |
| 4 | 1 | Binary version `1` |
| 5 | 1 | Flags; bit 0 is keyframe |
| 6 | 2 | Header length `64` |
| 8 | 8 | Session epoch |
| 16 | 8 | Frame ID |
| 24 | 8 | Capture timestamp ms |
| 32 | 8 | Encode timestamp ms |
| 40 | 4 | Width |
| 44 | 4 | Height |
| 48 | 4 | Cumulative dropped-frame count |
| 52 | 4 | Payload length |
| 56 | 1 | Color: `1=srgb`, `2=hdr10` |
| 57 | 1 | Codec: `1=raw_bgra`, `2=h264`, `3=jpeg` (explicit media profile only) |
| 58 | 6 | Reserved, all zero |

The decoder rejects bad magic/version, non-zero reserved bytes, zero/oversized
payloads, length mismatches, invalid dimensions, stale epochs, and stale frame
IDs. H.264 is the LAN candidate; raw BGRA is a local/diagnostic candidate. HEVC
and AV1 are not v1 codecs.

The enum alone does not establish a negotiated or usable H.264 path. The current
Hook LiveRelay viewer supports sRGB/raw BGRA and the explicitly negotiated JPEG
profile below. H.264 still lacks a functioning end-to-end decoder path; its
enum is not enabled by diagnostics or by JPEG negotiation.

### JPEG 连接级媒体 profile

`loom.live.jpeg.v1` 是可选 WebSocket 子协议，不替换 `loom.live.v1` 控制协议。
只有请求明确提供它时，Loom 才返回它；旧客户端仍选 v1。新 Hook 提供
`loom.live.jpeg.v1,loom.live.v1`，旧 Loom 可选择第二项，因此无需额外探测请求。
鉴权、Host/Origin 检查、source/viewer 分权、连接上限和 epoch 门禁不变。

| 源/观看端 | 新 Loom 的处理 |
| --- | --- |
| 旧源 → 新或旧观看端 | 原 raw BGRA，不强行重新编码 |
| 新 JPEG 源 → 新 JPEG 观看端 | 原始 NLLV/JPEG 字节直通 |
| 新 JPEG 源 → 旧观看端 | 有界懒解码，生成同 epoch/frame ID 的 raw NLLV |
| 新 JPEG 源 → 屏幕墙 | 共用 raw adapter，再走既有 NLWM raw/PNG 路径 |

JPEG codec `3` 的约束：独立关键帧、sRGB、压缩载荷不超过 16 MiB、单边不超过
16384、BGRA 像素总量不超过 64 MiB。协议头在复制载荷前校验；daemon 复核
JPEG 头中的真实尺寸与声明一致，完整解码由实际消费者完成。JPEG 解码器设有
`image::Limits.max_alloc=128 MiB` 的尽力分配限制，并以显式像素/输入上限及
两路 admission 约束工作量；该配置不代表进程总内存硬上限。头校验不是呈现成功证据。
不允许 JPEG 成为旧控制/发现对象的 `frameStream.codec`；它只描述本次媒体
连接的表示。所有新观看端也必须接受 raw，以保留源端超大压缩帧的兼容回退。

原始 wire frame 继续保留在 2–3 帧 ring 中，不建立第二条媒体队列。每个不可变
帧最多缓存一个 raw 结果（成功或失败），共享给旧观看端和屏幕墙；缓存随帧及
其在途引用释放，不保留历史。仅旧消费者请求转换时解码，全局最多两个转换，
`try_lock`/非等待 admission 不阻塞 JPEG 直通。忙时旧消费者不推进已发送游标，
让出后重新取最新帧，同时继续处理控制消息。缓存锁、会话锁不跨 socket 写入。
转换后发送前再次检查观看权限、当前 epoch 和停止状态，不能用慢转换延续旧权限。
每次媒体循环亦重查成员资格；空闲连接通过既有 250ms 等待边界观察撤销。

该候选只启用 daemon 已有 `image` 依赖的 JPEG feature，不引入新服务、视频栈或
传输路由。Hook 与 Loom 可独立回退到旧版本并自动恢复 raw，无持久格式迁移。
`protocol/fixtures/live-jpeg-v1.nllv` 与 Hook 同路径字节一致；Rust 混合版本
loopback WebSocket、旧端像素和墙 adapter 测试不能冒充跨设备性能验收。

### Bounded LiveRelay stage diagnostics

The existing authorized session snapshot adds `mediaDiagnostics`; the NLLV wire
header layout, permissions and screen-wall protocol remain unchanged. These in-memory
counters are cumulative for the session, not persisted telemetry:

Discovery omits `mediaDiagnostics`; only existing member/administrator session
access returns the sample. It does not grant observation, controller or edit rights.

- `receivedBinaryBytes`: accepted NLLV bytes including its 64-byte header;
- `sourceSequenceGaps`: source frame-ID gaps; `bufferEvictions`: ring removals;
- `forwardedFrames` and `forwardedBinaryBytes`: successful viewer socket writes,
  summed across viewers (not unique source frames or receiver acknowledgements);
- `viewerSkippedFrames`: frame-ID positions skipped after an established viewer
  cursor in the same epoch. Initial/late joins and epoch changes are not counted;
- `failedWrites`: write failures; partial failed writes are not counted as successful bytes.

The legacy `relayDroppedFrames` combines source sequence gaps and ring eviction
counts. Neither ring eviction nor a cursor gap establishes network packet loss;
a fully consumed retained frame can be evicted normally.

Only one `lastForward` sample is retained across viewers, with the viewer device
ID, epoch, frame ID, actual output `wireCodec`, binary size, skipped count,
`adaptationMs`, `queueAgeMs`, `socketWriteMs` and
`writeSucceeded`. Queue age starts at Loom's accepted-frame retention timestamp;
socket write time includes the connection's payload copy and synchronous write.
`adaptationMs` measures selection of the connection representation, including a
legacy cache miss's decode; output bytes count raw fallback bytes rather than
the smaller source JPEG. All durations use Loom's monotonic clock. Failed samples replace the previous sample but
do not advance success counters; late results from a closed/old epoch are ignored.

These fields neither prove receiver decoding/presentation nor compute
cross-device latency from uncalibrated clocks. There is no per-frame history,
pixel dump, token, private key or authorization URL. The latest sample is not
a p50/p95 distribution. An external bounded sampler and actual receiver evidence
are required for the [#67 A baseline](https://github.com/aiaimimi0920/Loom/issues/67).
The focused `live_media_diagnostics_tests` include real loopback WebSocket writes;
this is not a physical two-device, WAN or multi-screen acceptance result.

The read-only [bounded sampler](../docs/LIVE_RELAY_MEASUREMENT.md) collects these
existing snapshots without changing the daemon protocol. Its observed-sample
percentiles are not all-frame distributions. Current `queueAgeMs` is sampled
after representation adaptation and already includes that adaptation duration.

## Observation and trigger trust

Observations always carry `source`, `confidence`, `observedAtMs`, and `sequence`.
`unknown`, `stale`, and `error` observations cannot carry an authoritative
value. An `unknown` source can only have `low` confidence, a `vision` source
cannot claim `exact`, and a UI Automation source must carry a validated element
locator. Trigger conditions
record stability time, rising-edge, re-arm, minimum confidence, and a revision.
Trigger audit records the session-bound observation ID, observation sequence,
source device, observation source, authorizer, action request ID, and a
deterministic idempotency key.

The server accepts at most 256 current observations per live session and at most
64 KiB of serialized value data per observation. These limits are enforced
before state storage and reliable event publication. Observation timestamps and
sequences must be positive. Rejected limits do not consume an observation or
control sequence.

Paired-device discovery returns value-redacted session summaries. A specific
session's full observation snapshot requires session membership; the
administrator credential retains its explicit operational read path. Reliable
events also require a paired session member. Media frames never carry
observation values.

Hook reports observations and executes transparent input, but it never executes
Art locally. A paired viewer registers a trigger with
`POST /v1/live/sessions/{sessionId}/triggers`. The viewer may target only its
own mounted Surface attachment. Loom validates the target and condition before
accepting the viewer's next ordered control sequence.

Conditions use one of `equals`, `not_equals`, `greater_than`,
`greater_or_equal`, `less_than`, `less_or_equal`, or `contains`. An operand may
be a literal, or the exact selector object `{ "path": "/json/pointer",
"value": expected }`. Operands are limited to 4 KiB, 16 levels, and 512 JSON
nodes. Numeric operators require a JSON number.

Only fresh `stable` or `triggered` observations from a known source can match.
The observation must meet the condition's minimum confidence and stability
duration, and `stableSinceMs` cannot exceed `observedAtMs`. `unknown`, `stale`,
`error`, detected-only, observing-only, old, future, regressed, and
low-confidence observations fail closed. Visual estimates therefore cannot
silently acquire UIA's `exact` trust.

Loom owns rising-edge state, re-arm, the authorizing viewer's online state, and
idempotency. A matching observation is first reserved and written to the
reliable trigger audit stream. The same deterministic key becomes the Surface
event ID. The existing `SurfaceActionExecutor` then performs authorization,
generation checks, confirmation for declared high-risk actions, cancellation,
timeout, retry/concurrency policy, and Loom-owned Art execution. Its final
accepted, skipped, or failed result replaces the reservation under the same
idempotency key. A session cannot close while a reserved action is awaiting
finalization. Failed dispatch releases the key and re-arms a re-armable
condition for a later, newly sequenced observation.

If the authorizing viewer is offline, media and observation publication
continue, but the remote trigger is skipped with `authorizer_offline`. A fresh
observation after reconnect may fire; the offline observation itself is never
replayed. Discovery redacts trigger definitions and audits, while attached
members can receive the bounded current snapshot and reliable audit updates.

## Phase 8 extension capability boundary

Hook exposes the local `hook.live.extensions.v1` discovery report before an
optional provider is allowed to affect a live session. Each entry carries a
bounded capability ID, kind, `available` or `unavailable`, execution owner,
trust boundary, optional observation source and maximum confidence, and an
unavailable reason. An unavailable entry is a denial, not a planned or degraded
success state.

Application adapters and visual providers have `executionOwner =
loom_capability_plugin` and `trustBoundary =
loom_verified_capability_package`. An adapter cannot become live merely because
Hook recognizes an application name. Its code and configuration must be an
installed, enabled, non-revoked Loom Capability Plugin with approved permissions
and a content-bound package digest. The generic Capability Plugin runtime owns
process isolation, limits, lifecycle, permission grants, signature status, and
fault recovery; Hook does not add browser-, Electron-, or renderer-specific
branches to capture or UIA owners.

The initial report contains three adapter slots (`browser_accessibility`,
`electron_accessibility`, `special_rendering`), one visual slot
(`vision_observation`), and five extended-input slots (`touch_input`,
`pen_input`, `ime_input`, `clipboard_input`, `file_drop_input`). This candidate
reports every slot as unavailable because no corresponding provider/backend is
shipped in the core pair. OCR remains an independently installed Loom Capability
Plugin and is not copied into Hook core.

Visual observations have maximum confidence `high`. Loom also resolves the
target action from the locked Surface package immediately before dispatch and
requires `exact` confidence for a declared high-risk Surface action. Stability,
authorization, confirmation, cancellation, idempotency, and audit checks remain
in Loom. Human takeover continues through controller transfer or the source
device's immediate reclaim path. Optional semantic active operations remain
ordinary permission-checked Loom Surface/Art actions; an adapter never executes
Art inside Hook.

The current window-message input backend continues to advertise only the input
kinds it actually implements. Touch, pen, IME composition, live clipboard, and
file drop are not added to `LiveInputKind`; callers see explicit unavailable
capabilities rather than a protocol value that the source cannot execute.

## Phase 9 private-network and latency boundary

The binary WebSocket media path remains independent of any cloud account. Loom
accepts non-loopback HTTP routes only when an authenticated TLS terminator is
declared, and media/control routes still require an administrator credential or
an approved, unexpired device session. Device disable/revocation, session epoch,
one-time challenge, nonce replay prevention, attachment membership, and the
single-controller lease remain authoritative server checks.

Viewer media sockets now read bounded WebSocket control frames as well as
sending media. They answer Ping with the exact Pong payload, consume Pong, and
reject viewer binary/text data. Hook uses this to measure viewer round-trip
latency and choose its replaceable pointer-move coalescing interval. Reliable
input edges and control events remain on the authenticated ordered HTTP control
plane and are not downgraded by latency policy.

Hook exposes `hook.live.network.v1` as a local, non-secret capability report.
For this candidate, `websocket_binary` may be available for loopback/private
HTTPS Loom, while `cloud_relay` and `webrtc_turn` are unavailable because no
relay provider or NAT traversal service is configured. Loom continues to reject
non-`websocket_binary` session creation. A future provider must add independent
runtime, storage-retention, redaction, deletion, congestion, recovery, and
red-team evidence before either transport may be advertised as available.

No live media frame or observation value is written to the Surface stream.
Current local runtime buffers are bounded and transient. The declared no-cloud
frame/OCR-retention policy is not an external cloud audit: there is no provider
to audit in this candidate, and the G9 evidence must remain partial until that
infrastructure and both clean-source formal releases exist.

## Phase 1 verification

The `loom_protocol` tests cover JSON schema acceptance/rejection, unknown fields,
version rejection, bounds, exact control/input ordering, frame gaps, epoch
resume, binary round-trip, truncated/oversized/mismatched frames, and untrusted
observation values. Hook maintains an independent TypeScript parser and binary
codec with matching negative tests.
