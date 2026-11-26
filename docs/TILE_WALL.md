# Tile Wall Guide and Acceptance Checklist

A Tile is a network-authenticated display terminal with physical output geometry and declared input capabilities. Otter capture Units, Live Screenshot sessions, and Art Surfaces are content sources independent of any individual terminal.

## Ownership and runtime behavior

- **Daemon and management UI**: `loom-daemon` owns terminal registration, wall layouts, source placements, presentation leases, scene activation clocks, and authorization recovery. Closing the Loom Desktop or Web management console never stops active wall presentation or interaction on connected terminals.
- **Source independence**: publishing an Otter capture Unit to Loom, moving or resizing its placement across tiles, closing its parameter inspector, or removing one placement never restarts the underlying capture or Art instance while other placements still reference it. Recovering a Live source preserves its origin, paired device, capture handle, and public Live session ID without replaying stale input edges.
- **Explicit capability negotiation**: terminals advertise only the codecs, surface protocols, presentation scheduling, and input modes they actually implement (for example `image`, `raw_bgra`, `surface_v1`, `scheduledPresentation`, and `pointer` / `wheel` / `keyboard`). Capability declarations never grant authorization on their own, and unsupported multi-touch or pen inputs are reported as unavailable rather than emulated.
- **LAN exposure**: on the optional LAN HTTPS listener (`apps/daemon/src/lan_http_wall.rs`), Loom allowlists only the exact Device + nonce wall endpoints (`GET /v1/walls/state`, `POST /v1/walls/endpoints/{register,connect,heartbeat,disconnect,identify/report}`, `POST /v1/walls/images/read`, `GET /v1/walls/live/media` with `loom.wall.media.v1` / `NLWM` 80-byte headers, and exact Wall Surface `open`/`state`/`image`/`close`/`event`/`confirmation`/`cancel` routes). Layout administration and resource uploads remain admin-only.
- **Fail-safe input release**: focus loss, terminal disconnect, source closure, layout replacement, clock invalidation, freeze/blackout presentation modes, or device revocation immediately release held keys/buttons and revoke active input leases. Cross-tile pointer handoffs that cannot verify identity cancel safely.

## Normative protocol specifications

- [Wall geometry and ownership](../protocol/WALL_PROTOCOL.md): endpoints, walls, sources, placements, and half-open coordinate seams so boundary hits map to at most one tile.
- [Wall control API](../protocol/WALL_CONTROL_API.md): paired-device registration, CAS layout updates, persistence, presentation leases, and application receipts.
- [Wall Live input API](../protocol/WALL_INPUT_API.md): focus, ordered input sequences, controller leases, and source-input authorization without exposing admin tokens, host paths, or GPU handles.
- [Wall Art Surface API](../protocol/WALL_SURFACE_API.md): ephemeral terminal Surface views, scoped image leases, host-owned high-risk action confirmation, cancellation, and local draft/value boundaries without executing arbitrary Art JS on wall terminals.
- [Wall presentation API](../protocol/WALL_PRESENTATION_API.md) and [physical output identification](../protocol/WALL_IDENTIFICATION_API.md): freeze, blackout, resume, and bounded on-screen output identification banners.
- [Wall scene timing and media](../protocol/WALL_TIMING_MEDIA.md): prepared scene deadlines, boot-anchored monotonic server clocks, `raw_bgra` and shared-profile `png` media streaming (`NLWM`), and bounded frame queues without claiming hardware Genlock/Frame Lock.

## Open physical and multi-machine acceptance checklist

Focused protocol, store, LAN HTTPS/WSS, and isolated native offscreen integration tests pass in CI, but code and loopback tests do not substitute for physical multi-display and two-machine acceptance. The 19 open physical acceptance criteria below remain tracked explicitly:

### TW-04: Terminals and physical outputs (4 items)

- [ ] Enumerate, identify, and bind stable physical display outputs, supporting multiple output IDs per host device.
- [ ] Create borderless presentation windows on selected outputs and handle pixel dimensions, per-monitor DPI, rotation, hot-plug, and output removal cleanly.
- [ ] Render calibration patterns across multi-tile wall layouts with exact seam clipping and zero management-UI chrome bleed.
- [ ] Report accurate runtime capabilities and structured unavailability reasons across two independent terminals connected to one Loom hub, verifying clean per-terminal resource teardown on exit.

### TW-05: Presentation and source reuse (2 items)

- [ ] Deliver negotiated consumable media (`raw_bgra` or scaled `png` / declarative Surface snapshots) to constrained terminals without requiring local app or script execution.
- [ ] Bound decode dimensions, encoder concurrency, frame rings, and caches so a slow or stalled terminal never starves other viewers or grows memory unboundedly.

### TW-06: Interaction and controller authority (2 items)

- [ ] Verify deterministic cross-tile pointer handoff and capture ownership, cancelling unverified handoffs safely.
- [ ] Verify concurrent multi-user interaction across independent wall placements and visible single-controller arbitration when contending for the same live application source.

### TW-07: Restart recovery (1 item)

- [ ] Verify that restarting `loom-daemon` restores persisted wall layouts, re-establishes terminal leases and recoverable sources, and explicitly reports non-recoverable closed captures.

### TW-08: Scheduling, latency, and long-run stability (5 items)

- [ ] Verify prepared-scene staging and conservative activation deadlines with terminal `appliedRevision` receipts across real displays.
- [ ] Measure clock-offset estimation, 80 ms target buffering, late-frame dropping, and mixed-refresh-rate presentation on physical monitors.
- [ ] Measure cross-display visual skew on dynamic content using external optical/capture observation rather than inferring physical scanout from socket send counters.
- [ ] Measure end-to-end input-to-display latency alongside per-process CPU, private memory, OS handles, and NIC media bandwidth.
- [ ] Complete uninterrupted 600-second dynamic streaming + live input + disconnect/reconnect stress runs without handle or memory growth.

### TW-09: Release gates and joint delivery (5 items)

- [ ] Pass formatters, unit/contract suites, Rust/TypeScript builds, and strict effective-code-line gates across both repositories.
- [ ] Pass dependency security and clean-source release contracts with zero fabricated approvals or waivers.
- [ ] Complete two-machine physical LAN acceptance covering static images, Live sessions, interactive Art Surfaces, cross-screen input, disconnect recovery, and headless server operation.
- [ ] Re-verify the critical joint scenarios using the exact signed/packaged release binaries, recording executable paths, SHA-256 digests, and observed hardware bounds.
- [ ] Audit final `git status` and `git diff --check` across participating repositories with complete artifact provenance.
