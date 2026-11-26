# Loom Runtime Security Boundaries

This document consolidates the daemon HTTP request-body admission rules, native image workflow resource budgets, and Linux managed-process containment guarantees. For plugin manifest permissions, signing, and trust policies, see [plugin security](plugin-security.md), [plugin permissions](plugin-permissions.md), and [plugin signing and trust](plugin-signing-and-trust.md).

## 1. HTTP request-body admission

For routes permitting bodies above the ordinary small-JSON request limit, `loom-daemon` (`apps/daemon/src/http_body_admission.rs` and `apps/daemon/src/runtime/connection_read_admission.rs`) validates complete HTTP headers before accepting additional upload bytes:

- `Host`, `Origin`, browser-context headers, and `Content-Type` checks run as soon as the header terminator arrives (at most one 64 KiB read chunk may already contain initial body bytes).
- Large-body routes require header-only authentication preflight before reading the upload stream:
  - Administrator bearer tokens or `HttpOnly; SameSite=Strict` management cookies admit routes that accept administrator authority. Projection routes requiring a paired device actor must still pass device preflight even when an administrator header is present.
  - Device credentials (`Authorization: Device <token>`) must reference an enabled, approved, unexpired session, carry a fresh nonce, and match the route's scope. Device sessions never gain package-installation privileges, and sessions with exhausted nonce budgets are rejected during read-only preflight so they cannot park read workers.
  - Inbound peer routes (`/v1/projection-peer/raster-check` and `/v1/projection-peer/transfer`) use body-carried pinned-peer signatures under a 6 MiB body cap and grow buffers strictly with received bytes.
- Device preflight is read-only and non-blocking: it neither consumes a nonce nor caches authority. Contended or poisoned device-registry locks return `503 / device_registry_unavailable` immediately without blocking socket-read workers. Full dispatch rechecks session validity, revocation, route scope, and atomically consumes the nonce once the body is read.
- Request readers never preallocate a declared `Content-Length`. Buffers grow incrementally with fallible allocation up to the route's maximum size and declared length.
- Early admission rejections write the HTTP error response and close the write half immediately, then drain at most the remaining declared bytes into a fixed 64 KiB scratch buffer within a 500 ms total budget (200 ms write timeout) so eager clients receive structured `401`/`403` responses instead of TCP resets.

Regression coverage lives in `apps/daemon/src/tests/http_body_admission.rs`.

## 2. Native image workflow resource limits

`crates/loom_native_image` implements the six built-in `core.image.*` workflow operators invoked by `loom_workflow_runtime`. Inputs are PNG bytes/data URLs and outputs are PNG data URLs; caller parameters never override host resource bounds.

Each image operation enforces these independent per-job limits:

| Resource | Limit |
| --- | ---: |
| Compressed PNG input | 16 MiB |
| Base64 input (including optional data URL prefix) | `ceil(16 MiB / 3) * 4 + 128` bytes |
| Width or height | 8,192 pixels |
| Total decoded pixels | 8,388,608 pixels |
| Decoder allocation budget and decoded pixel bytes | 64 MiB |
| Encoded PNG output before Base64 | 64 MiB |
| Blur radius | Integer `0..=128` (`0` preserves radius-1 behavior) |
| Pixelation block size | Integer `0..=8192` (`0` preserves block-1 behavior) |
| Cooperative processing deadline | Earlier of caller deadline and 15 seconds |

- Dimension, pixel-product, and `image::Limits::max_alloc` bounds are installed and checked before pixel allocation or decoding. `max_alloc` bounds decoder buffers per job rather than total process RSS.
- Box blur uses sliding column sums (`O(pixels)` time independent of radius, `O(width)` scratch memory). Blur, pixelation, codec reads/writes, and stage transitions check cooperative cancellation and deadlines so timed-out jobs leave no detached worker threads.
- Out-of-range dimensions or parameters fail closed with structured `ProcessResult` errors rather than silently clamping or truncating values.

Regression coverage lives in `crates/loom_native_image/tests/resource_limits.rs`.

## 3. Linux managed-process containment

On Linux, `crates/loom_process` enforces strict descendant-process lifetime containment for managed plugin, framework, capability, and MCP child processes:

- **Required host primitives**: distribution Bubblewrap (`/usr/bin/bwrap` >= 0.8), `perl-base` (`/usr/bin/perl` for an `exec`-only acknowledgement shim), enabled unprivileged user and PID namespaces, and Linux `pidfd` support (`pidfd_open` and `pidfd_send_signal`, Linux >= 5.3).
- **Pinned supervisor descriptors**: the outer launcher validates that `/usr/bin/bwrap` and `/usr/bin/perl` (and their ancestor directories) are host-root-owned and not group/world-writable in the initial user namespace, pins kernel file descriptors to both binaries, and passes one-use `SCM_RIGHTS` frames over an inherited Unix `socketpair` verified via cross-namespace `SO_PEERCRED`. Nested launchers execute through `/proc/self/fd`, preventing `PATH` overrides, plugin environment injection (`LOOM_PROCESS_SUPERVISOR_FD` is scrubbed/reserved), or bind-mount spoofing. Rootless/remapped callers without an inherited outer launch capability fail closed.
- **Namespace reaper and `pidfd` termination**: every managed invocation runs in its own user/PID namespace under Bubblewrap's dedicated PID 1 reaper (`--as-pid-1` is never given to untrusted commands). A bounded `--info-fd` / `--block-fd` startup handshake (4,096-byte cap, 5-second launch budget) pins the namespace init process via `pidfd` before any plugin code runs.
- **Complete tree cleanup**: on timeout, cancellation, drop, output-limit breach, or leader exit, Loom sends `SIGKILL` through the pinned `pidfd` before joining pipe workers. The kernel terminates all namespace descendants—including double-forked processes, `setsid`/`setpgid` leaders, cleared `PDEATHSIG` children, and nested namespaces—without PID-recycling races or `/proc` polling.
- **Exec shim**: a private `FD_CLOEXEC` pipe distinguishes target spawn failures (`ProcessError::Spawn`) from normal exit codes (such as exit `127`). All Linux capabilities are dropped, and plugin environment variables are applied only immediately prior to `exec` without shell interpolation.
- **Deployment and explicit limits**: Docker images include `bubblewrap` and `perl-base`, while host kernels/LSMs (such as Ubuntu 24.04 AppArmor `userns` policies) must permit unprivileged user namespaces for `/usr/bin/bwrap`; unsupported hosts fail managed launches closed without disabling LSMs or falling back to unsafe process groups. Windows uses suspended process creation assigned to a Job Object before resume. Both mechanisms guarantee process-tree lifetime containment (plus memory/process-count caps on Windows Job Objects; advisory on Linux), not a full hostile-code filesystem/network OS sandbox.
