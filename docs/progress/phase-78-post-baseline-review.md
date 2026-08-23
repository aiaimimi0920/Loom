# Phase 78 — Post-baseline cross-repo review (Hook v0.1.7 → HEAD, Loom a8e3df0 → HEAD)

## Purpose

Review every change made since the last big version, in both repos, and record the
findings here. Primary question: are the changes correct, reasonable, and free of newly
introduced problems? Secondary question: where can performance, coverage, and general
code quality be improved?

This document is the durable ledger for the review. Each slice is reviewed as its own
small task and its findings are appended below as soon as that slice is done, so the work
survives an interrupted session.

## Baselines

| Repo | Baseline | Head | Diff |
| --- | --- | --- | --- |
| Hook | `v0.1.7` (`85b785a` tag → `ffe21d5` commit, 2026-08-03) | `5c7d6da` (2026-08-20) | 225 files, +22163 / -5479, 24 commits |
| Loom | `a8e3df0` (tag `框架修改前的最后一个版本`, 2026-08-01) | `671ae48` (2026-08-20) | 424 files, +113147 / -33106 |

Loom has no `V0.1.7` tag; `a8e3df0` is the closest user-named version to Hook's
2026-08-03 baseline and its diff is a superset of the `7abfeed` diff, so nothing is
missed by choosing it.

## Exclusions (noise, not reviewed line-by-line)

- `Cargo.lock` (Hook `src-tauri`, plus Loom's 3).
- Hook vendored crates `src-tauri/crates/drag/**`, `src-tauri/crates/scap-direct3d/**`
  (diff is pure `rustfmt` reflow).
- Loom `mcp-server-packages/stock-api/runtime/vendor/**` (104 vendored files, ~7989
  lines). Only `UPSTREAM.json`, `PYSNOWBALL.json`, `node-runtime.json` are reviewed.
- Loom `gen/schemas/capabilities.json` (generated), ~140 binary icons,
  `design/icon-review-r1..r6/**`.
- CSS-only churn (`app.css`, `styles.css`, surface CSS) — skimmed, not line-reviewed.

## Already-closed findings — do not re-report

- `docs/progress/art-framework-refactor-audit-2026-08-12.md` (4 closed gaps + verification matrix).
- `docs/progress/art-framework-refactor-independent-review-handoff-2026-08-13.md`
  §4.1 known platform limits, §4.2 what still needs review, §4.3 deliberately not
  implemented and must not be added back, §6 questions the review must answer.
- `docs/progress/phase-69` … `phase-77` records.
- `docs/progress/MASTER.md` R-numbered ledger (R29 superseded by R30).

## Deliberately deferred — cross-device remote Surface

Recorded 2026-08-21, after S3-1 was reviewed. This is an owner decision, not a review
finding, and it settles the "two readings" question S3-1 raised.

**The intended feature.** A user adds devices to Loom, actively or passively. Loom then
pushes Hook's sticker blocks / Art nodes to those devices, and each device renders what it
receives. Loom is the central compute; the other devices are display endpoints. The
payload being pushed is rendered output (an Art node's result), not the computation — an
edge-compute-shaped split where the frames, not the work, cross the wire.

**Its status.** Future work. The device-management half is knowingly incomplete and that
is accepted. It is not a defect to report.

**What this reclassifies.** S3-1 stands as an accurate description of the code — the
remote / device-session half really is unreachable in the shipped binary — but it is no
longer a P1 defect. It is a documentation and feature-flag gap: the pairing, Ed25519
identity, device-session, and HTTPS-validation code is present in the tree and therefore
*looks* shipped, while `validate_loom_manifest` (`loom_connector.rs:184-189`) gates all of
it off. Mark it staged-for-later in the docs and put it behind a feature flag, so no
reader assumes those controls are protecting anything today.

**What this does not reclassify.**

- S1-1, S1-3, and S3-2 remain gate conditions. They are unreachable only because the
  remote path is unreachable; each becomes a live defect the day the flag is flipped, so
  they must be fixed *before* that switch, not after.
- S5b-1 is unrelated to remote and is unaffected. It is about the default loopback
  configuration shipping with no authentication at all, and it becomes more important, not
  less, once cross-device push is built on top.
- The snapshot / Art-node push pipeline itself is live code and stays fully in scope:
  surface snapshots, `surface_resources`, and the Hook-bridge canvas all run on the
  loopback path today. Only the "deliver to another device" half is gated.
- The Loom-side device surface is partially implemented and stays in scope:
  `/v1/devices/requests`, `/v1/device-sessions/challenges`, `/v1/device-sessions`, and the
  device registry exist (see S5b-1 and S5b-2). Hook's half is what the validator disables.

## Facts reviewers must not get wrong

- Hook's frontend is **Solid.js**, not React (`createSignal` / `createEffect` /
  `onMount` / `onCleanup`, `solid-js/store`, `Show` / `For`; props are getters and must
  never be destructured).
- Hook `src-tauri` is a single crate (`hook`, lib target `hook_lib`), not a workspace.
- Loom is a 25-member Cargo workspace plus two detached manifests:
  `apps/desktop/src-tauri` and `framework-packages/runtime-host` (declares its own empty
  `[workspace]`, so no workspace command reaches it).

## Slices

Reviewed in descending risk order. Status is updated in place.

| # | Slice | Scope | Status |
| --- | --- | --- | --- |
| S1 | Joint wire contract | Hook `loom_hook.rs` wire types + `services/{protocol,client,api}.ts` ↔ Loom `crates/loom_protocol/src/{surface,hook,device}.rs` + `protocol/schemas/*.v1.schema.json` | **done** — 4 findings (1×P1, 2×P2, 1×P3) |
| S2 | Hook sandbox trust boundary | `JavaScriptSurface.tsx`, `public/javascript-surface-bootstrap.js`, `public/javascript-surface-host.html`, `tauri.conf.json` CSP | **done** — 4 findings (2×P2, 2×P3) |
| S3 | Hook Rust runtime | `loom_hook.rs`, `device_session.rs`, `network_proxy.rs`, `lib.rs` integration points | **done** — 4 findings (1×P1, 2×P2, 1×P3); also corrects S1-1 reachability |
| S4a | Loom surface resources | `surface_resources.rs` (590 lines) | **done** — 4 findings (1×P1, 3×P2) |
| S4b | Loom surface store and actions | `surface_store.rs` (2063), `surface_actions.rs` (2370) | **done** — 7 findings (4×P2, 3×P3) |
| S5a | Loom daemon HTTP front door | `request_executor.rs` (480), `lib.rs` accept loop / read / parse / concurrency classes | **done** — 5 findings (2×P1, 1×P2, 2×P3) |
| S5b | Loom daemon persistence and platform | sensitive-file atomics, `repair_legacy_control_plane_permissions`, router auth checks | **done** — 6 findings (1×P1, 3×P2, 2×P3) |
| S6a | Loom art credentials and settings | `credentials.rs` (1010), `art_settings.rs` (813), digest/trust debt from S4b | **done** — 7 findings (3×P2, 4×P3) |
| S6b1 | Loom art extraction, outbound policy, runtime deps | `secure_zip.rs` (243), `network_policy.rs` (301), `dependency.rs` (230), extraction call sites | **done** — 10 findings (5×P2, 5×P3) |
| S6b2a | Loom art install lifecycle and crash recovery | `install.rs:322-950`, `2238-2295`, recovery call sites in `framework.rs:526-539` | **done** — 9 findings (4×P2, 5×P3) |
| S6b2b1 | Loom art integrity verify, activation, lockfile verify, MCP dependency locks | `install.rs:1119-2008`, `surface_actions.rs:255-343`, `loom_plugin_security` digest | **done** — 10 findings (1×P1, 3×P2, 6×P3) |
| S6b2b2 | Loom art uninstall, dependency lock sets, binaries, packaging and signing | `install.rs:2010-2236`, `2332-2771`, `verify_package_signature` cross-check | **done** — 11 findings (2×P2, 9×P3) |
| S6b2c1 | Loom framework readiness, resolution, registry state, trust store, recovery | framework.rs:121-955, plus status_of:1253-1345 and verify_framework_lockfile:1616-1712 | **done** — 8 findings (1×P1, 3×P2, 4×P3) |
| S6b2c2 | Loom framework package install, rollback, uninstall, retention, dependency registration | framework.rs:956-1252, 1349-1614 | **done** — 8 findings (1×P2, 7×P3) |
| S6b2c3 | Loom framework process execution | `framework_process.rs:1-1020` | **done** — 10 findings (2×P2, 8×P3) |
| S6b2d1 | Loom tool definitions, Surface manifest validation, registry persistence | `loom_tool_registry/src/lib.rs:32-830`, `art_settings.rs:133-227, 574-584` | **done** — 11 findings (1×P2, 10×P3) |
| S6b2d2 | Loom tool dispatch, cloud API execution, network policy | `loom_tool_registry/src/lib.rs:832-1400`, `network_policy.rs:67-232` | **done** — 9 findings (4×P2, 5×P3) |
| S6b2d3 | Loom MCP result normalization, image candidate collection, download fallback | `loom_tool_registry/src/lib.rs:1402-2321` | **done** — 11 findings (3×P2, 8×P3) |
| S7a | Loom MCP package install, trust, digest verification | `loom_mcp/src/package.rs` (532), `loom_tool_registry/src/secure_zip.rs` | **done** — 10 findings (3×P2, 7×P3); confirms S6b2b1-4 and S6b2b1-5 |
| S7b1 | Loom MCP config, Windows spawn-command resolution, registry URL, handshake | `loom_mcp/src/lib.rs:1-535` | **done** — 8 findings (1×P2, 7×P3) |
| S7b2 | Loom MCP stdio and streamable-HTTP clients, framing, bounded IO | `loom_mcp/src/lib.rs:536-1136` | **done** — 12 findings (1×P2, 11×P3); confirms S7b1-7, shares the crate-relocation fix with S7a-1 |
| S7c1 | Runtime-host MCP bridge: entry, config load, validation, env and header construction | `framework-packages/runtime-host/src/mcp.rs:1-550` | **done** — 9 findings (1×P2, 8×P3); confirms S7b1-7, compounds S7a-8 |
| S7c2 | Runtime-host MCP bridge: call/surface-action resolution, argument binding and schema normalization, execution, redaction | `framework-packages/runtime-host/src/mcp.rs:551-930` | **done** — 11 findings (1×P2, 10×P3); supplies the bounded-recursion fix shape for S6b2d3-2 |
| S8a1 | Image-search MCP server package runtime | `mcp-server-packages/image-search/runtime/image-search-mcp.ps1` (364) | **done** — 9 findings (1×P2, 8×P3); confirms S7b1-5, and the server's own URL-scheme check shows what S6b2d3-5 is missing |
| S8a2a | Stock-api MCP server package: constants, test fixture hook, parsers, aggregation | `mcp-server-packages/stock-api/runtime/stock-api-entry.js:1-500` | **done** — 10 findings (all P3); 3 go to the S9 performance/coverage queue |
| S8a2b | Stock-api MCP server package: fetch layer, host rotation, tool dispatch, JSON-RPC loop | `mcp-server-packages/stock-api/runtime/stock-api-entry.js:501-985` | **done** — 12 findings (1×P2, 11×P3); the P2 framing desync is reachable via S7c2-1 |
| S8b1 | Image-search Art runtime | `art-packages/samples/image-search/runtime/main.ps1` (358) | **done** — 11 findings (2×P2, 9×P3); S8b1-2 is the art-side twin of S6b2d3-2, S8b1-1 shares the outbound-policy fix with S7b2-1 |
| S8b2 | Shared PowerShell image helper (art-side halves of S6b2c3-1 and S6b2c3-2) | `art-packages/shared/image-runtime-common.ps1` (371) | **done** — 11 findings (2×P2, 9×P3); S8b2-1 adds a UNC/SMB path an HTTP-only outbound policy cannot see, S8b2-2 is the first finding that exceeds the 120 s framework timeout outright |
| S8c1 | Stock-monitor Art runtime: request handling, upstream fetch, parsing | `art-packages/samples/stock-monitor/runtime/main.ps1:1-500` | **done** — 11 findings (2×P2, 9×P3); S8c1-1 is a control-flow confused deputy (untrusted `surfaceAction` injection), S8c1-2/-3 are opposite-direction staleness bugs |
| S8c2 | Stock-monitor Art runtime: rendering, output assembly, entry point | `art-packages/samples/stock-monitor/runtime/main.ps1:501-1000` | **done** — 11 findings (2×P2, 9×P3); S8c2-1 serializes the same payload three times in one response, S8c2-2 is a validation failure path more permissive than its success path |
| S8d1 | Stock-monitor Surface: bootstrap, state ingest, helpers | `art-packages/samples/stock-monitor/surface/main.js:1-450` | **done** — 11 findings (2×P2, 9×P3); S8d1-1 sets a client timeout below the host's own budget, S8d1-2 re-derives the whole 2000-row series once per second |
| S8d2 | Stock-monitor Surface: rendering and chart drawing | `art-packages/samples/stock-monitor/surface/main.js:451-900` | **done** — 11 findings (3×P2, 8×P3); S8d2-1 latches the light tick channel off for good, S8d2-2 starves the K-line refresh whenever ticking, S8d2-3 rebuilds ~200 DOM nodes and ~16 MB of canvas per render |
| S8d3 | Stock-monitor Surface: event wiring, action dispatch, teardown | `art-packages/samples/stock-monitor/surface/main.js:901-1264` | **done** — 11 findings (2×P2, 9×P3); S8d3-1 puts unescaped provider text into `innerHTML`, S8d3-2 lets an interval click unlock an in-flight refresh |
| S9 | Performance and coverage gaps | Loom has no perf budgets at all; thin coverage in `loom_mcp/package.rs`, `surface_resources.rs`, `surface_actions.rs`; `runtime-host` never built by CI; Hook `lint` / `typecheck:test` / `test:surface-browser` in no workflow | **done** — 11 findings (5×P2, 6×P3); S9-10 names the one structural change that unblocks five earlier findings |

Out of scope for S8: everything under `mcp-server-packages/stock-api/runtime/vendor/` is
third-party code vendored verbatim (the `stock-api` dist bundles, `pysnowball`), reviewed only
for how the package invokes it, not line by line.

## Findings

Severity: **P0** ship-blocking, **P1** should fix before release, **P2** worth fixing,
**P3** note only.

(Appended per slice.)

### S1 — Joint wire contract

Checked first, and clean: the `loom.hook.v1` method/event name sets on the two sides
agree (`loom.hook.v0` and `loom.hook.v2` occur only inside tests, and
`negotiate_hook_protocol` correctly falls back to the one version it implements);
Hook's `loom.hook.subscribe` list covers exactly the four Hook events it handles in
`start_listener`, and the eleven `loom.surface.*` events are routed by prefix into
`emit_surface_push`, whose 11 arms match `SURFACE_EVENT_METHODS` one for one; the
`loom.hook.art.*` streaming events are handled in `forward_hook_art_execute` on the
per-request connection, so their absence from the subscribe list is correct, not a gap;
`loom_protocol::advertised_protocol_versions` is an inherent method on two different
structs, so the flat `pub use hook::*` glob re-export does not actually collide.

**S1-1 (P1, downgraded to P2 latent — see S3-1; gate condition for remote Surface, see "Deliberately deferred") — remote Surface poll can spin at 100% CPU and re-deliver the same snapshots forever.**

`Hook/src-tauri/src/loom_hook.rs:1314` loops `poll_remote_surface_once` with **no delay
on the success path** (`:1317-1332`), relying entirely on the daemon to long-poll. The
daemon does not always block:

- `Loom/apps/daemon/src/lib.rs:4207` seeds `messages` from
  `surface_snapshot_recovery_messages_for_device` whenever `after == 0`.
- With `messages` non-empty, `:4216` passes `Duration::ZERO` to `wait_after`, so
  `:2723-2735` returns immediately; with an empty broadcast history it returns
  `(after, false, [])`, leaving `cursor == after == 0`.
- `:4239` then breaks and `:4257` answers `"next": 0`.
- Hook stores that as its new cursor (`:1318`, `:1386-1389` defaults to the old cursor)
  and immediately re-polls.

Trigger: a non-loopback Loom base URL (so Hook takes the HTTP poll path rather than the
WebSocket one) plus at least one non-disposed attachment carrying a snapshot plus an
empty broadcast history — exactly the state after a daemon restart, because surface
instances are persisted on disk while `HookBridgeBroadcastHub` history is in memory only.
Effect: an unthrottled request loop, and every iteration re-reads the Loom manifest from
disk, re-runs `device_session::authorize_surface_request` (Ed25519), builds a brand new
`reqwest::Client`, and re-serializes and re-applies the full snapshot set on both sides.
The same spin is reachable a second way: if the `history` mutex is poisoned by a panic,
`wait_after:2720` returns instantly forever.

Fix direction: give the success path a floor delay when the response carried no new
cursor and no messages, and make the daemon advance/return a cursor that reflects the
recovery messages it just sent (or suppress the reseed when `cursor` did not move).

Reachability correction from S3: Hook cannot currently reach a non-loopback base URL at
all, because `loom_connector::validate_loom_manifest` rejects one (S3-1). The daemon-side
half of this bug is live for any other client of `/v1/surfaces/stream`, but the Hook-side
spin is latent — it becomes live the moment the manifest validator gains a remote mode,
which is exactly when nobody will be looking at this code. Fix it with S3-1, not after.

**S1-2 (P2) — the stream envelope is a fifth, undeclared protocol identifier and is never validated.**

Owner: **Lane B** (taken 2026-08-21, out of F8). Hook half only. The Loom half of the fix —
declaring the identifier in `crates/loom_protocol` and `protocol/schemas/*` — needs Lane A,
so Lane B will post the exact constant and shape it expects in
`docs/progress/phase-78-lane-sync.md` before changing anything in `Hook/`.
**Closed: Hook half by Lane B 2026-08-21, Loom half by Lane A 2026-08-22 in F14.**

`Loom/apps/daemon/src/lib.rs:4256` answers `"protocolVersion": "loom.surface-stream.v1"`.
That identifier exists nowhere else: not in `crates/loom_protocol`, not in
`protocol/schemas/*`, and Hook never reads the field
(`Hook/src-tauri/src/loom_hook.rs:1384-1395` takes only `next` and `messages`). A future
bump on either side is therefore undetectable at runtime. Fix direction: hoist it to a
`loom_protocol` constant next to `SURFACE_PROTOCOL_VERSION` and have Hook reject a
mismatch the way it already validates `loom.surface.v1` elsewhere.

**S1-3 (P2, gate condition for remote Surface — see "Deliberately deferred") — Hook drops the `reset` flag the daemon computes for it.**

`wait_after:2725` sets `reset` when the client's cursor has fallen behind the retained
history (`after + 1 < oldest`), and `:4243` then appends a fresh snapshot recovery set.
Hook never reads `reset` (`:1386-1395`), so it applies those snapshots as ordinary
messages on top of whatever local surface state it still holds instead of discarding the
stale state first. `surfaceStore.ts` monotonicity checks make this mostly benign today,
but the signal the daemon computes is being thrown away.

**S1-4 (P3) — long-poll timeout is silently clamped to a quarter of what Hook asks for.**

Hook requests `timeoutMs=20000` (`loom_hook.rs:1368`); the daemon does
`.unwrap_or(5_000).min(5_000)` (`lib.rs:4195-4198`). The request is not rejected, just
quietly reduced, so the real poll cadence is 4× Hook's intent. Harmless on its own,
but it multiplies the cost of S1-1 and should be either honoured or documented.


### S2 — Hook sandbox trust boundary

The isolation design itself holds up, and it is worth recording exactly why so that no
later change weakens it by accident. The surface iframe is
`sandbox="allow-scripts"` with `src` pointing at the static
`/javascript-surface-host.html` (`JavaScriptSurface.tsx:862-863`), so it gets a fresh
opaque origin per instance — no `allow-same-origin`, therefore no cross-surface and no
cross-window DOM access. Every plausible exfiltration channel is closed by the
intersection of the two policies: the host document's own meta CSP
(`javascript-surface-host.html:5`) sets `default-src 'none'`, `connect-src 'none'`,
`worker-src 'none'`, `form-action 'none'`, `frame-src 'none'`, `base-uri 'none'`, and
narrows `img-src` / `media-src` / `font-src` to `data:` and `blob:` only, so there is no
remote-pixel or beacon path; the missing `allow-popups` and `allow-top-navigation` plus
the app-level `frame-src 'self'` close the navigation path. Beyond CSP: `NeuroSurface` is
installed with `Object.defineProperty` + `Object.freeze` and the `MessagePort` stays in
closure scope, so surface code cannot reach the port directly; every inbound port message
must carry the matching per-instance token; `navigator.sendBeacon` is neutered;
`canNotifyHost` requires `event.isTrusted`; the entry module is imported from a blob URL
that is revoked in a `finally`; the heartbeat/CPU-budget watchdog can kill the frame;
initialization is correctly gated on `onLoad` (`:869`) rather than raced; and the document
URL carries no token or payload (`:504-507`).

**S2-1 (P2) — a single stray `message` event permanently bricks a surface, because the init listener is `once: true`.**

Owner: **Lane B** (taken 2026-08-21, out of F8). `Hook/` only. **Fixed 2026-08-21**, including
the defence-in-depth check below: the listener is now a named `onHostMessage` that removes
itself only after an init message is accepted, and it ignores anything whose
`event.source` is not `globalThis.parent`. Verified two ways — a source contract in
`Hook/__tests__/integration/ArtSurfaceInteractionContract.test.ts` (asserts the
registration carries no `once: true`, that the removal exists, and that both guards run
before it), and a real-Chromium regression scenario `stray-message` in
`Hook/scripts/run-javascript-surface-browser-smoke.mjs` that posts a junk message and a
port-less init before the real init and still requires a heartbeat. The scenario was
confirmed to fail (`{"kind":"timeout"}`) against the old `once: true` registration.

`Hook/public/javascript-surface-bootstrap.js:640-680` registered
`globalThis.addEventListener("message", handler, { once: true })` and the handler opens
with `if (port || event.data?.type !== "surface:init" || !event.ports?.[0]) return;`.
Those two guards are written as though a non-matching message could be ignored and the
real `surface:init` awaited afterwards, but `once` removes the listener when it is
*invoked*, not when it succeeds. Any `message` event that reaches the frame before the
host's init — today only same-frame `postMessage` or a future second sender, so this is
latent rather than live — consumes the listener, and the surface then never loads: no
entry import, no `ready`, and the host's watchdog eventually reports a heartbeat failure
that points nowhere near the cause. Fix direction: drop `once` and remove the listener
explicitly once init is accepted.

Related and deliberately kept separate because it is defence-in-depth, not a live bug:
the handler validates neither `event.origin` nor `event.source`. Not exploitable in the
current shape (opaque origin, no `allow-same-origin`, listener registered synchronously
at script-parse time before any surface module can run since `import()` happens at `:675`
inside the handler), but the check costs nothing and should exist.

**S2-2 (P2) — the `host-keydown` relay lets sandboxed content synthesize arbitrary keystrokes into the host window, unthrottled.**

Owner: **Lane B** (taken 2026-08-21, out of F8). `Hook/` only. **Fixed 2026-08-22.** The
relay in `JavaScriptSurface.tsx` now runs shape validation, then a key allowlist, then the
shared per-second event budget, and only then dispatches — so a non-relayable key costs
nothing and a relayable flood is throttled exactly like surface events. The new policy
module `Hook/src/services/surfaceHostKeydown.ts` owns all of it:
`isRelayableSurfaceHostKeydown` admits only plain `Escape`, exactly `Ctrl+E` (the reserved
edit-mode shortcut), and the bare `Control` / `Shift` / `Alt` presses that
`StickerAnnotationLayer`'s modifier tracking needs; everything else — `Delete`, `Tab`,
`Ctrl+S`, `Ctrl+Q`, `Enter`, `Alt+F4`, `Ctrl+Shift+E`, `Ctrl+Escape`, a bare `e` — is
dropped.

One deliberate deviation from the fix direction below: **`isTrusted` cannot be the trust
axis in Hook.** `src/app.tsx:1250` legitimately replays keydowns captured by the native
overlay keyboard hook (`overlay/global_shortcut`) as untrusted `KeyboardEvent`s, so
rejecting untrusted events wholesale would break real global shortcuts. Instead the relay
*tags* what it dispatches (`markSurfaceRelayedKeydown`, a non-writable non-enumerable
own property the sandbox cannot reach across the port), and host listeners call
`acceptsSurfaceRelayedKeydown(event)` — which ignores tagged events unless the listener
passes `{ surfaceRelayed: true }`. Same property, no collateral damage. All five `window`
keydown listeners were updated: `StickerAnnotationLayer` (opted in — modifier state would
go stale mid-drag, since a host drag started inside a surface keeps the pointer over the
iframe), `StickerContextMenuLayer` and `StickerTopStripPropertyBar` (opted in — dismissing
a menu or dropdown is non-destructive and matches user intent), `SurfaceConfirmationDialog`
and `AppSettingsDialog` (**not** opted in — a surface must not be able to answer its own
permission prompt even with the safe answer, nor close the user's settings).

Verified by `Hook/__tests__/unit/surfaceHostKeydown.test.ts` (allowlist accept/reject
matrix, tag detection and opt-in semantics including the untrusted-but-untagged case, and
tag unforgeability), a new relay-ordering source contract in
`Hook/__tests__/unit/JavaScriptSurface.test.ts`, and a runtime case in
`Hook/__tests__/unit/SurfaceConfirmationDialog.test.tsx` proving a relayed `Escape` leaves
the prompt undecided while the next real `Escape` still rejects.

`JavaScriptSurface.tsx:681-696` re-dispatches a sandbox-supplied keydown onto the host
`window` as a real `KeyboardEvent`. `validateJavaScriptSurfaceHostKeydown` is shape-only:
it checks types and two 64-character length caps and nothing else, so **any** `key` /
`code` with **any** modifier combination passes. None of Hook's five `window` keydown
listeners filters on `event.isTrusted` (the only `isTrusted` checks in `src/` are the
sticker-drag mouse paths in `app.tsx:2033/2049/2060`), so a synthetic event is
indistinguishable from a user keystroke. The relay also returns at `:696`, *before* the
per-message event budget at `:719`, so unlike surface events it is completely
unthrottled — and unlike the `host-drag-*`, `host-background-double-click`, and
`host-wheel` relays it is not gated on an active host `gestureId`.

Audited what is reachable today, and nothing is destructive: `StickerAnnotationLayer`
only tracks modifier keys, `AppSettingsDialog` / `StickerContextMenuLayer` /
`StickerTopStripPropertyBar` are dismiss/commit handlers on dialogs the surface cannot
open, and `SurfaceConfirmationDialog:56-62` reacts only to `Escape`, which calls
`onDecision(false)` — reject. That last one matters: the permission prompt fails safe, so
a surface can decline its own request but cannot approve it. Hence P2, not P1. The
exposure is that the next global shortcut anyone adds — `Delete`, `Ctrl+S`, `Ctrl+Q` —
becomes sandbox-reachable with no guard at all. Fix direction: allowlist the keys the
relay actually needs, put it behind the event budget, and have host shortcut handlers
ignore untrusted events unless they opt in.

**S2-3 (P3) — the app-wide CSP had to be widened to `script-src 'self' blob:`, and the reason is recorded nowhere.**

`Hook/src-tauri/tauri.conf.json:32` now allows `blob:` scripts for the whole app, not just
the sandbox. This is forced: the blob-URL `import()` at bootstrap `:632-636` needs it, an
iframe `<meta>` CSP can only *narrow* a header-delivered policy, and Tauri injects one
policy for every served document — so the surface requirement leaks out into the main
window's policy, where `blob:` script sources are the classic escalator that turns a
minor DOM injection into arbitrary script execution. The same commit also relaxed
`frame-src` (absent → `'self'`) and `frame-ancestors` (`'none'` → `'self'`), both of which
are genuinely required by the iframe and are harmless with no remote content in play.
Fix direction: either serve `javascript-surface-host.html` with its own CSP header so the
main document can go back to `script-src 'self'`, or, if that is not worth the plumbing,
state the trade-off in a comment next to the CSP and in the surface design doc.

**S2-4 (P3) — the `"*"` target origin at `JavaScriptSurface.tsx:764` is correct but reads as a bug.**

`iframe.contentWindow.postMessage({ type: "surface:init", token, entryBase64, snapshot,
resources }, "*", [channel.port2])` ships the capability token, the full snapshot, and the
`MessagePort` with a wildcard target origin. That is unavoidable — an opaque-origin frame
cannot be addressed by any concrete origin — but it is exactly the line a future reader
will "harden" into `documentUrl`'s origin, silently breaking surface init. It needs a
one-line comment saying so.

### S3 — Hook Rust runtime

**S3-1 (P1 → reclassified, see "Deliberately deferred — cross-device remote Surface") — the entire remote / device-session half of the Surface runtime is unreachable in the shipped binary.**

Every manifest read funnels through one place: `read_default_loom_manifest`
(`loom_connector.rs:478-487`) calls `validate_loom_manifest`, which at `:184-189` rejects
anything that is not an *origin-only http loopback* URL —
"transport.baseUrl must be an origin-only http loopback URL". There is no second entry
point: `validate_loom_manifest_value` has exactly one other caller (`:288`, which also
validates), and the only `LoomManifest` struct literal outside that module is in
`device_session.rs:496`, inside `#[cfg(test)]`. All eight `authorize_surface_request`
call sites in `loom_hook.rs` take their manifest from that one function.

Consequently `authorize_surface_request` always returns at `device_session.rs:102-104`
via `loopback_surface_authorization`, and everything past that line is dead in
production: `validate_secure_loom_base_url`, `register_device_pairing_request`,
`wait_for_approved_device_session`, `create_device_session_attempt`, the Ed25519 identity
file, the session cache and its renewal margin, and `invalidate_surface_sessions` — about
500 of `device_session.rs`'s 611 lines. Same for the transport fork: `loom_hook.rs:316-318`
computes `remote_surface` from the same manifest, so it is always false and
`start_remote_surface_poll_listener` never runs.

This is the largest single thing the post-baseline diff added, and none of it can execute.
Two readings, and the team has to pick one explicitly: either remote surfaces are a
shipped feature, in which case the validator needs a remote mode and this code needs real
end-to-end coverage (today it has only unit tests over hand-built manifests, which is why
the gap was invisible); or remote is staged for later, in which case it should be marked
as such in the docs and behind a feature flag, so nobody assumes the pairing and HTTPS
controls are protecting anything today. Either way S1-1, S1-3 and S3-2 must be fixed
*before* the switch is flipped, not after.

Resolution (2026-08-21): the owner picked the second reading — remote is staged for later.
See "Deliberately deferred — cross-device remote Surface" above. The remaining action here
is the docs note plus the feature flag; S1-1, S1-3 and S3-2 stay open as gate conditions.

> Owner: **Lane B**. **Fixed 2026-08-22 / F13.** The remote half is now behind Hook's
> `remote-surface` Cargo feature, off by default, and `Hook/docs/REMOTE_SURFACE_STAGED.md`
> records the staged status, what the flag covers, and the S1-1 / S1-3 / S3-2 gate conditions
> as work that must land *before* the flag is flipped. The manifest validator was **not**
> relaxed — it is still the only gate. `invalidate_surface_sessions` and
> `DeviceSessionAuthorization::device_id` stay compiled in both combinations because live
> loopback paths use them; with the flag off the former is a documented no-op and
> `authorize_surface_request` refuses a non-loopback endpoint with an error naming the flag and
> the document. Both combinations verified. See `### F13` in
> `docs/progress/phase-78-lane-sync.md`.

**S3-2 (P2, gate condition for remote Surface — see "Deliberately deferred") — four different definitions of "loopback", two of them naive prefix matches with a userinfo bypass.**

- `loom_connector::is_loopback_base_url` (`:236-260`) — correct: parses the URL, rejects
  non-`http`, rejects userinfo, rejects non-`/` path / query / fragment, then matches
  `localhost` or `IpAddr::is_loopback`.
- `loom_hook::loom_base_url_is_loopback` (`:328-338`) — `starts_with` on a lowercased
  string.
- `device_session::validate_secure_loom_base_url` (`:183-199`) — the same `starts_with`
  list, used to decide whether the HTTPS requirement applies.
- `network_proxy::endpoint_is_loopback` (`:72-83`) — parses, but checks only the host.
  Correct for its purpose (bypass the proxy for loopback), just a fourth spelling.

The two prefix matchers accept `http://localhost:8080@evil.com/`: the authority's
*userinfo* is `localhost:8080` and the real host is `evil.com`. In
`validate_secure_loom_base_url` that means the "remote connections require HTTPS" check —
the only thing standing between a device token plus an Ed25519 public-key registration
and a plaintext request to an attacker-controlled host — is skipped for exactly the URL
shape it exists to catch. It is unreachable today only because the strict predicate
guards the single manifest entry point (S3-1), i.e. the safety of the weak check depends
entirely on a check somewhere else, which is the fragile arrangement. Fix direction:
delete the two prefix matchers and call `is_loopback_base_url`. Note the behaviour delta
before swapping: the strict version treats `https://localhost:19820` as *not* loopback
(it requires scheme `http`), which flips the transport fork for that URL.

**S3-3 (P2) — a brand-new `reqwest::Client` is built for every single outbound request.**

Owner: **Lane B** (taken 2026-08-21, out of F8). **Fixed 2026-08-22.** `Hook/src-tauri`
only; Hook is a single crate with its own `target/`, so this did not touch Lane A's build.

`network_proxy.rs` now owns two shared-client entry points — `shared_client(endpoint,
timeout)` and `shared_client_with(endpoint, timeout, flavor, configure)` — backed by a
`OnceLock<Mutex<ClientCache>>`. Every per-request construction was migrated to them:
`loom_hook.rs` × 8 (Surface stream / event / HTTP / remount / lifecycle / confirmation /
cancellation / resource), `device_session.rs::surface_client`, `loom_config.rs::
read_hook_voice_config` (no timeout), `loom_connector.rs` and `talk_connector.rs` (both
manifest-supplied `timeout_ms`), and `lib.rs::download_remote_image_bytes_with_reqwest`
via `shared_client_with(.., "image-search-fetch", |b| b.user_agent(..))`. Each site lost
its `configure …` error arm, since the one remaining `map_err` covers both failures.

**Three clients were deliberately not migrated** because they are already long-lived and
built once per owning struct, not per request: `tea_client.rs::TeaIntakeClient::new`,
`voice/client.rs::HttpTranscriber::new` and `voice/client.rs::HttpTextProcessor::new`.
Routing those through the cache would only add a lock acquisition to their constructors.

**One deliberate deviation from the fix direction below: the cache key excludes
`base_url`.** The key is `(loopback, timeout_millis, flavor)`. Keying on the endpoint would
mint a separate client — and therefore a separate connection pool — per host, which is
backwards: a single `reqwest::Client` already pools connections per host internally, so
one client serving *n* hosts is exactly what the library is designed for. The only inputs
that change how the client is *built* are whether the endpoint is loopback (which decides
`no_proxy()`), the timeout, and the builder customization named by `flavor`. Proxy
generation is likewise not in the key: it is a separate `AtomicU64` compared against
`ClientCache::generation`, so `apply_loom_settings` invalidates the whole map at once.

Invalidation runs on both edges. `apply_loom_settings` now computes whether the setting
actually changed, bumps `PROXY_GENERATION` and eagerly clears the map only on a real
change — so re-applying the same mode does not throw away live pools — and `shared_client_with`
also clears lazily on a generation mismatch. A client whose build races a proxy change is
handed to its one caller but not cached. Lock order is fixed in both directions: the proxy
write lock is released before the cache lock is taken, because `shared_client_with` takes
the cache lock first and the proxy read lock second (inside `apply_to_url`). A poisoned
cache mutex originally degraded to building an uncached client; S3-4 replaced that with
recovery, so the cache survives a panic instead of switching itself off — see below. The
map is capped at `MAX_CACHED_CLIENTS = 32` and cleared on overflow, because
`timeout_millis` is manifest-supplied and a file that varied it per call could otherwise
grow the map without bound.

`apply_to_url` was left unchanged and still public by this fix — it remains the right call
for a site that genuinely needs its own client. Its poisoned-lock fallback is S3-4, fixed
separately below.

Verified: `cargo fmt --check` clean, `cargo clippy --all-targets` adds no new warning in
any touched file, `cargo test --no-fail-fast` green at 274 tests. Three unit tests in
`network_proxy.rs` cover reuse across endpoints sharing a proxy decision, separate entries
per timeout/flavor, eager invalidation on a real proxy change, no invalidation on a
no-op re-apply, and the cap. The two connector source-shape contract tests
(`tests/loom_connector_contract.rs`, `tests/talk_connector_contract.rs`) were updated:
they now assert `network_proxy::shared_client` at the call site plus
`apply_to_url(Client::builder(), endpoint)` inside the shared path, so the loopback-bypass
guarantee they exist to protect is still asserted end to end.

Original finding: nine constructions on the Surface path alone (`loom_hook.rs:1361, 2236,
2482, 2581, 2645, 2680, 2713, 2896` and `device_session.rs:448`), fourteen across the
crate. Each `Client::builder().build()` creates a fresh connection pool and a fresh TLS
configuration, so no connection is ever reused between calls: every art execute, every
surface action, every resource fetch pays a new TCP handshake, and the poll loop of S1-1
pays one per iteration. `reqwest::Client` is already `Clone` and internally shared —
the intended usage is one long-lived client. Fix direction: cache clients per
`(base_url, proxy generation)` in a `OnceLock<Mutex<HashMap<..>>>` and invalidate on
`apply_loom_settings`, which is also the natural place to make a proxy change take effect
without a restart.

**S3-4 (P3) — a poisoned proxy lock silently re-enables the system proxy.**

Owner: **Lane B** (claimed 2026-08-22 — this finding belonged to no batch; it sits in
`Hook/src-tauri/src/network_proxy.rs`, which is Lane B's reserved path, and it is the last
open item in the file S3-3 had just rewritten). **Fixed 2026-08-22.** `Hook/src-tauri` only.

The fallback is gone rather than retargeted. `unwrap_or_default()` was replaced by a
`runtime_proxy()` helper that recovers the configured setting out of the poisoned guard with
`PoisonError::into_inner()`, then calls `RwLock::clear_poison()`. **This is a deliberate
deviation from the fix direction below, which asked for the fallback to become `Disabled`
instead of `System`.** `Disabled` is the right *fallback*, but no fallback is needed: a
poisoned lock means some thread panicked while holding it, which says nothing about the
setting, and the stored value cannot be half-written because the only write is a single
whole-value assignment (`*store = proxy`). So the user's actual choice is still there to be
read, and honouring it beats guessing — `Disabled` would have broken a `Custom`-proxy user's
outbound calls just as surely as `System` broke a `Disabled` user's privacy.

Three supporting changes:

- **`impl Default for RuntimeProxy` was deleted**, with a doc comment saying why. It had
  exactly one caller, the `unwrap_or_default()` this finding is about; leaving a `Default`
  that resolves to `System` in place would keep the footgun loaded for the next reader. This
  also removed a standing `clippy::derivable_impls` warning on the same `impl`.
- **`apply_loom_settings` recovers on the write side too.** It used to map a poisoned lock to
  `Err("无法锁定 Hook 代理设置")`, which the finding correctly calls out as the two halves of
  the module disagreeing. Erroring is the worse half: it means one unrelated panic locks the
  user out of their own proxy settings permanently. It now recovers and clears the poison the
  same way, so the two halves agree in the direction that keeps the setting under user
  control.
- **The client cache mutex got the same treatment** — beyond this finding's letter, but the
  same failure mode in the same file, introduced by S3-3 the day before. `lock_client_cache()`
  replaces three `if let Ok(..) = client_cache().lock()` sites; on poison it clears the poison
  and **empties the map**. The map is not trusted the way the proxy value is, because a panic
  could in principle interrupt a `HashMap` mutation, and an empty cache costs one rebuild per
  key while a half-valid one could serve anything. Without this, a single panic would disable
  client reuse for the life of the process *and* leave `apply_loom_settings` unable to drop
  the clients built for the old proxy — i.e. a proxy change would silently stop taking effect,
  which is the same class of bug one layer down.

Neither recovery path logs the setting itself: a custom proxy address can carry credentials,
so the two `eprintln!` lines name the lock and the action only. `eprintln!` is what the rest
of Hook uses — the crate has no `log`/`tracing` dependency.

Worth recording for whoever reads this next: `src-tauri/Cargo.toml` sets
`[profile.release] panic = "abort"`, so in a shipped `hook.exe` a panic never unwinds and a
lock can never be *observed* poisoned. This fix therefore hardens debug and test builds and
protects against a future profile change; it is not a live user-facing bug today. That is
consistent with the P3 rating and with the finding's own "practically unreachable".

Verified: `cargo fmt --check` clean; `cargo clippy --all-targets` reports nothing in
`network_proxy.rs` at all now (one pre-existing warning removed, none added);
`cargo test --no-fail-fast` green at 276 tests across all nine binaries, up from 274. Two new
unit tests: one poisons the proxy lock — by panicking in a spawned thread while holding the
guard, the only way a lock actually becomes poisoned — and asserts that a user on `disabled`
still reads back `Disabled` rather than `System`, that the poison is cleared, and that a
settings change still applies after a second poisoning; the other poisons the client cache
and asserts the next call rebuilds into an empty-but-working cache. The two connector
source-shape contract tests still pass unchanged: `apply_to_url(Client::builder(), endpoint)`
and the loopback `no_proxy()` early return are both still present verbatim.

Original finding: `network_proxy.rs:92-95` reads the setting with `.unwrap_or_default()`, and
`RuntimeProxy::default()` is `System` (`:14-18`). So if the `RwLock` is ever poisoned,
a user who explicitly set the proxy to `disabled` silently gets the system proxy back —
fail-open on a privacy setting. `apply_loom_settings:66-68` treats the same poisoning as
a hard error, so the two halves of the module disagree. Practically unreachable (the
write critical section is a single assignment), but the fallback should be `Disabled`,
not `System`.

### S4a — Loom surface resources

`surface_resources.rs` is a content-addressed blob store with time-limited leases, and the
security-relevant parts are right: ids must be `sha256:` + 64 lowercase hex
(`normalize_digest:443-451`), so no path component ever comes from a caller and there is no
traversal surface; `get:211` re-verifies size *and* content hash before handing bytes out;
`get_with_lease:230-237` refuses a lease that does not name the requested object;
`validate_references:250-262` requires the client's lease to be byte-identical to the
host-issued one, which is what stops a surface from widening its own grant;
`validate_replacement_transport:371-419` pins the `loom_resource` path to the digest and
checks `width * height * 4 == size` with `checked_mul` for shared memory; and every write
goes through `write_atomic:421-434` (sensitive temp, `sync_all`, atomic replace, temp
cleaned on failure), with the payload written before its metadata so a crash can only
leave an ignored orphan, never a metadata record pointing at nothing.

**S4a-1 (P1) — the resource directory has no garbage collection, and a single missing file makes the whole daemon refuse to start.**

Nothing in the store ever removes a resource. `cleanup_expired:359-362` retains only
`leases`; `self.resources` and the `{digest}.bin` / `{digest}.json` pairs on disk are
write-only, and there is no `unregister` / `prune` in the API (the only `fs::remove_file`
in the file is the temp-file cleanup at `:431`). So every distinct image any surface has
ever produced stays on disk forever, at up to `MAX_SURFACE_RESOURCE_BYTES` = 16 MiB each.
A surface that re-renders on a timer — the shipped `stock-monitor` sample does exactly
that — writes a new object per distinct frame.

The second half is what makes it P1: `new:93-98` returns `Invalid` if any metadata record's
payload is missing or has a different length, and `lib.rs:544-546` propagates that with
`?` through `open Surface resource store`, so daemon startup fails outright — not the
surface subsystem, the whole daemon. An unbounded directory of large binaries is precisely
what a user, an installer, or a disk-cleanup tool eventually prunes, and doing so bricks
Loom with an error that names a resource id. Fix direction: drop unreadable entries at load
with a warning instead of failing (the store is content-addressed, so a lost object can be
re-registered), and add a GC pass that deletes objects with no live lease and no reference
from any persisted surface instance.

**S4a-2 (P2) — every descriptor validation re-reads and re-hashes the full payload from disk.**

`validate_descriptor:266-278` calls `get`, and `get:210-211` does
`fs::read` + `hex_digest` — a full SHA-256 over up to 16 MiB. `validate_references:241-264`
calls `validate_descriptor` once per resource *and* once per lease, and it runs on surface
snapshot/patch updates. So a surface carrying a handful of image resources re-reads and
re-hashes all of them on every revision; `renew_loom_resource_lease:333-357` hashes twice
(once in `get`, once inside `register:152`). Verifying on ingest and on external fetch is
right; re-verifying host-internal state on every patch is not. Fix direction: keep the
verification in `get` for the HTTP fetch path, and have `validate_descriptor` compare
against the in-memory `resources` map, re-verifying from disk only when the file's
`(len, mtime)` changed since it was last checked.

**S4a-3 (P2) — the lease table is unbounded and fully rewritten with an `fsync` on every registration.**

`surface_store.rs` caps pending events (1024) and confirmations (64), but there is no cap
on leases. `persist_leases:364-368` serializes the entire map with `to_vec_pretty` and
`write_atomic`, i.e. one `sync_all` plus an atomic replace per `register` / `release` /
`duplicate` / `replace_lease_transport`. With `MAX_RESOURCE_LEASE_MILLIS` at one hour, a
busy surface accumulates an hour's worth of leases and pays O(n) serialization and a
disk sync on each new one. Fix direction: cap the live lease count per instance, and batch
or debounce the persist.

**S4a-4 (P2) — `duplicate_loom_resource_lease` inherits the source lease's expiry, so fanned-out surfaces can start with an almost-expired grant.**

`:325-328` clones the lease wholesale and only replaces `lease_id`, keeping
`expires_at_ms`. A duplicate handed to a second attachment 14 minutes into a 15-minute
default TTL is valid for under a minute, after which `cleanup_expired` drops it and the
attachment's next `validate_references` fails with `surface_resource_lease_rejected` —
a failure that looks like a protocol error rather than an expiry. `renew_loom_resource_lease`
goes through `register` and does get a fresh TTL, so the two sibling paths disagree. Fix
direction: give the duplicate a fresh TTL (or at minimum floor it at the default).

### S4b — Loom surface store and actions

The parts most likely to be wrong in a patch engine are right here. `apply_patch:487-529`
applies every operation to `let mut next = snapshot.clone()` and only commits with
`*snapshot = next` after `loom_protocol::validate_surface_node_tree` accepts the result, so
a mid-patch failure cannot leave a half-mutated scene; `transaction:1084-1103` additionally
restores a full snapshot of the instance map both when the closure errors and when
`persist` fails; `validate_surface_patch` (`loom_protocol/src/surface.rs:897-902`) rejects
`revision <= base_revision`, so snapshot revisions cannot go backwards; `pointer_tokens:1525-1536`
unescapes `~1` before `~0`, which is the RFC 6901-correct order (the common inversion would
decode `~01` to `/`); `mutate_node_json:1492-1523` allowlists `/props`, `/layout`, `/style`,
`/accessibility`, `/events` and re-checks that a replacement cannot change a node's stable
id; `lifecycle_transition_allowed:1284-1297` keeps `Disposed` terminal. On the action side,
`finish_cancelled:1128-1144` does release its reservation, `request_executor.rs:247` wraps
each job in `catch_unwind(AssertUnwindSafe(..))` so a panicking job cannot shrink the worker
pool, and the production tool resolver (`surface_actions.rs:128-138`) resolves through
`resolve_installed_art_package(root, art_id, art_version, package_digest, ..)` — so
`validate_locked_tool` being `#[cfg(test)]` is the test resolver's stand-in for that pin, not
a missing production check. S6 still owes a check that `resolve_installed_art_package`
actually compares the digest rather than only using it to locate the directory.

**S4b-1 (P2) — a panic inside a surface action job wedges that action forever, with no failure ack.**

`execute_surface_action_job:610-751` calls `release_reservation` only on its normal exit
path (`:745`) and in `finish_cancelled`. Everything between `:625` and `:744` — including
`parse_surface_action_response`, `apply_action_response`, the base64 decode and the resource
writes inside `broker_action_resource_uploads` — can panic, and because
`request_executor.rs:247` catches the unwind, the daemon neither crashes nor notices. Two
things are left behind: for `RejectWhileRunning` the key stays in `reject_reservations`, so
every later invocation of that `instance:action` pair fails with "Surface action ... is
already running" for the daemon's remaining lifetime; and the last persisted ack is the
`Running` one written at `:627`, so Hook waits on a request that will never resolve. Fix
direction: put the reservation release and a `Failed` ack behind an RAII guard whose `Drop`
runs on unwind, or wrap the body in `catch_unwind` locally and convert a panic into
`surface_action_panicked`.

**S4b-2 (P2) — `Serial` concurrency is not enforced once a job times out or is cancelled, and the abandoned runner thread is never reclaimed.**

The runner body runs on a thread spawned at `:641-645`; the `JoinHandle` is bound as
`_runner_thread` inside the `match` arm and dropped when the polling loop breaks, so it is
detached. On timeout (`:662-669`) or cancellation (`:654-660`) the worker sets the
cancellation flag and returns, releasing `_serial_guard` (`:619`) while the runner is still
executing. The next `Serial` job for the same `instance:action` then takes the lock and runs
concurrently with the abandoned one — exactly what `SurfaceActionConcurrency::Serial`
promises will not happen. The flag is only advisory: `execute_tool_with_workflows_timeout`
is what actually bounds the runner, and nothing joins the thread or reports if it outlives
its budget, so a runner that ignores cancellation accumulates one live thread per
invocation. Fix direction: keep the `JoinHandle`, hold the serial lock (and the reservation)
until the thread is joined or a hard abort is confirmed, and log when a runner outlives its
deadline.

**S4b-3 (P2) — every store mutation clones the whole instance map and rewrites the whole store file, and the global store lock is held across package resolution.**

`transaction:1090` does `self.instances.clone()` and `:1098` calls `persist:1105`, which
serializes every instance — descriptors, full scene snapshots, authoritative state, up to
`MAX_PENDING_SURFACE_EVENTS` = 1024 queued events each — and writes the file. All 17
mutating methods go through it, including the per-event ones: `accept_event:732`,
`update_event_ack:1051` (called by `persist_ack` at least three times per action, for
`Running`, then `Succeeded`/`Failed`), and `expire_confirmations:958-994`, which runs its
transaction unconditionally and therefore performs a full-store write on every
`recover_pending` tick even when nothing expired. On top of that, `submit_internal:322-406`
holds the store mutex across `(self.tool_resolver)(&instance.descriptor)` (`:343`, an
installed-package resolve that reads from disk) and `tool.surface_manifest()` (`:344`, a
manifest parse), so unrelated surface HTTP requests serialize behind package I/O. Fix
direction: skip the persist when a transaction changed nothing, debounce or make persistence
incremental (per-instance files, or a dirty flag flushed on an interval), resolve the tool
and manifest outside the lock, and cache the resolved manifest per
`(art_id, art_version, package_digest)`.

**S4b-4 (P2) — `Set` on an array element silently destroys the array, and `Remove` on one silently does nothing.**

`set_json_pointer:1537-1560` walks parents with an "object or else replace" rule: any
ancestor that is not a JSON object — including an array — is overwritten with `{}` before
the walk continues. So a legal-looking `Set { path: "/props/items/0" }` turns
`items: [a, b, c]` into `items: { "0": value }`, losing the other elements, and the node
tree validator has no reason to reject it. `remove_json_pointer:1562-1580` takes the other
branch: a non-object ancestor yields `None` and it returns `Ok(())`, so removing an array
element reports success and changes nothing. Both are reachable from any surface through
`mutate_node_json`'s allowlisted paths. Fix direction: add RFC 6901 array handling (numeric
token plus `-`), and make a type mismatch an explicit `Invalid` error in both helpers rather
than a silent coercion or a silent no-op.

**S4b-5 (P3) — `MoveNode` detaches the subtree before the checks that can still fail, and reports the wrong reason.**

`apply_operation:1395-1417` calls `remove_node(root, node_id)` at `:1405`, then looks up the
destination parent at `:1407` and bounds-checks the index at `:1410`; either can return
`Err` with the subtree already detached and now owned by a local. Today this is contained —
`apply_operation` has exactly one caller, `apply_patch:514`, which operates on the clone
described above — so nothing is persisted and the bug is latent. It stops being latent the
moment a second caller passes a live tree. The error text at `:1408` is also wrong: it says
"a node cannot move into itself" for any missing parent id. Fix direction: resolve and
validate the destination first, then detach; correct the message.

**S4b-6 (P3) — two byte-identical private copies of the request-id derivation, coupled by an equality check.**

`surface_store.rs:1299-1304` and `surface_actions.rs:1121-1126` are the same function under
two names (`surface_request_id`, `request_id_for_event`). `reserve_action:517` records the
`surface_actions` version in `latest_requests` while the ack carries the `surface_store`
version, and `is_latest:591-608` compares the two by string equality. If either copy ever
changes, every `ReplaceLatest` / `Coalesce` action is immediately classified as superseded
and cancelled — a silent, total feature failure with no error anywhere. Fix direction: move
it into `loom_protocol` (or expose the store's copy) and call the one function from both
sites.

**S4b-7 (P3) — uploads are registered before the patch that references them is validated or applied, and alias substitution rewrites the entire response.**

`broker_action_resource_uploads:1027-1039` registers each upload into the resource store,
taking a lease, and only afterwards does the alias round-trip (`:1042-1054`) and — back in
`apply_action_response` — the patch application and `validate_action_response_resources`.
Any failure after `:1039` leaves the payload on disk with a live lease and no referencing
snapshot; combined with S4a-1 (no `unregister`, no prune) those bytes are never reclaimed,
not even after the lease expires. Two smaller issues in the same path:
`replace_surface_resource_aliases:1066-1085` walks every string in the serialized response
and replaces any that equals `surface-upload:<id>`, so a label or log string that happens to
match is rewritten too — substitution should be scoped to resource-reference positions; and
the 16 MiB budget at `:1021` is checked only *after* `BASE64.decode` (`:1006`), so an
oversized upload is fully decoded into memory before being rejected, when
`upload.data_base64.len() / 4 * 3` gives a cheap pre-check. Fix direction: stage uploads,
commit them only once the patches validate and apply, and roll back (or immediately release)
on failure.

### S5a — Loom daemon HTTP front door

`request_executor.rs` is the strongest file reviewed so far. The queue is a bounded
`mpsc::sync_channel`, so `try_submit:191-199` gives real backpressure and the accept loop
answers `Full` with a 503 instead of growing a queue (`lib.rs:695-698`); the receiver mutex is
held only across `recv()` and released before the handler runs (`:237-247`), so the workers do
run concurrently; `catch_unwind(AssertUnwindSafe(..))` at `:247` means a panicking job cannot
shrink the pool; a mid-way spawn failure drops the sender and joins the workers already
started (`:176-182`); `Drop` and `shutdown` both close then join; and `from_env:69-77`
validates `LOOM_DAEMON_WORKERS` / `LOOM_DAEMON_QUEUE_CAPACITY` against `1..=32` / `1..=1024`
rather than silently clamping. On the HTTP side the header cap (`MAX_HTTP_HEADER_BYTES`
= 16 KiB) is enforced before a terminator is even found, body caps are per-route, and
`is_reserved_probe:1019-1025` deliberately answers `GET /health` and `GET /status` on the
accept thread so liveness probes still work when the pool is saturated.

**S5a-1 (P1) — the Surface long-poll holds the daemon's global route lock for its entire 5-second wait.**

`handle_request_job:1147-1165` takes `runtime.serialized_route_lock` for the whole of
`route_request` whenever `request_concurrency_class` returns `Serialized`, and that function's
last arm is `_ => RequestConcurrencyClass::Serialized` (`:1015`). `GET /v1/surfaces/stream`
matches none of the `Concurrent` arms, so it is `Serialized` — and `poll_surface_stream:4186-4221`
blocks in `hub.wait_after(cursor, remaining)` for up to `timeout_ms` (default and maximum
5 000 ms) whenever there is nothing to deliver. So an idle long-poll owns the global lock for
five seconds, and every `POST /v1/surfaces/{id}/events`, `/patch`, `/resources` — all
`Serialized` too — waits behind it. It is worse than a simple stall: the message that would
end the poll early can only be produced by a request that is itself blocked on the lock the
poll is holding, so each surface interaction during an idle window pays the full five
seconds. The dedicated `surface_stream_executor` (`SURFACE_STREAM_WORKERS` = 8,
`lib.rs:624-629`) buys nothing, because its jobs contend for the same lock as the request
pool. The curated `Concurrent` list at `:987-996` shows the intent — `/v1/hook-bridge/canvas`
and `/v1/mcp/registry` are both listed — `/v1/surfaces/stream` was simply missed. Fix
direction: classify `GET /v1/surfaces/stream` as `Concurrent` (it only reads the broadcast hub
and the instance store, each of which has its own lock), and add a regression test asserting
that a POST completes while a long-poll is parked. `request_concurrency_classification_is_conservative`
(`:21172`) currently asserts the opposite of what is wanted here, so it needs updating with
the fix.

**S5a-2 (P1) — the accept thread reads each request itself, so one trickling client stops the whole daemon; there is also no write timeout.**

`serve_until:641-645` calls `read_connection(stream)` inline, before anything is handed to a
worker, and `read_http_request:1651-1690` loops on `stream.read` until the body is complete.
The only bound is `set_read_timeout(2s)` at `:738`, which is a per-`read` timeout, not a
per-request one: every byte that arrives re-arms it. A client that sends one byte every
1.9 seconds therefore holds the accept thread until the 16 KiB header cap trips — roughly
eight hours — and during that time the daemon accepts no connections at all, so even the
reserved `/health` lane is unreachable. Any local process can do this, and so can any web
page in the user's browser via a streamed `fetch` body to the loopback port. Separately,
nothing calls `set_write_timeout`, so `write_response` blocks indefinitely once a peer stops
reading and the socket buffer fills; for a large response (`support_bundle`, canvas
snapshots) that parks a worker permanently, and on the probe / 503 / shutdown paths
(`:657`, `:666`, `:682`, `:697`) it parks the accept thread. Fix direction: give the whole
request read a wall-clock deadline (a few seconds) in addition to the per-read timeout, move
the read off the accept thread into a small reader stage, and set a write timeout on every
accepted stream.

**S5a-3 (P2) — a package upload is copied three times and read 512 bytes at a time.**

`read_http_request:1653` reads into a 512-byte buffer, so a `MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES`
= 96 MiB install performs about 196 000 `read` syscalls. The accumulated `Vec` is then
converted with `String::from_utf8_lossy(&request).to_string()` (`:1688`), which copies it,
and `ParsedHttpRequest::from_raw:1789` copies the body a third time into `body: String` —
roughly 288 MiB resident for one install, before the base64 payload inside is decoded. Fix
direction: read in 64 KiB chunks, and keep one owned buffer, handing the body out as a slice
or by moving the `String` rather than re-copying it.

**S5a-4 (P3) — the body-size limit is chosen from the raw request-line path, while routing strips the query string.**

`request_body_size_limit:1721-1742` compares the path with `matches!(path, "/v1/frameworks/install" | "/v1/arts/install")`,
`path == "/v1/mcp/servers/install"` and `path == "/v1/surfaces/resources"` — exact matches on
the unsplit target — whereas every routing and classification site uses
`path.split('?').next()` (`:983`, `:1022`, `:3335`). So `POST /v1/surfaces/resources?foo=1`
silently falls back to `MAX_HTTP_BODY_BYTES` = 1 MiB and a legitimate 16 MiB resource upload
is rejected with 413 by the reader, before any handler can explain why. Fix direction: split
the query off once, in one helper, and use it everywhere including the size-limit lookup.

**S5a-5 (P3) — the daemon token is compared with `==`, and the `Content-Length` handling disagrees with how the body is actually split.**

`has_bearer:1793-1803` compares the presented token with `parts.next() == Some(token)`, a
byte-wise comparison that short-circuits on the first mismatch; for a long-lived local
credential a constant-time comparison is the cheap, conventional hardening.
`content_length:1754-1764` takes the *first* parsable `Content-Length` and treats a missing or
unparsable one as `0`, while `ParsedHttpRequest::from_raw:1776` treats everything after the
first `\r\n\r\n` as the body regardless — so a duplicated or malformed header makes the
reader's view of the body length and the parser's disagree. With `Connection: close`,
one request per connection and no pipelining this is currently harmless, but it is the exact
shape that becomes a smuggling primitive the moment a proxy is put in front of the daemon —
and this repository does ship a Gateway. `Transfer-Encoding: chunked` is likewise neither
supported nor rejected: it lands as a body of raw chunk framing and fails later as invalid
JSON. Fix direction: reject duplicate or unparsable `Content-Length` with 400, reject
`Transfer-Encoding` with 501, and use a constant-time token comparison.

### S5b — Loom daemon persistence and platform

Reviewed: the sensitive-file write helpers (`create_sensitive_temporary`
`apps/daemon/src/lib.rs:1293-1335`, `replace_sensitive_file` `:1337-1361`,
`restrict_sensitive_path_permissions` `:1363-1365`, `extended_windows_path` `:1367-1400`,
`sync_sensitive_parent` `:1402-1413`), every non-test persist site in the daemon,
`repair_legacy_control_plane_permissions` (`:1437-1494`), and the router's authentication
and authorization gate (`route_request` `:1040-1105`, `route_with_runtime` `:3333-3366`,
`is_public_device_auth_route` `:3201-3208`, `device_session_route_allowed` `:3210-3226`,
`authenticate_http_device_session` `:3228-3253`).

Confirmed correct — do not re-report:

- `create_sensitive_temporary` uses `create_new(true)` with a pid + nanosecond + attempt
  name and 100 retries, so two writers cannot collide on the same temporary regardless of
  clock resolution; `0o600` is applied at open time on Unix rather than after the fact.
- `replace_sensitive_file` on Windows uses `MoveFileExW` with
  `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`, which is the correct primitive —
  `fs::rename` would fail on an existing destination.
- `write_local_capability_manifest` (`:1205-1283`) is the reference implementation:
  restrict directory, write temporary, `sync_all`, restrict temporary, restrict the old
  target before replacement, replace, restrict the new target, `sync_sensitive_parent`,
  and remove the temporary on any error.
- `repair_legacy_control_plane_permissions` holds a process-wide `REPAIR_LOCK`, treats
  `NotFound` as success at every step, and repairs the two legacy private stores *before*
  testing existence — the comment at `:1452-1454` correctly explains why (`is_file()` can
  itself fail under the legacy ACL).
- `device_session_route_allowed` and `is_public_device_auth_route` are both fed
  `route_path` (query already stripped at `:3333-3337`), so `?` cannot be used to slip a
  device session past its allowlist.

#### S5b-1 (P1) — the entire authentication gate is skipped by default, and no request is checked for origin

Every authentication branch is wrapped in `if let Some(token) = auth_token`
(`lib.rs:1043`, `:1070`, `:1089`, `:3343`). `auth_token` defaults to `None` (`:280`) and is
only *required* when the bind host is not loopback (`:460`); `LOOM_DAEMON_TOKEN` is
documented as "required for non-loopback binds" (`:158`). The default shipping
configuration is therefore a loopback daemon with no authentication at all, and the route
table that becomes reachable includes `POST /v1/plugin-credentials/reveal` (`:3727`),
`POST /v1/publisher-identity/private-key` (`:3717`), `POST /v1/arts/install` (`:3733`),
`POST /v1/frameworks/install` (`:3730`), `POST /v1/mcp/servers/install` (`:3636`) and
`POST /v1/mcp/call` (`:3633`) — that is, plaintext secret disclosure plus package
installation and process launch.

The daemon also never inspects `Origin`, `Host`, or `Sec-Fetch-*` on any request, and
never requires a JSON `Content-Type`. A page open in any browser on the machine can
therefore reach these routes with a CORS-simple `fetch` (`Content-Type: text/plain`, no
preflight), and a DNS-rebinding name resolving to `127.0.0.1` defeats the
loopback-only bind. The device-pairing routes that are deliberately public
(`/v1/devices/requests`, `/v1/device-sessions/challenges`, `/v1/device-sessions`) are
sound in isolation, but they are moot while the token is absent.

Fix direction: generate a random token on first start and persist it in the
already-ACL-restricted control-plane root instead of defaulting to `None`; make the gate
unconditional (fail closed when no token can be loaded); and reject requests whose `Host`
is neither a loopback literal nor `localhost`, and whose `Origin` is present but not an
allowed local origin.

#### S5b-2 (P2) — the device registry is written non-atomically and silently resets to "local host only" if that write is torn

`DeviceRegistryStore::persist` (`:2906-2911`) uses a plain
`fs::write(&self.path, serde_json::to_vec_pretty(&document)?)`: truncate in place, no
temporary, no `sync_all`, no parent flush — even though the atomic helpers live in the
same file and the surface store and MCP registry cache both use them. The loader
(`:2845-2856`) then treats *any* read or parse failure as `unwrap_or_default()`, inserts
the synthetic local host, and immediately calls `persist()` (`:2894-2896`). A crash, power
loss, or full disk during the write leaves truncated JSON; the next start silently
rewrites the file with a single approved local device, discarding every paired device
together with its `public_key`, `key_fingerprint`, and `session_epoch`. The `session_epoch`
loss is the sharpest edge: it is the revocation counter, so a device that was revoked by
bumping its epoch can be resurrected by re-pairing against a registry that no longer
remembers the revocation.

Fix direction: route `persist` through `create_sensitive_temporary` +
`replace_sensitive_file` + `sync_sensitive_parent`, and distinguish "file absent"
(legitimately empty registry) from "file present but unparsable" — the latter should
refuse to start, or quarantine the corrupt file, not silently discard authorizations.

#### S5b-3 (P2) — `save_publisher_identity` deletes the live file before writing the replacement

`save_publisher_identity` (`:9269-9284`) does not use the sensitive-write helpers. It
writes a **fixed** temporary name `.publisher-identity.json.tmp`, then
`fs::remove_file(&path)` (`:9280`), then `fs::rename(&temporary, &path)` (`:9282`). The
window between the remove and the rename has no identity file on disk at all, so an
interruption there loses the publisher identity permanently rather than leaving the
previous version intact — which is the entire point of the temporary. Additionally: the
fixed temporary name means two concurrent callers overwrite each other's partial bytes,
there is no `sync_all`, and neither the temporary nor the final file is passed to
`restrict_sensitive_path_permissions` (confidentiality is only inherited from the
control-plane root ACL, so this depends on `repair_legacy_control_plane_permissions`
having already run).

Fix direction: replace the body with the same
`create_sensitive_temporary` / `sync_all` / restrict / `replace_sensitive_file` /
`sync_sensitive_parent` sequence used by `write_local_capability_manifest`; the Windows
`MoveFileExW` path already replaces an existing destination, so the `remove_file` is not
needed on any platform.

#### S5b-4 (P2) — Loom rewrites Hook's canvas file in place, with no atomicity and no cross-process interlock

`write_hook_canvas_root` (`:14427-14436`) serializes the whole canvas document and calls
`fs::write(path, &bytes)` directly on Hook's canvas file. Two problems compound: the write
is a truncate-then-fill, so a Hook process reading concurrently can observe a
zero-length or half-written document and fail to parse it; and there is no lock, lease, or
generation check shared with Hook, so a Loom write can silently clobber an edit Hook made
between Loom's read and Loom's write. This is the viewer writing the editor's authoritative
file — the exact direction the Hook/Loom boundary is supposed to forbid, so the atomicity
fix should be accompanied by a decision about whether this route should exist at all.
`persist_mcp_servers_snapshot` (`:1518-1531`) has the same non-atomic shape, made
conspicuous by `persist_mcp_registry_cache` immediately below it (`:1559-1587`) doing it
correctly: the *cache* is crash-safe while the *authoritative* server store is not.
`LoomSettingsStore::save` (`:2492-2499`) is the third instance, with the same silent
`unwrap_or_default()` loader at `:2483-2489`.

Fix direction: give the daemon one `write_json_atomically(path, value)` helper built on the
existing primitives and route all four sites (`persist_mcp_servers_snapshot`,
`LoomSettingsStore::save`, `DeviceRegistryStore::persist`, `write_hook_canvas_root`)
through it, so "which persist site is crash-safe" stops being per-site trivia.

#### S5b-5 (P3) — a failed ACL repair aborts startup, and skipped entries are never retried

`repair_legacy_control_plane_permissions` is called with `?` during runtime construction
(`:498-503`), so any error it returns aborts daemon startup with
`repair Loom control-plane permissions in <root>`. Every error path other than `NotFound`
propagates — including transient Windows failures such as a file held open by an
anti-virus scanner or a `SetNamedSecurityInfoW` denial on an entry the current user no
longer owns. A permissions hiccup on one file in the control-plane tree therefore bricks
startup entirely, even though the daemon could run with the tree left as-is.

The marker is also final: `repair_private_tree_permissions` returns the entries it could
not touch, the count is logged and written into the marker body
(`fs::write(&marker, format!("2 skipped={}\n", ...))`, `:1487`), and then the marker's mere
existence short-circuits all later runs (`:1475-1477`). Entries skipped once are never
retried, and the marker records the count without recording *which* entries, so there is
nothing to retry from. Note the marker write itself uses plain `fs::write` — consistent
with S5b-4.

Fix direction: treat a repair failure as a warning plus a degraded-mode log rather than a
startup abort (the non-loopback bind is what genuinely warrants failing closed), and write
the skipped paths into the marker so a later version can re-attempt exactly those.

#### S5b-6 (P3) — a stale device credential masks a valid administrator credential, and `/health` leaks the executable path unauthenticated

`route_with_runtime` calls `authenticate_http_device_session` at `:3339` and returns
`device_auth_error_response(error)` immediately on failure (`:3341`), *before* the
administrator bearer is examined at `:3344`. A request that carries both a valid admin
bearer and an expired or revoked `Authorization: Device …` credential — the natural state
of a desktop client that has just been re-paired — is rejected with a device error even
though its admin credential alone would have authorized it. The fix is to evaluate the
admin bearer first and only surface device errors when no admin credential was presented.

Separately, `GET /health` is public by design (`is_public_device_auth_route:3202`), but
`HealthResponse` (`:3379-3389`) returns `pid` and `executable_path` — the full filesystem
path of the running binary. Combined with S5b-1's browser reachability, that is a free
install fingerprint (user name, install root, and whether this is a dev or packaged build)
for any page that can reach loopback. `/health` should answer `status` and `version` only,
and move `pid` / `executable_path` behind the administrator credential where `/status`
already lives.

### S6a — Loom art credentials and settings

Reviewed: `crates/loom_tool_registry/src/credentials.rs` in full, `art_settings.rs`
(store, `apply_settings_metadata`, `merge_tool_arguments`, `resolve_tool_value_bindings`),
and `resolve_installed_art_package` (`install.rs:950-1075`) to settle the debt S4b left open.

S4b debt closed — `resolve_installed_art_package` does verify the digest, and does far more
than locate a directory:

- it recomputes `canonical_package_digest` over the on-disk version directory and compares
  it to the caller's digest (`install.rs:1006-1015`), skipping non-matching candidates;
- it requires the version directory name to end with the first 12 hex characters of the
  computed digest (`:1022-1026`), so a renamed directory is rejected rather than trusted;
- it rejects both "no match" and "more than one match" (`:1029-1038`);
- it loads the trust store, verifies the package signature, and enforces the effective
  trust policy (`:1040-1053`);
- it verifies the lockfile for the art and its framework dependencies (`:1055-1066`).

Also confirmed correct — do not re-report:

- `activation.local_authoring` is derived from the install source
  (`local_authoring: source == ArtInstallSource::LocalAuthoring`, `install.rs:649`), not
  from package data, so the trust-policy bypass at `:1050` / `:1175` / `:1446` / `:1723`
  cannot be claimed by a downloaded package.
- `CredentialStore::write_file` (`credentials.rs:451-478`) is the correct sequence:
  restrict the parent (creating it if absent), unique temporary, `sync_all`, restrict the
  temporary, atomic replace, restrict the target, and remove the temporary on error.
- `CredentialStore::read_file` (`:441-449`) propagates a JSON parse error instead of
  silently defaulting — the opposite of the device registry (S5b-2) and the right choice.
- `validate_input` (`:524-549`) rejects unsafe credential names and scope references,
  rejects empty values, canonicalizes by value type, and requires RFC 3339 expiry.
- `merge_tool_arguments` (`art_settings.rs:228-273`) removes secret parameters and
  value-binding ids from the merged defaults, so user settings cannot inject a value into
  a parameter that is supposed to come from a credential.

#### S6a-1 (P2) — the publish ownership check trusts package-supplied metadata

`art_is_locally_authored` (`art_settings.rs:189-194`) returns true whenever the tool
manifest's `metadata.authoring` is a JSON object. That predicate is what guards two
authority decisions in the daemon: `/v1/arts/store/publish` rejects with
`art_publish_not_owned` / "只能发布当前用户本地创建的 Art" only when it returns false
(`lib.rs:8486-8495`), and identity editing (`name` / `description`) is likewise gated on it
(`lib.rs:9931-9940`, surfaced to the UI as `locallyAuthored` / `canEditIdentity` at
`:8448-8449`).

`metadata` is package content. Any downloaded or store-installed art that ships
`"metadata": {"authoring": {...}}` therefore presents itself as locally authored, so it can
be renamed and re-published to the store under the current user's publisher identity and
signature. The correct source of truth already exists and is used for the trust decisions:
`read_art_activation(...).local_authoring`, derived from the install source. The two
notions of "locally authored" disagree, and the weaker, attacker-controlled one guards
ownership.

Fix direction: gate publish and identity editing on the activation flag (optionally
requiring both), and keep `art_is_locally_authored` for presentation only.

#### S6a-2 (P2) — credential scoping is not enforceable against art code, because art code can decrypt the store itself

Secrets are protected with `CryptProtectData` under `CRYPTPROTECT_UI_FORBIDDEN` and no
optional entropy (`credentials.rs:613-649`), i.e. DPAPI CurrentUser. The file ACL restricts
access to the current user, and framework and art child processes run *as* that user. So
any art package — the very principal the `CredentialScope` machinery exists to constrain —
can read `plugin-credentials.json` and call `CryptUnprotectData` on every entry, bypassing
`grants_for` / `grants_for_bindings` entirely. Scoping is therefore advisory against art
code and only load-bearing against other user accounts.

Adding entropy would not fix it (any constant the daemon uses is readable in its binary).
The options are real ones with real cost: run art and framework processes under a
restricted token or separate account so the ACL becomes meaningful; or keep secrets
daemon-side and hand child processes short-lived, scope-checked handles instead of values.
This should be recorded as a known boundary of the model even if the fix is deferred —
today the UI implies a stronger guarantee than the runtime provides.

Note the non-Windows path is honest but must not be shipped: `protect_value` there is
`BASE64.encode` labelled `local-file-base64` (`:697-699`), and that label is surfaced
verbatim as the credential's `protection` field.

#### S6a-3 (P2) — which credential satisfies a binding depends on file order

`grants_for_bindings` (`credentials.rs:296-351`) picks a candidate with
`max_by_key(|c| usize::from(c.scope.framework_id.is_some()) + usize::from(c.scope.art_id.is_some()))`.
A framework-only scope and an art-only scope both score 1, and Rust's `max_by_key` returns
the *last* maximal element, so when two credentials share a name — one scoped to the
framework, one scoped to the art — the winner is whichever appears later in
`plugin-credentials.json`. Re-saving an unrelated credential can reorder the file and
silently change which secret an art receives. `grants_for_mcp_bindings` (`:353-400`) has the
same shape but only one specificity axis, so it is unambiguous.

Fix direction: make the ranking total — weight `art_id` above `framework_id` (the narrower
scope should win), and reject or warn on a genuine tie rather than resolving it by
position.

#### S6a-4 (P3) — a malformed expiry means "never expires"

All four resolution paths test expiry as
`expires_at.and_then(|v| DateTime::parse_from_rfc3339(v).ok()).is_some_and(|e| e <= now)`
(`credentials.rs:276-283`, `:321-326`, `:373-378`, `:417-422`). An `expires_at` that fails
to parse yields `None`, `is_some_and` yields false, and the credential is treated as valid
forever. `validate_input` does reject a malformed expiry on write, so this only bites files
edited by hand or written by an older schema — but it fails in the unsafe direction, and
the safe reading is one line away: treat unparsable as expired.

#### S6a-5 (P3) — the art-settings writer uses a fixed temporary name and skips permission hardening

`ArtSettingsStore::write_file` (`art_settings.rs:172-186`) writes to
`self.path.with_extension("json.tmp")` — a single fixed path — via `fs::File::create`,
which truncates. Two concurrent saves therefore interleave into the same temporary and the
loser renames a corrupt merge over the real file. It also never calls
`restrict_private_path_permissions`, unlike the credential writer in the same crate, so the
file (which records `credentialBindings`, i.e. which secret each art parameter draws from)
inherits whatever the parent directory grants. `read_file` (`:165-170`) does propagate
parse errors rather than defaulting, which is right.

Fix direction: reuse the credential store's `create_sensitive_temporary` +
`restrict_path_permissions` pair; both live in this crate already.

#### S6a-6 (P3) — value bindings can only resolve globally scoped credentials

`resolve_tool_value_bindings` (`art_settings.rs:277-300`) resolves through
`global_values_for_bindings`, which matches only credentials with *all three* scope fields
`None` (`credentials.rs:412-422`). Secret bindings for the same art go through
`grants_for_bindings`, which accepts framework- and art-scoped credentials. So a credential
the UI let the user scope to one art resolves fine as a secret binding and fails as a value
binding with `MissingBinding`, naming a credential that plainly exists. Either accept
scoped credentials here with the same specificity rule, or make the UI refuse to offer a
scoped credential as a value binding.

#### S6a-7 (P3) — every credential read performs an ACL write, and read-modify-write has no lock

`read_file` calls `loom_plugin_security::restrict_private_path_permissions(&self.path, false)`
before every read (`credentials.rs:442`) — a security-descriptor write on what is otherwise
a pure read, executed on the per-execution grant-resolution path (`grants_for`,
`grants_for_bindings`, `grants_for_mcp_bindings`, `global_values_for_bindings` each call it,
and `resolve_tool_value_bindings` constructs a fresh store per invocation). Hardening on
write plus a periodic repair would cost nothing per call.

Separately, `upsert` and `delete` are read-modify-write over the whole file with no file
lock and no in-process mutex, so two concurrent mutations lose one of the two. In the
daemon this is currently masked by the global `serialized_route_lock` (S5a-1), which means
the correctness of the credential store depends on an unrelated routing decision — worth an
explicit lock if that classification is ever relaxed.

### S6b1 — Loom art package extraction, outbound policy, runtime dependency resolution

Files: `crates/loom_tool_registry/src/secure_zip.rs` (243), `network_policy.rs` (301),
`dependency.rs` (230), plus the extraction call sites in `install.rs` and `framework.rs`.

Confirmed correct — do not re-report:

- `extract_zip_securely` uses `ZipFile::enclosed_name()` and then re-validates through
  `validate_relative_path` (every component must be `Component::Normal`, rejects `:`,
  trailing dot or space, and Windows reserved names), so traversal and drive-relative
  paths are covered twice (`secure_zip.rs:39-123`, `130-156`).
- Duplicate entries are rejected after case folding (`normalize_relative_path`), so a
  package cannot overwrite an earlier entry through case variation on Windows.
- Symlink entries are rejected by mode (`unix_mode() & 0o170000 == 0o120000`), and both
  the destination and its parent are checked with `symlink_metadata` before the write, so
  a symlink planted by an earlier entry cannot redirect a later one.
- Files are created with `OpenOptions::new().write(true).create_new(true)`, so extraction
  can never overwrite an existing file.
- Art install extracts into a nonce-named staging directory and removes it on any error
  (`install.rs:522-527`, `708-710`), so a failed art extraction leaves nothing behind.
- `secure_client` re-validates every redirect hop against the policy and caps hops at
  `max_redirects` (`network_policy.rs:86-107`); `read_bounded_response` checks the declared
  `content_length()` and then still reads through `take(max_bytes + 1)`
  (`network_policy.rs:125-148`).
- Outbound requests are HTTPS-only; plain HTTP is permitted only for explicit loopback
  literals under `allow_http_loopback` (`network_policy.rs:174-188`).
- `resolve_dependencies` selects the highest matching semver and fails closed on a digest
  pin mismatch when versions did match (`dependency.rs:45-64`).

#### S6b1-1 (P2) — the zip-bomb accounting trusts declared sizes, so real disk writes are ~130× the intended cap

`extract_zip_securely` accumulates `total_uncompressed` from `entry.size()` — the value
declared in the archive's own headers — and compares it against `MAX_UNCOMPRESSED_BYTES`
(512 MiB) at `secure_zip.rs:80-84`. The compression-ratio guard directly below it
(`:86-92`) is also driven by declared values, and it only fires when the declared
`entry_size` exceeds `1024 * 1024`:

```rust
if entry_size > 1024 * 1024
    && (compressed_size == 0 || entry_size / compressed_size.max(1) > MAX_COMPRESSION_RATIO)
```

An archive that declares 1 KiB per entry skips the ratio test entirely and keeps the
declared total at 4 MiB, well under the 512 MiB cap. The only bound that survives on the
actual byte stream is `copy_bounded(&mut entry, &mut output, MAX_ENTRY_BYTES)`, which is
per entry (128 MiB) and is never accumulated. With `MAX_FILES = 4096` the theoretical
write volume is 512 GiB; the practical ceiling is the 64 MiB compressed budget at
deflate's maximum expansion (~1032:1), roughly 66 GiB — about 130× the intended limit,
written to the user's disk before the install fails.

Fix: accumulate the value `copy_bounded` returns (the bytes actually written) into a
running total and abort as soon as that total exceeds `MAX_UNCOMPRESSED_BYTES`. The
declared-size checks can stay as a cheap early rejection, but they must not be the only
accounting. Bounding each `copy_bounded` call by `MAX_UNCOMPRESSED_BYTES - written_so_far`
makes the limit exact rather than approximate.

#### S6b1-2 (P2) — an IPv6 answer of `::ffff:127.0.0.1` passes the private-address filter

`validate_ip` treats loopback specially and then classifies the remainder through
`ipv6_is_private_or_special`, which only recognises unspecified, multicast, `fc00::/7`
and `fe80::/10` (`network_policy.rs:245-250`). IPv4-mapped IPv6 addresses are none of
those, and `Ipv6Addr::is_loopback()` is true only for `::1`, so `::ffff:127.0.0.1`,
`::ffff:169.254.169.254` and `::ffff:10.0.0.1` are all reported as public and allowed.
On a dual-stack host, connecting to a mapped address reaches the corresponding IPv4
address, so an art whose domain resolves to an AAAA record in `::ffff:0:0/96` reaches
loopback services and the cloud metadata endpoint. No rebinding is required — one DNS
answer is enough.

The same missing normalisation also breaks the legitimate path: `Url::host_str()` returns
IPv6 literals with brackets (`[::1]`), which never parse as `IpAddr`, so
`host_is_loopback_literal("[::1]")` is false and the `host.parse::<IpAddr>()` fast path at
`network_policy.rs:158-160` misses too. `http://[::1]:8080` is therefore rejected even
with `allow_http_loopback` enabled, while the mapped form above is accepted.

Fix: strip brackets before parsing, and canonicalise through `to_ipv4_mapped()` /
`to_ipv4()` so a mapped address is classified by the IPv4 rules. Also reject NAT64
`64:ff9b::/96`, which translates to arbitrary IPv4 destinations.

#### S6b1-3 (P2) — the outbound guard resolves the host, then lets reqwest resolve it again

`validate_outbound_url` resolves the host with `(host, port).to_socket_addrs()` and
validates every returned address (`network_policy.rs:161-171`), but what it hands to the
client afterwards is the URL, not a validated address. reqwest performs its own,
independent resolution when the request is issued. A name served with a short TTL that
answers with a public address for the check and a private or loopback address for the
connection defeats the entire filter — the classic resolve-then-fetch time-of-check /
time-of-use gap. The per-redirect re-validation in `secure_client` has the same shape and
so inherits the same gap.

Fix: resolve once, pick a validated address, and pin it — `ClientBuilder::resolve` /
`resolve_to_addrs` for the checked host, or a custom `dns::Resolve` implementation that
returns only addresses that passed `validate_ip`. Pinning also removes the duplicated
resolution cost.

#### S6b1-4 (P2) — a framework runtime is deleted before its replacement is validated

`unpack_runtime_zip` removes the existing runtime directory and only then extracts
(`framework.rs:1941-1946`):

```rust
if runtime_dir.exists() {
    fs::remove_dir_all(runtime_dir)?;
}
fs::create_dir_all(runtime_dir)?;
crate::secure_zip::extract_zip_securely(zip_bytes, runtime_dir)
    .map_err(|error| fail(error.to_string()))?;
```

If extraction fails for any reason — a rejected entry, a size cap, a mid-stream I/O
error — the working runtime is already gone and a half-extracted tree is left in its
place, with no cleanup on the error path. Every framework that depends on that runtime is
then broken until it is reinstalled, and because the partial tree exists,
`RuntimeRegistry::prune_stale` and `resolve`'s `path.is_dir()` filter still consider the
runtime present. Art install already has the right shape for this (staging directory,
then rename, with staging removed on error); the runtime path should use it too: extract
to a sibling staging directory, then replace the target only after extraction succeeds.

#### S6b1-5 (P2) — the registered runtime version is a constant, so semver resolution is decorative

Both `register_framework_runtimes` and `resolve_framework_dependencies` label any
`python-embed` directory as `loom.runtime.python` at whatever `LOOM_PYTHON_RUNTIME_VERSION`
says, defaulting to the literal `"3.12.0"` (`framework.rs:1541-1547`, `1572-1585`). The
version is never derived from the runtime itself. A framework shipping a 3.11 or 3.13
embed is registered as 3.12.0, and a manifest declaring `^3.12` then resolves
successfully against a runtime that does not satisfy it. Since `loom.runtime.python` is
currently the only runtime kind, the whole `VersionReq` mechanism in `dependency.rs` is
resolving against a value the framework never supplied.

Fix: read the real version out of the embed (the `pythonXY._pth` / `python3Y.dll` name, or
`sys.version` from a one-shot probe) and register that; fall back to rejecting the embed
rather than assuming a version. The environment override is a reasonable escape hatch but
should not be the primary source.

#### S6b1-6 (P3) — an empty `allowed_domains` allows every host

`domain_allowed` returns `true` when the allow-list is empty (`network_policy.rs:191-193`),
and `OutboundPolicy::default()` has `allowed_domains: vec![]`. The error message the
non-empty path produces — "URL host is not declared by the package" — describes a
guarantee that silently does not exist for a package that declares nothing. The IP filter
still applies, so this is not full SSRF, but the domain restriction is fail-open exactly
where a package has given the least information about its intent. Fail closed instead, or
require callers to pass an explicit "unrestricted" marker so the decision is visible at
the call site.

#### S6b1-7 (P3) — registered runtime digests are never re-verified at resolve time

`RuntimeRegistry::resolve` filters candidates only by `path.is_dir()` (`dependency.rs:127-133`),
and `resolve_dependencies` compares a manifest's `sha256` pin against the digest **recorded
in the registry**, not against the runtime's current contents. A runtime directory whose
bytes changed after registration still satisfies its pin. Art install closed exactly this
gap by re-deriving `canonical_package_digest` and comparing it before use
(`install.rs:950-1075`); the runtime path should be consistent with it, at least for
lockfile-pinned dependencies where the cost is paid once per install rather than per
launch.

#### S6b1-8 (P3) — the runtime registry re-reads its whole file 2–3× per operation, and its permission fallback produces a misleading error

`storage_path` reads the entire registry file and discards the bytes just to decide which
path to return (`dependency.rs:158-168`), then `list()` reads the same file again. So
`list` costs two full reads, and `register` — `storage_path` in `list`, then again in
`write` — costs three.

The fallback in the same function is the bigger problem: on `PermissionDenied` it silently
switches to `plugin-runtimes-recovered.json`, which for a fresh install does not exist, so
`list()` returns an empty vector with no warning. Every framework dependency then fails
with `no compatible runtime dependency ... satisfies ...`, which points the reader at the
manifest instead of at the file permissions that actually caused it. Cache the resolved
path, and surface the permission failure as a distinct error rather than as an empty
registry.

#### S6b1-9 (P3) — configuring a proxy voids the outbound IP policy, and the policy is process-global

`configure_runtime_proxy` stores a `RuntimeProxy` in a process-wide
`OnceLock<RwLock<..>>` (`network_policy.rs:20-48`), and `secure_client` applies it to every
outbound client. When a custom or system proxy is in effect, the proxy performs the
resolution and the connection, so `validate_ip` on the target host no longer constrains
what is actually contacted — `allow_private_networks: false` becomes unenforceable. The
custom address is assembled as `format!("{}://{}", protocol.trim(), address)` and validated
only by `Url::parse`, with no scheme allow-list of its own (unsupported schemes are caught
later by `Proxy::all`, which makes the failure message worse than it needs to be).

Two smaller consequences of the same global: one proxy setting applies to every outbound
client in the process, including store publish and framework downloads, with no per-art
override; and `runtime_proxy()` swallows a poisoned lock and returns
`RuntimeProxy::System`, so a panic elsewhere silently re-enables the system proxy after
the user disabled it. Prefer an explicit default over a poisoned-lock fallback, and record
in the policy whether a proxy is in effect so callers can refuse private-network-sensitive
work.

#### S6b1-10 (P3) — three small gaps in the range and path checks

- `ipv4_is_private_or_special` (`network_policy.rs:234-243`) omits `100.64.0.0/10`
  (carrier NAT, routable inside many ISP and cloud networks) and `198.18.0.0/15`
  (benchmarking). `Ipv4Addr::is_shared()` and `is_benchmarking()` cover both.
- `is_windows_reserved_name` (`secure_zip.rs:158-174`) covers CON, PRN, AUX, NUL, COM1–9
  and LPT1–9 but not `CONIN$` / `CONOUT$`, which are also device names.
- `MAX_RELATIVE_PATH_BYTES = 240` bounds only the entry's own path, ignoring the length of
  the destination root, and this write path does not go through the extended-length prefix
  helper used elsewhere in the crate. A deep package installed under a long control-plane
  root fails with a raw Windows path-length error instead of a package-level diagnostic.

### S6b2a — Loom art install lifecycle, activation, crash recovery

Files: `crates/loom_tool_registry/src/install.rs:322-950` and `2238-2295`
(`install_art_from_zip_with_source`, `prune_art_versions`, the activation and lifecycle
writers, `recover_art_lifecycle`, `recover_art_uninstall_tombstones`,
`write_art_lockfile`), plus the recovery call sites in `framework.rs:526-539`.

Confirmed correct — do not re-report:

- Every path read back from disk is re-validated: `is_safe_art_version_path` requires
  exactly two components with `versions` first (`install.rs:798-809`), and
  `art_activation_is_safe` / `art_lifecycle_journal_is_safe` apply it to the active
  pointer, the previous pointer and the journal target. A tampered `active.json` or
  `lifecycle.json` cannot traverse out of the art root.
- A journal that fails to parse, or that fails the safety check, is renamed to
  `lifecycle.corrupt` and skipped rather than acted on (`install.rs:855-866`).
- The install failure paths are ordered correctly: `write_art_activation` failure clears
  the journal and removes the target only when this install created it; registry-save
  failure restores the previous activation (or deletes the pointer) before removing the
  target (`install.rs:660-694`).
- `remove_tree` clears the read-only bit before deleting (`install.rs:383-389`), so the
  immutability hardening applied by `set_tree_readonly` cannot block rollback or pruning.
- Framework installation, readiness and the `frameworkVersion` semver requirement are all
  enforced before any file is written (`install.rs:452-501`).
- Pointer paths are stored with forward slashes but compared as `Path` values, and Windows
  treats `/` as a separator in `Path::components`, so the pinned-version comparison in
  `prune_art_versions` matches correctly on both platforms.
- Tombstone directory names are validated before restoration — prefix stripped, nonce must
  be all ASCII digits, original name must pass `is_safe_art_id`
  (`install.rs:374-381`, `909-920`).

#### S6b2a-1 (P2) — crash recovery can delete a version directory the interrupted install did not create

`install_art_from_zip_with_source` computes `target_created` (`install.rs:599-605`): when
the version directory already exists, staging is discarded and the existing directory is
reused, so nothing was created. That distinction is honoured on the synchronous failure
paths (`if target_created { remove_tree(&art_dir) }` at `:662-664` and `:689-691`) but it
is **not recorded in the lifecycle journal**. `ArtLifecycleJournal` carries only
`old_activation`, `next_activation` and `target`.

So when recovery finds a journal whose `next_activation` does not match the current
pointer, it deletes the target unconditionally (`install.rs:875-878`):

```rust
let target = art_root.join(&journal.target);
if target.exists() {
    let _ = remove_tree(&target);
}
```

Reinstalling a version that is already installed — the common "repair" gesture, and the
path taken by the bundled-catalog sweep — writes a journal whose target is a directory
that the *old* activation still points at. A crash between `write_art_lifecycle` and
`write_art_activation` then makes recovery restore `old_activation` and delete the very
directory that pointer names, leaving the art permanently unresolvable with no version
to roll back to.

Fix: add a `target_created: bool` (or `adopted_existing`) field to the journal and skip
the deletion when the target predated the install. The value is already computed at the
call site.

#### S6b2a-2 (P2) — a version directory is keyed on 48 bits of digest, and a colliding directory is adopted without verifying its contents

The version directory name is the package version plus **twelve hex characters** of the
digest (`install.rs:561`):

```rust
let version_dir = format!("{}-{}", sanitize_version_for_path(version), &digest[..12]);
```

Both halves are package-controlled (`version` comes from the package's own security
metadata). When that name already exists and its manifest is readable, the installer
discards its own freshly verified staging tree and adopts the existing directory
(`install.rs:599-605`) — the existing bytes are never compared against `digest`. It then
writes a lockfile named after the *new* package's full digest, hashes the *existing*
directory's binaries into it (`write_art_lockfile` reads from `art_dir`), and points
`active.json` at that directory with the new package's digest and version.

48 bits is roughly 2^24 work to collide deliberately. The saving grace is that every
production consumer goes through `resolve_installed_art_package`, which re-derives
`canonical_package_digest` and rejects a mismatch (`install.rs:950-1075`), so this cannot
substitute executed code — it fails closed. The realisable impact is a denial of service:
any package that can be installed under a given publisher and id can pre-create a
directory whose 12-hex prefix collides with a genuine version, and the genuine install
then silently becomes unresolvable ("no installed version matches the requested digest")
while reporting success.

Fix: compare the full digest, and when the target exists either verify its contents match
`digest` or install under a fresh recovered name — the permission-denied branch at
`:587-596` already implements exactly that fallback.

#### S6b2a-3 (P2) — pruning deletes version directories that pinned Surfaces are still executing

`resolve_installed_art_package` documents its purpose as pinning one immutable version so
"an unrelated store update cannot silently move their code or break execution"
(`install.rs:946-949`). But the pin only stabilises the *pointer*; nothing stabilises the
*files*. `prune_art_versions` retains the active version, the previous version, and
`art_history_limit().saturating_sub(2)` further directories — one by default
(`install.rs:722-758`) — and deletes everything else by modification time, with no notion
of which versions are in use.

Two installs after a Surface pinned version *V* are therefore enough to delete *V* out
from under a running instance. The failure surfaces later as file-not-found errors from
inside the framework process, far from the install that caused it.

Fix: record in-use versions (the Surface instance store already knows the resolved
`art_dir`) and exclude them from pruning, or take a lease/refcount per resolved package
and defer deletion until it drops. At minimum, never prune a directory that any live
Surface instance resolved.

#### S6b2a-4 (P2) — the recovery sweep runs from any process that opens a framework registry, including the CLI

`FrameworkRegistry::new` performs four filesystem recovery sweeps and a registry prune as
a side effect of construction, discarding every error (`framework.rs:526-539`):

```rust
let _ = registry.recover_uninstall_tombstones();
let _ = registry.recover_lifecycle_journals();
let _ = crate::install::recover_art_uninstall_tombstones(&registry.root);
let _ = crate::install::recover_art_lifecycle(&registry.root);
let _ = crate::dependency::RuntimeRegistry::new(&registry.root).prune_stale();
```

Recovery is destructive — it rewrites `active.json` and calls `remove_tree` on journal
targets — and a journal is present on disk for the entire duration of a normal install.
`apps/plugin-cli/src/lib.rs:1269` constructs a `FrameworkRegistry` against the same
control-plane root, so running the CLI while the daemon is installing an art will observe
the in-flight journal, decide the install did not complete, roll the pointer back and
delete the target directory the daemon is still writing. No cross-process lock guards
this, so the daemon's in-process `serialized_route_lock` does not help.

Fix: gate recovery on an exclusive lock file over the control-plane root, and require it to
be an explicit call rather than a constructor side effect so that read-only consumers
(the CLI's list/status commands) never mutate installed state. Note the daemon also
constructs the registry twice at startup (`lib.rs:552` and `:571`), so the sweep runs
twice per launch.

#### S6b2a-5 (P3) — the crash-consistency journal is itself not crash-consistent

`write_art_activation` and `write_art_lifecycle` both use a fixed temporary name derived
from the target (`path.with_extension("json.tmp")`), write it with a bare `fs::write`, and
hand it to `replace_registry_file` with no `sync_all` on either the file or the parent
directory (`install.rs:766-792`). The daemon's own sensitive-file writer does all three
(`apps/daemon/src/lib.rs:1205-1283`).

Consequences: two installs touching the same art root collide on the temporary name, and
after a power loss the rename may be visible while the journal's contents are not — the
exact scenario the journal exists to survive. Same class as S6a-5; the fix is the same
(nonce-named temporary, `sync_all`, parent sync).

#### S6b2a-6 (P3) — a corrupt activation pointer silently discards rollback history

`read_art_activation` maps every failure — missing file, I/O error, malformed JSON — to
`None` (`install.rs:762-764`). A truncated `active.json` therefore reads as "this art has
no activation", and the next install writes a fresh state with `previous: None`, so
`rollback_art_package` has nothing to roll back to. Distinguishing "absent" from
"unparseable", and preserving the file as `.corrupt` the way the journal path already does,
would keep the failure visible.

#### S6b2a-7 (P3) — every art install re-hashes the entire framework runtime, and the lockfile write is not atomic

`write_art_lockfile` calls `canonical_package_digest(&framework_dir, None)`
(`install.rs:2264-2266`) — a full recursive hash of the framework's runtime directory,
which for the Python embed is on the order of a hundred megabytes and thousands of files.
It runs on every art install, and `install_art_recursive` repeats it for every dependency
in the graph. The framework's digest is already known at framework-install time and could
be read from the framework registry or its lockfile instead.

The same function finishes with `std::fs::write(path, bytes)` (`:2293`), the only
non-atomic write in an otherwise temp-then-replace crate. An interrupted write leaves a
truncated lockfile, and the version directory it describes has already been made read-only,
so `verify_art_lockfile` fails and the art is unusable.

#### S6b2a-8 (P3) — the recovery sweeps abort on the first I/O error and report nothing

`recover_art_lifecycle` and `recover_art_uninstall_tombstones` propagate errors with `?`
from inside their per-art loops (`install.rs:856`, `:871`, `:923`, `:927`, `:929`), so one
unreadable art root — exactly what a partially applied ACL migration produces — stops
recovery for every art after it in directory order. Both are then called as
`let _ = ...` (`framework.rs:535-536`), so the error is discarded and nothing is logged.
Recovery should be per-art fault-isolated (collect failures, continue) and should surface
what it skipped.

#### S6b2a-9 (P3) — `resolve_active_art_package` is public and performs no digest verification

`resolve_active_art_package` (`install.rs:936-946`) validates the pointer path and checks
that a manifest exists, but never re-derives the package digest — unlike
`resolve_installed_art_package`, which does. It currently has no callers outside the
crate, which is the only reason S6b2a-2 is confined to denial of service. Either fold the
digest check into it or make it private, so a future caller cannot reintroduce an
unverified execution path.

### S6b2b1 — Loom art integrity verification, activation, lockfile verification, MCP dependency locks

Scope: `install.rs:1119-2008` (`verify_art_package_integrity`, `list_installed_art_versions`,
`activate_art_version`, `rollback_art_package`, `activate_art_pointer`, `verify_art_lockfile`,
`locate_exact_installed_art_package`, MCP dependency validation and lock resolution), plus the
call sites in `apps/daemon/src/surface_actions.rs` and `loom_plugin_security`'s
`canonical_package_digest`.

#### S6b2b1-1 (P1) — every Surface interaction re-hashes the whole Art and its framework runtime, while holding the Surface store lock

`SurfaceActionExecutor` resolves the tool through `tool_resolver` on each action dispatch
(`surface_actions.rs:343`) and on each event acknowledgement (`surface_actions.rs:275`).
That resolver is `resolve_installed_art_package` (`surface_actions.rs:128-137`), which:

1. re-derives `canonical_package_digest` over every file of the Art package,
2. re-verifies the package signature and trust policy,
3. calls `verify_art_lockfile`, which re-derives `canonical_package_digest(&runtime_dir, None)`
   over the **entire** framework runtime directory (`install.rs:1573`) — for the Python
   framework this is the whole embedded interpreter, hundreds of megabytes,
4. reads and hashes every locked binary in full (`install.rs:1597`),
5. recursively repeats all of the above for every Art dependency (`install.rs:1632`).

`canonical_package_digest` (`loom_plugin_security/src/lib.rs:696-712`) walks the tree and
reads every file with `fs::read`; there is no digest cache anywhere in the crate, so none of
this work is amortised.

Both call sites do this while the Surface store mutex is still held: `store` is a
`MutexGuard` acquired earlier in the same scope, `instance` borrows from it, and
`(self.tool_resolver)(&instance.descriptor)` runs before the guard is dropped
(`surface_actions.rs:255-275` and the corresponding block ending at `:343`). So the whole
Surface subsystem — every instance, not just the one being acted on — serialises behind a
multi-second full-runtime hash on every user click.

Fix: verify once at activation time and keep a verified handle, or cache verification keyed
by a cheap fingerprint (directory mtime plus size set) with a full re-hash only on change.
Independently, resolve the tool before taking the store lock, or clone the descriptor and
drop the guard first — holding a global mutex across unbounded file I/O is wrong even if the
hashing becomes cheap.

#### S6b2b1-2 (P2) — activation journals a target it did not create, so an interrupted rollback deletes the version being rolled back to

`activate_art_pointer` writes the lifecycle journal with `target: next.active.path`
(`install.rs:1500-1507`) and then calls `write_art_activation(active_path, &next)?`
(`install.rs:1508`) with no rollback on failure — unlike the install path, which restores the
previous activation and clears the journal when that same write fails
(`install.rs:660-666`).

If the process crashes, or that single write merely fails, the journal survives with
`next_activation` not matching `active.json`. Recovery then takes the branch at
`install.rs:869-879`: it restores the old activation (correct) and executes
`remove_tree(art_root.join(&journal.target))`. But in the activation path the target
directory is a pre-existing, already-verified installed version that this operation never
created. The result is that an interrupted `rollback_art_package` deletes exactly the
known-good version the user was rolling back to.

This shares the missing-`target_created` root cause with S6b2a-1, but is strictly worse:
during activation the delete is *never* correct, whereas during install it is correct only
when the directory was freshly created. Both are fixed by recording in the journal whether
the operation created the target, and by having activation roll back its own journal on a
failed activation write.

#### S6b2b1-3 (P2) — locked Art dependencies whose version carries SemVer build metadata can never resolve

Version directories are named with `sanitize_version_for_path`
(`install.rs:561`, definition at `install.rs:2295-2311`), which replaces every character
outside `[A-Za-z0-9._-]` with `-`. SemVer build metadata uses `+`, so `1.2.0+build.7` is
installed into `versions/1.2.0-build.7-<12 hex>`.

`locate_exact_installed_art_package` then matches the directory against the **raw** version:

```rust
if actual.eq_ignore_ascii_case(digest)
    && path.ends_with(format!("{version}-{}", &actual[..12]))
```

(`install.rs:1711-1713`). `1.2.0+build.7-<12 hex>` never equals the installed
`1.2.0-build.7-<12 hex>`, so the dependency is reported "unavailable" even though it is
installed and its digest matches. The failure is fail-closed but total: such a package can be
installed and activated, yet can never be used as a dependency by another Art.

The other three places that check the directory-name/digest binding compare only the digest
suffix (`install.rs:1189`, `:1261`, `:1342`) and are unaffected. Fix: apply
`sanitize_version_for_path` to the expected name, or drop the name check and rely on the
manifest version plus the recomputed digest, which are already compared.

#### S6b2b1-4 (P2) — MCP server packages are exempt from signature and trust-policy enforcement, and an Art can execute one

`crates/loom_mcp/src/` contains no reference to `TrustStore`, `verify_package_signature`, or
`effective_policy`. `verify_active_mcp_package` (`install.rs:1827-1922`) checks identity,
publisher/digest well-formedness, SemVer, state-file agreement, and directory shape — but
never a publisher signature or the trust policy.

Art packages do get that enforcement, including when reached as a dependency
(`locate_exact_installed_art_package`, `install.rs:1714-1728`). Since an Art may declare the
`mcp` framework plus a matching `metadata.mcp` block
(`validate_mcp_execution_dependency`, `install.rs:1746-1795`), installing an Art under a
policy that requires signed publishers can still end up launching an unsigned third-party
MCP server process. The trust boundary is therefore weaker than the policy states.
Confirm the launch-time behaviour in S7 and extend the same publisher/trust enforcement to
MCP package install and activation.

#### S6b2b1-5 (P3) — an installed MCP package's recorded digest is never checked against its contents

`verify_active_mcp_package` compares `servers.json`'s `package.digest` with `active.json`'s
`digest` (`install.rs:1869-1877`) — two pieces of state that are written together — and uses
`package.digest[..12]` to derive the expected directory name (`install.rs:1897`). It never
re-derives the digest from the files. The caller recomputes it
(`install.rs:1998`) but only to record it in a new Art lockfile, and only
`verify_art_lockfile`'s `mcp` arm compares a recomputed digest against a previously locked one
(`install.rs:1651`).

So editing the files of an installed MCP package leaves both state files self-consistent, and
detection depends entirely on some Art having already locked the old digest. Newly installed
Arts lock whatever is on disk (trust on first use). `verify_active_mcp_package` should
re-derive the content digest and compare it against the recorded state. Escalate to P2 if S7
shows the direct MCP launch path performs no verification either.

#### S6b2b1-6 (P3) — listing and activating versions re-hash the entire version history

`list_installed_art_versions` calls `canonical_package_digest` for every directory under
`versions/` on every call (`install.rs:1248-1255`), and `activate_art_version` does the same
across all directories in order to find the one matching version
(`install.rs:1329-1336`). With the default history of three versions, opening a version list
in the UI re-hashes three complete Art packages. The digest is already recorded in
`active.json` and in the lockfile names under `locks/`; use those, and re-hash only when the
caller asks for verification.

#### S6b2b1-7 (P3) — `activate_art_version` no-ops on the version that most needs repair

`activate_art_version` returns `Ok(current)` as soon as `activation.active.version` equals the
requested version (`install.rs:1306-1308`), before any safety, existence, or digest check.
Re-activating the current version is the natural operator response to a corrupted or
half-installed active pointer, and it is precisely the case that does nothing. It also returns
the registry copy, so the caller cannot distinguish "verified and activated" from "ignored".
Either verify before the early return, or make the early return conditional on the active
directory passing `verify_art_package_integrity`.

#### S6b2b1-8 (P3) — rollback overwrites `previous` with the version rolled back from, so the good version can be pruned

`activate_art_pointer` always sets `previous: Some(activation.active.clone())`
(`install.rs:1494-1499`). After rolling back from a broken v2 to v1, `previous` becomes v2, so
a second rollback returns to the broken v2 and v1 is no longer pinned. `prune_art_versions`
pins only `active` and `previous` (S6b2a-3), so the known-good version becomes eligible for
deletion after two more installs. Rollback should either keep the pre-rollback `previous`
chain or mark the version it rolled back from as quarantined rather than as the rollback
target.

#### S6b2b1-9 (P3) — dependency verification is exponential on diamond dependency graphs

`verify_art_lockfile` inserts the child id into `verifying` before recursing and removes it
afterwards (`install.rs:1623`, `:1641`). The set therefore detects cycles but does not memoise
success, so a dependency shared by several parents is fully re-verified — including a full
re-hash of its framework runtime — once per distinct path through the graph. Combined with
S6b2b1-1 this multiplies the per-interaction cost. Keep a separate
`BTreeSet<(id, digest)>` of already-verified packages for the duration of one verification.

#### S6b2b1-10 (P3) — verification reads whole files into memory, and reloads the trust store per candidate

`canonical_package_digest` hashes with `fs::read(path)` per file
(`loom_plugin_security/src/lib.rs:704`), and `verify_art_lockfile`'s binary arm does
`sha256_hex(&std::fs::read(art_dir.join(relative))?)` (`install.rs:1597`). A locked
several-hundred-megabyte binary is therefore a single allocation of that size on every
verification. Stream through a fixed buffer instead.

Separately, `locate_exact_installed_art_package` calls `TrustStore::load` from inside the
per-candidate loop (`install.rs:1714`), re-reading `plugin-trust.json` once per matching
version directory. Hoist it out of the loop.

#### Confirmed correct — do not re-report

- Cycle detection in `verify_art_package_integrity_inner` correctly inserts before the closure
  and removes after it, so the identity is released on both the success and error paths
  (`install.rs:1136`, `:1205`).
- The lockfile containment check canonicalises both the Art root and the lockfile path before
  `starts_with`, so symlinked lockfiles cannot escape (`install.rs:1527-1533`).
- Locked binary paths reject absolute paths and `ParentDir`/`RootDir`/`Prefix` components
  (`install.rs:1584-1596`).
- Unsupported lockfile dependency kinds are rejected rather than skipped
  (`install.rs:1658-1662`) — fail closed.
- `validate_mcp_execution_dependency` requires `metadata.mcp` to correspond to exactly one
  identical `metadata.dependencies.mcpServers` entry (`install.rs:1785-1794`), closing the
  "execute a server that was never declared as a dependency" hole.
- `resolve_mcp_dependency_locks` rejects duplicate declarations, disabled servers, multiple
  installed servers with the same qualified id, and versions outside the requirement
  (`install.rs:1942-1996`).
- `verify_active_mcp_package`'s directory-shape check — canonicalised parent equals
  `versions/`, file name equals `<version>-<12 hex>`, and the registry directory equals the
  active directory (`install.rs:1897-1906`) — is a correct anti-escape check, and the MCP
  install path uses the same unsanitised naming (`loom_mcp/src/package.rs:137`), so the
  mismatch described in S6b2b1-3 does not apply here.
- `activate_art_pointer` restores the previous activation and clears the journal when
  `save_tool` fails (`install.rs:1509-1513`).

### S6b2b2 — Loom art uninstall, dependency lock sets, binary resolution, packaging and signing

Scope: `install.rs:2010-2236` (lock-set validation, uninstall, dependency lock resolution) and
`install.rs:2332-2771` (binary verification and download, recursive install, packaging,
signing), cross-checked against `loom_plugin_security::verify_package_signature`.

#### S6b2b2-1 (P2) — a signed Art that declares a downloaded binary is permanently unverifiable after install

`install_art_from_zip_with_source` verifies the package signature over the extracted staging
tree (`install.rs:531-547`) and only then calls `resolve_binaries`
(`install.rs:551`), which downloads every non-bundled binary **into that same tree**
(`dest = art_dir.join(&rel)`, `install.rs:2424`, written at `:2390`). The activation digest is
computed afterwards (`install.rs:552-559`), so it covers the downloaded bytes.

`verify_package_signature` (`loom_plugin_security/src/lib.rs:662-668`) recomputes
`canonical_package_digest(package_dir, Some(&signature.file))` over the **whole** directory and
compares it to the digest inside `signature.json`. Every later verification path calls it —
`resolve_installed_art_package`, `verify_art_package_integrity_inner` (`install.rs:1168`),
`activate_art_pointer` (`install.rs:1439`), `locate_exact_installed_art_package`
(`install.rs:1716`). By then the directory contains files the publisher never signed, so the
digest cannot match and every call fails with `DigestMismatch`.

Net effect: install succeeds, and then the Art can never be launched, activated, rolled back,
or used as a dependency. Only the signed + remote-binary combination is affected; unsigned
packages return `PackageTrustStatus::Unsigned` before the digest check.

No test covers it: the only binary tests use bundled binaries
(`install.rs:3341`, `:3359`, `:3375`), and nothing in `crates` or `apps` references
`download_binary` or `RemoteBinaryHashRequired` outside this file.

Fix: either download outside the signed tree (a sibling `binaries/` directory referenced by
the lockfile) or exclude declared binary paths from the canonical digest and rely on the
per-binary sha256 that is already mandatory for remote binaries.

#### S6b2b2-2 (P2) — Art install can drive plaintext HTTP requests to arbitrary loopback ports

`download_binary` builds its policy with `allow_http_loopback: true`
(`install.rs:2368-2371`), so a manifest URL of `http://127.0.0.1:<port>/<path>` passes
`validate_outbound_url`. Installing an untrusted package therefore lets it issue arbitrary GET
requests, in clear text, against every service bound to loopback on the host — including other
Loom components, local admin interfaces, and anything else listening. The mandatory sha256
protects the *content* that lands on disk, but the request itself is the side effect that
matters here.

This compounds S6b1-2: `::ffff:127.0.0.1` already slips through `ipv6_is_private_or_special`,
so the loopback allowance is not even the only route. Loopback fetches should be gated behind
an explicit development flag, not enabled unconditionally for package installs.

#### S6b2b2-3 (P3) — uninstall performs no reverse-dependency check

`uninstall_art_package` (`install.rs:2155-2184`) removes an Art without asking whether any
installed Art locks it. Every dependent then fails `verify_art_lockfile`'s `art` arm through
`locate_exact_installed_art_package` (`install.rs:1617-1622`), which reports the dependency as
"unavailable". The dependent Art becomes unusable with an error that names the *missing*
package, not the operation that removed it. Enumerate dependents and refuse, or require an
explicit cascade flag.

#### S6b2b2-4 (P3) — recursive install rolls back best-effort and reports nothing about a partial state

On failure `install_art_recursive` uninstalls the newly installed packages in reverse order
with `let _ = uninstall_art_package(...)` (`install.rs:2560-2564`). Every uninstall error is
discarded, so a rollback that itself fails leaves installed dependencies behind and the caller
receives only the original install error. Collect the rollback failures and attach them to the
returned error so the operator knows which packages were left on disk.

#### S6b2b2-5 (P3) — unqualified Art dependency references bind to whichever publisher happens to be installed

`art_reference_matches_qualified` (`install.rs:2010-2019`) lets a bare reference match any
publisher's Art with that id, and `ToolRegistry::get_tool` falls back to a bare-id search
(`lib.rs:594-599`). The first resolution therefore binds to whatever single publisher is
installed at that moment; the lockfile pins the digest afterwards, and a second publisher with
the same id turns the reference into `AmbiguousToolId`. So the model is trust-on-first-use with
a squatting window rather than a stated publisher. Require publisher-qualified references in
manifests, or record the resolved publisher at authoring time.

#### S6b2b2-6 (P3) — Art dependencies cannot express a version requirement

`read_dependencies(tool).arts` is a list of plain strings — `validate_art_dependency_lock_set`
(`install.rs:2021-2052`) checks only the declared/locked correspondence, with no version
requirement, while the MCP equivalent parses and enforces a `VersionReq`
(`install.rs:2070-2097`). An Art dependency is consequently pinned solely by the digest in the
lockfile: any upgrade or activation change of the dependency makes every dependent fail
verification, with no way to declare "compatible with 1.x". Give Art dependencies the same
`{ id, version }` shape the MCP dependencies already use.

#### S6b2b2-7 (P3) — the unsigned packaging path follows symbolic links

`copy_art_resources_for_signing` explicitly rejects symlinks (`install.rs:2663-2668`), but
`package_art_to_zip` walks the directory through `add_dir_to_zip`, which tests
`path.is_dir()` (`install.rs:2753`) — a call that follows links. A directory symlink cycle
recurses until the stack or the disk gives out, and a file symlink has its target's contents
embedded in the published package. Installed Art directories should not contain symlinks
(`extract_zip_securely` rejects them), so this matters for the authoring and local-directory
callers. Apply the same `file_type().is_symlink()` rejection as the signing path.

#### S6b2b2-8 (P3) — packaging and downloading buffer everything in memory

`add_dir_to_zip` reads each file whole (`install.rs:2766`) into a zip that is itself an
in-memory `Vec<u8>` returned by value (`install.rs:2576-2592`), so packaging an Art costs
roughly its full size in RAM twice. `download_binary` likewise holds up to 128 MiB in memory
before writing (`install.rs:2381-2390`). Stream to a temporary file instead.

#### S6b2b2-9 (P3) — packaged zip entries carry no permission bits

Both packaging paths use `zip::write::FileOptions::default()` (`install.rs:2579`, `:2706`) and
never set `unix_permissions`, so the executable bit is lost through the package → install
round trip. On Windows this is invisible; on Linux and macOS a bundled binary arrives
non-executable and the Art fails at launch with a permission error rather than a packaging
error.

#### S6b2b2-10 (P3) — authoring can build a package that installs but fails every later verification

`build_authored_art_package_zip` (`install.rs:2693-2741`) rejects absolute paths, traversal,
duplicates, and the reserved `manifest.json` / `art.runtime.json` names, but permits
`signature.json` and applies none of the installer's other limits (Windows reserved names,
`MAX_RELATIVE_PATH_BYTES`, entry counts). Because authored installs skip trust enforcement
(`install.rs:531-547` enforces only for `ArtInstallSource::ExternalPackage`), an authored
package carrying a bogus `signature.json` plus `packageSecurity.signature` metadata installs
cleanly and then fails `verify_package_signature` on every subsequent resolve — the same
bricked-after-install shape as S6b2b2-1. Validate authored file paths with the installer's own
rules and reject `signature.json` unless the authoring flow is actually signing.

#### S6b2b2-11 (P3) — the signing staging directory is created in the shared temp directory without ACL hardening

`package_signed_art_to_zip` stages into
`std::env::temp_dir().join(format!("loom-art-sign-{}-{nonce}", std::process::id()))`
(`install.rs:2601-2608`) and calls `create_dir_all`, which succeeds on an existing directory.
Nothing calls `loom_plugin_security::restrict_private_path_permissions` here, unlike the rest
of the private-state code. On a multi-user host the full package content is staged
world-readable, and a pre-created junction at that path would relocate both the staged copy
and the bytes that get signed. The nanosecond nonce makes the race hard, not impossible;
harden the directory and refuse to reuse an existing path.

#### Confirmed correct — do not re-report

- Uninstall ordering is crash-correct: rename to tombstone, then `delete_tool`, then
  `remove_tree` (`install.rs:2167-2182`). A crash before `delete_tool` leaves the registry
  entry present with the live directory missing, which is exactly
  `recover_art_uninstall_tombstones`' restore predicate; a crash after it leaves
  `installed == false`, which is its delete predicate.
- `uninstall_art_package` renames the tombstone back if `delete_tool` fails
  (`install.rs:2174-2179`).
- `resolve_art_root_for_uninstall` refuses a bare id that exists under several publishers
  (`install.rs:2144-2152`) and skips publisher directories failing
  `is_safe_publisher_id`.
- `resolve_binaries` rejects empty names, `:`, absolute paths, and
  `ParentDir`/`RootDir`/`Prefix` components (`install.rs:2406-2420`).
- Remote binaries must declare a sha256 (`install.rs:2353-2361`), and it is verified before the
  bytes are written to disk (`install.rs:2386-2390`).
- `install_art_recursive` detects cycles both by identity and by reference match
  (`install.rs:2488-2492`, `:2503-2510`), and verifies that a package fetched from the store
  actually matches the requested reference (`install.rs:2481-2487`) — the store cannot
  substitute a different Art for a dependency.
- Both lock-set validators require a strict one-to-one correspondence between declared and
  locked dependencies, rejecting extras and duplicates in either direction
  (`install.rs:2040-2050`, `:2080-2103`).
- `package_signed_art_to_zip` writes the manifest into staging before signing and
  `package_art_to_zip` re-serialises the same `signed_tool` while skipping the staged copy
  (`install.rs:2643-2649`, `:2760-2762`), so the signed digest and the packaged bytes agree.

### S6b2c1 — Loom framework readiness, package resolution, registry state, trust store, recovery

Scope: `crates/loom_tool_registry/src/framework.rs:121-955` (permission-mode parsing and the
enforcement matrix, `read_dependencies`, `framework_ready_in`, package-directory resolution,
`FrameworkRegistry` state/activation/lifecycle helpers, both recovery sweeps, the trust-store
API, `installed_ids`/`is_installed`/`is_enabled`/`readiness`/`statuses`, and the install and
enable entry points), plus `status_of` (`:1253-1345`) and `verify_framework_lockfile`
(`:1616-1712`) where they determine the cost and behaviour of the above, and the call sites in
`apps/daemon/src/lib.rs` and `crates/loom_tool_registry/src/install.rs`.

**S6b2c1-1 (P1) — a framework status query hashes every installed framework package three
times, and it sits on the Surface interaction hot path.**

`status_of` computes the same content digest three separate times per installed framework:

- `readiness` (`:834-848`) calls `framework_ready_in`, which calls `verify_package_signature`
  (`:380-388`) — that re-hashes the whole package directory
  (`loom_plugin_security/src/lib.rs:662-666`, `:696-712`) — and then
  `verify_framework_lockfile` (`:393`), which hashes the same directory again
  (`:1630-1637`) to derive the lockfile file name.
- `status_of` then calls `verify_package_signature` a third time purely to fill
  `trust_status` (`:1281-1291`).

`statuses()` (`:852-866`) runs `status_of` for every installed framework plus the four catalog
ids, so the cost scales with the number of installed frameworks. For the `process` framework
the package directory contains the bundled Python runtime, so each of those digests walks tens
of thousands of files.

`statuses()` is not a rare administrative call. `verify_art_lockfile` invokes it for every
`framework` entry in an Art lockfile (`install.rs:1552-1556`) and then hashes the resolved
runtime directory a fourth time (`install.rs:1573`). Per S6b2b1-1 that verification runs on
every Surface event acknowledgement and action dispatch, while the Surface store mutex is
held (`apps/daemon/src/surface_actions.rs:275`, `:343`). The daemon also calls `statuses()`
from four request handlers (`apps/daemon/src/lib.rs:10079`, `:10095`, `:10260`, `:11372`) and
`readiness` from three more (`:10156`, `:10983`, `:12047`).

Fix direction: compute the package digest once per `status_of` and thread it through
signature verification, lockfile verification, and the reported trust status; then cache the
result keyed by (package directory, directory mtime/size fingerprint) so repeated status and
readiness queries within a session do not re-walk the runtime. `verify_art_lockfile` should
resolve a single framework by id instead of materialising every status, and should reuse the
digest that resolution already produced.

**S6b2c1-2 (P2) — framework packages ignore the persisted trust policy that Art packages
honour, so a user who requires signed or trusted packages still gets unsigned frameworks.**

`TrustStore` exposes `effective_policy()` (`loom_plugin_security/src/lib.rs:186-188`), which
prefers the `LOOM_PLUGIN_TRUST_POLICY` environment override and otherwise uses the policy
persisted in the store. Every Art path uses it (`install.rs:544`, `:1052`, `:1177`, `:1448`,
`:1725`), and the daemon gates its UI on it (`apps/daemon/src/lib.rs:9707`). All three
framework paths instead use `TrustPolicy::from_env()` (`framework.rs:389` in
`framework_ready_in`, `:996` in the package install, `:1176` in rollback), which falls back to
`TrustPolicy::default()` — `AllowUnsigned` — and never reads the store.

`FrameworkRegistry::set_trust_policy` (`:767-772`) persists a policy into that same store, so
the operator-visible setting is silently inert for exactly the component that executes Art
code with the highest privilege: an operator who sets `require-trusted` sees Arts rejected,
the UI report claiming the strict policy, and unsigned third-party framework packages
installing and running anyway. Fix: replace all three call sites with
`trust_store.effective_policy()`.

**S6b2c1-3 (P2) — one damaged framework package makes healthy frameworks unresolvable,
because `resolve_framework_package_dir` propagates per-candidate failures out of the loop.**

```rust
for publisher in fs::read_dir(runtime_root).ok()? {
    let publisher = publisher.ok()?;
    ...
    if let Some(package) = resolve_framework_package_root(&candidate) {
        let manifest = read_framework_manifest(&package.join(FRAMEWORK_MANIFEST_FILE)).ok()?;
```

(`:413-420`.) Both `?` operators return `None` from the whole function, not from the current
iteration. A single unreadable directory entry, or a single sibling package whose
`framework.manifest.json` is truncated or invalid JSON, therefore aborts resolution for every
publisher — including the one whose package is intact. The framework then reports
`未找到活动框架包` (`:334`), `package_manifest` returns `None`, `is_installed` becomes false,
and every Art bound to that framework fails to run until the unrelated broken directory is
removed by hand. Both should be `continue`.

The same block also collapses ambiguity into the not-found path: `(matches.len() == 1)`
(`:426`) discards the multi-match case without distinguishing it, so two publishers shipping
the same local framework id surface as "not installed" rather than as the
`AmbiguousFramework` error `resolve_state_key` already models (`:584`).

**S6b2c1-4 (P2) — a corrupt `frameworks.json` silently reports zero installed frameworks, and
the next write makes that loss permanent.**

`installation_states` (`:1341-1347`) swallows both the read error and the parse error, the
latter through `unwrap_or_default()`, returning an empty map. Nothing distinguishes "no
frameworks installed yet" from "the state file is corrupt". Consequences compound:

- `installed_ids`, `is_installed`, `is_enabled`, `readiness`, and `statuses` all report every
  framework as uninstalled while the packages sit intact on disk.
- The next successful install writes the state map back (`write_installed`, `:1349-1362`),
  permanently dropping every other framework's entry.
- `recover_uninstall_tombstones` decides restore-versus-delete from that same map
  (`:729-733`): with an empty map every pending uninstall tombstone is deleted rather than
  restored, so a crash-interrupted uninstall silently completes for frameworks the user never
  asked to remove.

A parse failure should be surfaced as an error (or the file quarantined the way
`recover_lifecycle_journals` quarantines a bad journal at `:657-660`) rather than treated as
an empty registry.

**S6b2c1-5 (P3) — signature verification failures are reported to the UI as
`Unsigned`.**

`status_of` builds `trust_status` with `.ok()` followed by `unwrap_or_default()`
(`:1281-1292`), and `PackageTrustStatus::default()` is `Unsigned`
(`loom_protocol/src/lib.rs:91-98`). A tampered package, a digest mismatch, or an unreadable
trust store therefore all display as an ordinary unsigned package, even though the enum has an
`Invalid` variant for exactly this case. Execution is still blocked by `framework_ready_in`
(`:387`), so this is display-only, but it hides the difference between "the publisher never
signed this" and "this package no longer matches its signature".

**S6b2c1-6 (P3) — the recovery sweeps abort on the first failing entry, and the caller
discards the error.**

`recover_lifecycle_journals` propagates `fs::read_dir(&root)?` and `first?` at the top level
(`:640-641`) while deliberately swallowing the inner level with
`.into_iter().flatten().flatten()` (`:645`), and it propagates every `fs::read`,
`serde_json::to_vec_pretty`, and `replace_registry_file` failure inside the per-journal loop
(`:655-677`). `recover_uninstall_tombstones` likewise propagates `entry?` and both
`fs::rename`/`remove_framework_tree` calls (`:711`, `:730-732`). Because both run as
constructor side effects whose errors are discarded (`:533-534`, already recorded in S6b2a),
one directory held open by an antivirus scanner or a stale handle silently skips recovery for
every remaining framework. Per-entry failures should be logged and skipped so the sweep
finishes.

**S6b2c1-7 (P3) — `readiness` re-reads the state file and re-resolves the package directory
about six times per query.**

`readiness` (`:834-848`) calls `resolve_state_key` (one `installation_states` read), then
`package_manifest` (a `runtime_dir` resolution plus a manifest read), then `is_installed`
(another `resolve_state_key` plus another `package_manifest`), then `is_enabled` (a third
`resolve_state_key`, a third `package_manifest`, and another `installation_states`), before
finally calling `framework_ready_in` (a fourth manifest read). `resolve_state_key`,
`is_installed`, and `is_enabled` each re-read `frameworks.json` from disk because
`installation_states` has no caching, and `runtime_dir` re-runs the publisher-directory scan
each time. This is cheap next to S6b2c1-1 but it is pure duplicated I/O on the same hot path.

**S6b2c1-8 (P3) — `upgrade_framework_package_from_zip` is an unguarded duplicate of the
install entry point with no callers.**

```rust
pub fn install_framework_package_from_zip(&self, zip_bytes: &[u8]) -> ... {
    self.install_framework_package_zip(zip_bytes, None)
}
pub fn upgrade_framework_package_from_zip(&self, zip_bytes: &[u8]) -> ... {
    self.install_framework_package_zip(zip_bytes, None)
}
```

(`:894-909`.) The two functions are byte-for-byte identical: the "upgrade" variant performs no
check that the ZIP's manifest belongs to an already-installed framework, so calling it with an
unrelated package installs that package instead of upgrading anything. The guarded
`upgrade_framework_package(id, zip)` (`:913-925`) is the one the daemon actually routes to
(`apps/daemon/src/lib.rs:10868`), and `upgrade_framework_package_from_zip` has no callers in
the workspace. It should be removed rather than left as a name that promises a check it does
not perform.

Confirmed-correct behaviour worth recording for later slices:

- `framework_ready_in` verifies protocol negotiation, platform support, entry kind, entry path
  containment, entry existence, the permission policy, the signature, the trust policy, and
  the lockfile before reporting ready (`:347-395`); readiness is a genuine gate, not a
  cosmetic probe.
- `resolve_framework_package_root` refuses a flat package directory with no `active.json` and
  validates the activation pointer with `is_safe_framework_version_path` before joining it
  (`:429-445`), so a crafted pointer cannot escape the package root.
- `framework_storage_path` requires a `is_safe_publisher_id` publisher and a valid framework
  id, and rejects any further `/` (`:447-459`), so a qualified reference cannot address a
  deeper path.
- Unlike Art dependencies (S6b2b2-4), `verify_framework_lockfile` does enforce a SemVer
  requirement on every locked dependency, rejects duplicates, undeclared entries, and missing
  non-optional entries, and requires the locked digest to match a registered runtime
  (`:1656-1710`).

### S6b2c2 — Loom framework package install, rollback, uninstall, retention, dependency registration

Scope: `crates/loom_tool_registry/src/framework.rs:956-1252` (`install_framework_package_zip`,
`staging_dir`, `rollback`, `uninstall`) and `:1349-1614` (`write_installed`, tombstone paths,
readonly/remove helpers, the activation and journal safety predicates,
`prune_framework_versions`, `sanitize_version_for_path`, `resolve_framework_dependencies`,
`register_framework_runtimes`, `write_framework_lockfile`).

**S6b2c2-1 (P2) — the lifecycle journal's `target` means "a directory this operation created",
but install-onto-an-existing-version and rollback both journal a directory that already
existed, so crash recovery deletes a good installed version.**

Recovery treats `journal.target` as scratch to be destroyed whenever the activation on disk
does not match `journal.next_activation`:

```rust
let target = package_root.join(&journal.target);
if target.exists() {
    let _ = remove_framework_tree(&target);
}
```

(`:681-684`.) Two writers violate that contract:

- `install_framework_package_zip` distinguishes a freshly renamed directory from a reused one
  via `target_created` (`:1038-1044`) and correctly guards its own cleanup with it
  (`:1054-1056`, `:1084-1086`, `:1105-1107`) — but the journal it writes always records
  `target: active_relative` (`:1075-1082`), including the `target_exists` case. Reinstalling a
  version whose directory is still on disk (for example the version currently recorded as
  `previous` after a rollback) and then crashing before the activation commit makes recovery
  delete that directory.
- `rollback` journals `target: next.active` (`:1198-1205`), which is by definition an existing
  installed version — the very version being restored. Any crash between `:1205` and the
  completion of `write_activation` at `:1206` makes recovery restore the old activation and
  then delete the rollback target, so the rollback point is destroyed by the recovery pass
  that was supposed to protect it.

`rollback` also omits `clear_lifecycle_journal` on the `write_activation` failure path
(`:1206` uses a bare `?`), unlike the `write_installed` path immediately below it
(`:1211-1215`), so a failed rollback leaves exactly that dangerous journal behind for the next
`FrameworkRegistry::new`.

This is the framework-side analogue of S6b2b1-2. Fix both together: give the journal an
explicit `created_target: bool` (or record `target: None` when the directory pre-existed) so
recovery only removes directories the interrupted operation actually created, and clear the
journal on every failure path.

**S6b2c2-2 (P3) — installing over an existing version directory activates that directory
without re-verifying it, and the resulting failure is unexplained.**

Every check — manifest validation, permission policy, dependency resolution, signature, trust
policy, self test (`:964-997`) — runs against the staging tree. When the computed version
directory already exists, staging is deleted and the pre-existing tree is activated unchanged:

```rust
let target_created = if target_exists {
    fs::remove_dir_all(&staging)?;
    false
} else {
    fs::rename(&staging, &target)?;
    true
};
```

(`:1038-1044`.) Nothing compares the existing tree's content against the digest just computed
from staging. The control-plane root is ACL-restricted to the current user
(`loom_plugin_security::restrict_private_path_permissions`, applied to the root at
`apps/daemon/src/lib.rs:1446`), so this is not a cross-user integrity hole, and readiness
happens to catch a divergent tree indirectly: the lockfile is written under the staging digest
(`:1047-1053`, `write_framework_lockfile:1607`) while `verify_framework_lockfile` recomputes
the digest of the *activated* directory to find that file (`:1630-1639`). A tampered or stale
directory therefore yields "cannot read locks/<digest>.json" rather than a content mismatch,
and the install reports success while the framework never becomes ready. Install should verify
the existing tree's digest and replace it (or fail with a clear diagnosis) instead of trusting
the path name.

**S6b2c2-3 (P3) — two install steps leave an orphan version directory behind on failure.**

`set_framework_tree_readonly(&target, true)?` (`:1045`) and
`register_framework_runtimes(&self.root, &manifest, &target)?` (`:1046`) both use a bare `?`.
The very next step, `write_framework_lockfile`, wraps its failure in a `target_created` cleanup
(`:1047-1058`), and so do the activation and state writes. A readonly-marking or
runtime-registration failure therefore leaves a fully unpacked version directory with no
lockfile and no activation — the outer handler only removes `staging`, which has already been
renamed away (`:1116-1118`). `register_framework_runtimes` failing also leaves nothing
registered, but a successful registration followed by a later rollback leaves a runtime entry
pointing at a removed directory until the next `prune_stale`.

**S6b2c2-4 (P3) — failures after the operation has committed are reported as if it failed.**

`prune_framework_versions(&package_root, &activation)?` runs after the activation and state
files are already written (`:1111`) and propagates `remove_framework_tree` errors
(`:1503`); that early return also skips `clear_lifecycle_journal` at `:1113`. Likewise
`uninstall` propagates `remove_framework_tree(&tombstone)?` (`:1247`) after `write_installed`
has already removed the entry. In both cases the framework is installed (or uninstalled) but
the caller — and the UI — sees an error, and a retry is not idempotent in the obvious way.
Retention and tombstone deletion are janitorial: log and continue rather than fail the
operation.

**S6b2c2-5 (P3) — crashed installs leak a full unpacked package tree at the control-plane
root, and nothing ever sweeps it.**

`staging_dir` returns `<control-plane>/.loom-framework-<id>-<nonce>` (`:1122-1128`), and the
only cleanup is the `result.is_err()` branch inside the same call (`:1116-1118`). A process
kill or power loss during unpack, validation, or the self test leaves the tree in place; the
prefix `.loom-framework-` appears nowhere else in the workspace, so neither
`recover_lifecycle_journals`, nor `recover_uninstall_tombstones`, nor the Art recovery sweeps
remove it. For the `process` framework that is the entire embedded Python runtime per failed
attempt. Add a startup sweep for stale `.loom-framework-*` directories alongside the existing
recovery passes.

**S6b2c2-6 (P3) — the embedded Python runtime is hashed three times per framework install.**

`canonical_package_digest(&staging, ...)` hashes the whole package including `python-embed`
(`:1005-1011`); `resolve_framework_dependencies` hashes `staging/python-embed` again
(`:1547`); `register_framework_runtimes` hashes `target/python-embed` a third time after the
rename (`:1577`), even though the bytes are identical and the second digest was already
computed. Thread the dependency digest through instead of recomputing it.

**S6b2c2-7 (P3) — `uninstall` performs no reverse-dependency check.**

`uninstall` (`:1222-1250`) removes the package and its state without asking whether any
installed Art declares that framework. Every Art bound to it immediately fails readiness
(`install.rs:464`) and its lockfile verification fails with "locked framework ... is no longer
active" (`install.rs:1556-1566`). This is the framework-side counterpart of S6b2b2-3: the
uninstall should at least report the dependent Arts, and ideally require a force flag.

**S6b2c2-8 (P3) — the uninstall response drops the publisher qualification.**

The final line is `Ok(self.status_of(framework_local_id(&key)))` (`:1250`) — the bare local
id, after the state entry has already been removed. `status_of` then finds no state and no
manifest, so it falls back to the catalog defaults: for a third-party framework the response
carries `qualified_id` equal to the bare id and the generic `第三方 Art 框架` name and
description (`:1266-1269`, `framework_name:286-294`). A client that keys its framework list on
`qualified_id` cannot match the row it just uninstalled. Pass `&key` instead.

Confirmed-correct behaviour worth recording:

- The uninstall crash ordering is safe in both directions: a crash after the rename but before
  `write_installed` leaves the state entry present and the live directory missing, which
  `recover_uninstall_tombstones` restores (`:729-730`); a crash after `write_installed`
  leaves no entry, which the same sweep resolves by deleting the tombstone (`:731-732`).
- `rollback` re-verifies the target completely before switching: manifest identity against the
  installed publisher (`:1162-1168`), signature (`:1170-1175`), trust policy (`:1176`, subject
  to S6b2c1-2), the digest against the immutable directory name (`:1177-1190`), the permission
  policy (`:1191-1196`), and a live self test (`:1197`).
- `is_safe_framework_version_path` requires exactly `versions/<single-component>` (`:1429-1440`),
  and both the activation and the journal predicates apply it to every stored path
  (`:1442-1457`), so no persisted pointer can escape the package root — including the
  `-recovered-<nonce>` path minted on a `PermissionDenied` legacy directory (`:1022-1034`).
- `prune_framework_versions` pins both the active and the previous version and only counts
  unpinned directories against the history limit, which is itself floored at 2
  (`:1459-1504`), so retention can never delete the rollback target.
- The install failure chain restores the previous activation, removes a freshly created target,
  and clears the journal for lockfile, activation, and state-write failures
  (`:1047-1110`) — the ordering is correct apart from S6b2c2-1 and S6b2c2-3.

### S6b2c3 — Loom framework process execution

Scope: `crates/loom_tool_registry/src/framework_process.rs:1-1020` (the three public entry
points, `execute_framework_art_in_root_with_timeout`, `normalize_framework_image_output`,
`map_process_error`, the Art metadata accessors, `resolve_mcp_server`, `split_arguments`, the
candidate helpers, `response_to_tool_value`, `request_id`). Tests occupy `:1021-1644`.

**S6b2c3-1 (P2) — image candidates produced by a framework Art never reach any consumer,
because the producer and both consumers disagree on the candidate key names.**

`insert_image_candidate_metadata` copies `response.candidates` verbatim into
`loomMetadata.candidates.items` (`:946-964`, called from `:984`). Both consumers key each item
on `imageUrl` and drop items that lack it:

- `apps/daemon/src/hook_canvas.rs:836` — `let image_url = item.get("imageUrl").and_then(Value::as_str)?` inside a `filter_map`.
- Hook `src/services/artDeliveryCandidates.ts:21-23` — returns nothing when `imageUrl` is empty.

The shipped Art emits a different shape. `art-packages/samples/image-search/runtime/main.ps1:310-324`
builds each candidate as `{ id, title, thumbnail, data, sourceUrl, width, height, index }`: no
`imageUrl` at all, and `sourceUrl` where the consumers read `sourcePageUrl`. The Art *does*
carry an `imageUrl` internally (`main.ps1:60`, used at `:292`) and then drops it when
assembling the response. Consequence: for every framework Art, `result_candidates` is always
empty, the Hook canvas takes the no-candidate branch (`hook_canvas.rs:394`), and the candidate
strip renders nothing.

The reason this is invisible in CI: only the MCP *tool* path builds `imageUrl`
(`crates/loom_tool_registry/src/lib.rs:1910`), and every candidate assertion
(`lib.rs:3097`, `:3204`, `:3272`, `:3311`) exercises that path. The framework path has no
candidate-shape test. Fix: normalize candidate keys inside
`insert_image_candidate_metadata` (accept `data`/`thumbnail`/`sourceUrl` as sources for
`imageUrl`/`thumbnailUrl`/`sourcePageUrl`) so every producer converges on the wire shape the
consumers already expect, and add a framework-path test that asserts `items[0].imageUrl`.

Related inconsistency in the same function: when the Art's declared outputs contain no image
output, candidates are written to `output.candidates` instead (`:985-987`), a key neither
consumer reads.

**S6b2c3-2 (P2) — candidates bypass every guard the image-output path applies, and the
shipped Art duplicates each image several times inside a single response.**

`normalize_framework_image_output` enforces four things on the single output image: the path
must be absolute (`:458-464`), it must canonicalize inside one of the execution output roots
(`:479-490`), it must not exceed `MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES` (`:500-509`), and it is
replaced by exactly one data URL (`:518-533`). None of that applies to `response.candidates`,
which are inserted verbatim with no path validation, no per-item limit, and no cap on the
array length or the aggregate byte count.

The amplification is concrete rather than hypothetical:

- Each image-search candidate stores the same full-resolution data URL twice, as `thumbnail`
  and as `data` (`main.ps1:288-321` — one `$dataUrl` assigned to both fields), so a grid of
  N candidates carries 2N full-size images and the thumbnail is never actually a thumbnail.
- The selected output adds two more copies: `New-ImageOutput`
  (`art-packages/shared/image-runtime-common.ps1:245-266`) emits `output_base64` *and*
  `content[0].data`, and the host's normalization removes only the path keys
  (`framework_process.rs:525-528`), leaving `output_base64` in place beside the `content` it
  inserts.

Per S6b2b1-1 the resulting value is cloned through the store while its mutex is held on the
Surface action path, so the cost is paid on the interaction hot path. Fix: bound the candidate
array and total candidate bytes in the host, downscale thumbnails in the shared Art runtime,
and drop `output_base64` once `content` exists.

**S6b2c3-3 (P3) — the image size ceiling does not bound memory, and the emitted MIME type is
a guess.** A 256 MiB file admitted at `:500` becomes roughly 341 MiB of base64 plus the
original buffer; the limit is a file-size check, not a memory budget. The wrapper hardcodes
`"mimeType": "image/png"` (`:521`) whatever the real format is, so a JPEG or WebP output is
mislabelled for every consumer that trusts the field (the data URL itself carries the correct
type, so the two disagree). **Closed by F11o 2026-08-22, with the memory half turned into a bound
rather than removed; the MIME half's premise had gone stale, see that record.**

**S6b2c3-4 (P3) — process resource limits are self-declared with no host ceiling, except the
timeout.** `stdout_mib`, `stderr_mib`, `memory_mib`, and `max_processes` from
`framework.manifest.json` override the host defaults unconditionally (`:360-378`), while the
timeout is correctly clamped with `.min(timeout)` (`:354-359`). Because stdout is buffered in
memory before parsing, a package declaring `stdoutMib: 8192` can exhaust the daemon's memory,
and `maxProcesses` is likewise unbounded. This is not a privilege boundary — the framework
already runs arbitrary code — but the manifest fields are advertised as limits, and a
mis-authored package degrades the whole daemon rather than only its own execution. Clamp each
field to a host ceiling the way the timeout is clamped.

**S6b2c3-5 (P3) — a caller's declared deadline can only shorten the bound, never extend it,
silently.** `execute_framework_art_with_timeout` and
`execute_framework_art_with_timeout_and_cancellation` both apply
`timeout.min(DEFAULT_FRAMEWORK_PROCESS_TIMEOUT)` (`:98`, `:125`), and the manifest's own
`timeoutSeconds` is then clamped again (`:354-359`). A Surface action declaring a 300 s
deadline, or a framework declaring `timeoutSeconds: 600` for a legitimately slow generation
step, is killed at 120 s and reported as `FrameworkProcessTimeout` with `timeout_ms` reflecting
the clamp rather than the request. There is no environment override. Either honour the larger
declared value or surface the clamp explicitly in the error.

**S6b2c3-6 (P3) — a cancellation token is silently discarded when the caller supplies no
timeout.** `crates/loom_tool_registry/src/lib.rs:932-945` matches on `(timeout, cancellation)`
and the `(None, _)` arm calls `execute_framework_art`, dropping the token. Such a call cannot
be aborted and runs to the 120 s default. Route `(None, Some(cancellation))` to
`execute_framework_art_with_timeout_and_cancellation` with the default timeout.

**S6b2c3-7 (P3) — the execution temp directory lives in the shared system temp under a fixed
parent, keyed by a timestamp rather than a nonce.** `:258-260` builds
`std::env::temp_dir()/loom-framework/<request_id>`, and `request_id()` is
`loom-<pid>-<unix_nanos>` (`:1013-1019`) — not the random nonce used for the framework staging
directories. Two executions inside the same process that land on the same clock tick derive
the same path; `create_dir_all` succeeds for both, they share a scratch directory, and the
first `TempDirectoryGuard::drop` (`:49-53`) deletes it while the second is still running, which
breaks image normalization because the temp root is one of the allowed output roots (`:436`).
The fixed `loom-framework` parent is also a pre-created-directory hazard on platforms with a
world-writable temp, and no permission restriction is applied here, unlike the control-plane
tree. Use a random nonce and keep framework scratch under the control-plane root.

**S6b2c3-8 (P3) — output-path containment is gated on the declared output type.**
`normalize_framework_image_output` returns immediately unless one of `tool.outputs` is an image
definition (`:451-453`). A framework whose Art declares any other output type can return
absolute `output_path`/`filePath` values that nothing validates and that survive into the tool
value verbatim. The containment check belongs to the response contract, not to the image
branch.

**S6b2c3-9 (P3) — a corrupt MCP server store is reported as "server not installed".**
`resolve_mcp_server:730-733` reads `mcp/servers.json` with
`fs::read(...).ok().and_then(|bytes| serde_json::from_slice(...).ok()).unwrap_or_default()`, so
an unreadable or malformed store yields an empty list and the error
`independent MCP server \`X\` is not installed`. Same class as S6b2c1-4 and S6b2b2 — distinguish
missing from corrupt.

**S6b2c3-10 (P3) — a protocol error embeds the framework's entire stdout in the error
message.** `:404-411` formats `invalid JSON response: {error}; stdout: {stdout_text}`, where
`stdout_text` is bounded only by the self-declared `stdout_mib` from S6b2c3-4. A single stray
`print` in a framework runtime turns a diagnosable error into a multi-megabyte string that
propagates into the UI, the logs, and the Surface error payload. Truncate to a bounded prefix.
**Closed by F11g 2026-08-22.**

Confirmed-correct behaviour worth recording:

- Package resolution is properly contained: both the packages root and the resolved package
  directory are canonicalized and the latter must start with the former (`:154-174`), which
  defeats a symlinked version directory.
- Manifest identity is checked against both the bare and the qualified id (`:199-205`), the
  entry must be `kind == "process"` with a non-empty command (`:206-212`), and the command must
  be relative, free of `..` and root components, and exist as a file inside the package
  (`:220-239`).
- `enforce_framework_permission_policy` is re-evaluated at execution time (`:213-219`), not
  merely at install time.
- Credentials travel in the stdin JSON payload (`:319-349`, `:379-386`), never through argv or
  the environment, and the MCP path splits required from optional credential env and headers
  (`:839-852`) so an unconfigured optional credential does not block the launch while a missing
  required one fails early (`:813-826`).
- MCP dependency resolution validates the enabled state, the package identity, and a real
  SemVer requirement against the installed version (`:748-804`), then runs the server's own
  `validate()`.
- `split_arguments` handles both the explicit `inputs`/`params` envelope and a flat argument
  object partitioned by the manifest's declared parameter ids, and strips `disabledParams` from
  the payload (`:885-925`).
- Failure mapping is complete and typed: spawn, timeout, cancellation, and output-limit errors
  each get their own variant (`:574-620`), a non-zero exit carries the exit code and trimmed
  stderr (`:392-403`), and a framework-reported failure status is converted with its structured
  code, message, and detail (`:415-429`).

### S6b2d1 — Loom tool definitions, Surface manifest validation, registry persistence

Scope: `crates/loom_tool_registry/src/lib.rs:32-830` (error enum, `ToolDefinition` and its
validation, `validate_surface_package_manifest:243-414`, `validate_surface_entry_path:416-435`,
`ToolExecution::validate:497-526`, `ToolRegistry:530-746`, the recovery/replace/sort helpers at
`:748-830`) plus the settings layer it calls into, `art_settings.rs:133-227` and `:574-584`.

**S6b2d1-1 (P2) — a corrupt `art-settings.json` makes the entire tool registry unreadable, and
unlike `tools.json` it has no recovery path.**
`read_tools:644-666` recovers from a damaged `tools.json` (it retries through
`recover_tools_with_trailing_delimiters:748`, writes a backup copy, and rewrites the file), but
after parsing it calls `apply_persisted_art_settings` for every tool (`:662-664`), which calls
`ArtSettingsStore::get_optional` (`art_settings.rs:133`), which calls `read_file`
(`art_settings.rs:165-170`) — and `read_file` propagates `serde_json` errors verbatim. The `?` at
`lib.rs:663` turns that into a `read_tools` failure, so every registry operation (list, get, save,
delete) fails and every Art disappears from the UI because one unrelated preferences file is
truncated. The registry's own file is defended against exactly this and the settings file is not.
`get_optional` also validates the art id it is handed (`art_settings.rs:134`, `:574-584`); an id
that passes `ToolDefinition::validate` but not `is_safe_package_id` would fail every read the same
way. Fix: treat a damaged settings file as "no settings" (log and continue) rather than as a fatal
registry error, and give it the same backup-and-reset recovery `tools.json` already has.

**S6b2d1-2 (P3) — user settings are baked into `tools.json` on the next save, and the original
name and description are then unrecoverable.**
`read_tools` returns tools that `apply_settings_metadata` has already mutated: for a locally
authored Art it overwrites `tool.name` and `tool.description` from the settings store
(`art_settings.rs:197-209`) and injects an `artUserSettings` metadata block (`:218-226`).
`save_tool_inner:556-580` then persists that same in-memory vector through `write_tools`, so the
override becomes the stored definition. Deleting the override afterwards does not restore anything —
`apply_persisted_art_settings:679-685` only strips the `artUserSettings` key, so the name and
description stay at their overridden values forever. The registry file also carries a duplicate copy
of settings state that is only refreshed when read through this crate, so any consumer that reads
`tools.json` directly can observe stale settings. Fix: apply the override as a projection at the
read boundary that is stripped again before writing, or keep the authored name in a separate field.

**S6b2d1-3 (P3) — installing a package silently deletes the user's own tool of the same id.**
`save_tool_inner:559-563` runs, when `replace_unpublished` is set, `tools.retain(|existing| existing.id != tool.id || existing.publisher_identity().is_some())`,
so a packaged install drops every unpublished definition that shares the bare id — together with its
authoring metadata — before inserting itself. The caller gets no signal that anything was removed and
there is no backup of the discarded definition. This is plausibly deliberate (adopting a hand-made
Art into its packaged form), but it is undocumented, untested for the loss case, and irreversible.

**S6b2d1-4 (P3) — the Surface manifest schema advertises a 300 s action timeout the runtime cannot
honour.**
`validate_surface_package_manifest:403-411` accepts `action.timeout_ms` up to 300_000, but the
framework executor clamps every action to `DEFAULT_FRAMEWORK_PROCESS_TIMEOUT` = 120 s
(`framework_process.rs:98`, `:125`, `:354-359`). A manifest declaring 300 s validates, installs, and
then has its actions killed at 120 s. Same root cause as S6b2c3-5; either the validator's ceiling
should match the executor's, or the executor should honour a declared longer deadline.

**S6b2d1-5 (P3) — migration chains are validated per step, never end to end, so a gap only
surfaces at the first state upgrade.**
`:309-337` checks each entry for `from != 0`, `to != 0`, `from < to`, `to <= state_schema_version`,
and uniqueness of `from`. It never checks that following the chain from schema 1 actually reaches
`state_schema_version`, and it does not require `to` values to be unique. A manifest with a hole
installs cleanly and fails later at `apps/daemon/src/lib.rs:6091-6099` with
"Surface state migration chain stops at schema {current} before {}" — at which point the user has
Surface state that cannot be upgraded. The property is statically decidable at install time: walk
`from`/`to` once and reject a manifest whose chain does not terminate at `state_schema_version`.

**S6b2d1-6 (P3) — a malformed publisher block silently degrades an Art to an unqualified
identity.**
`publisher_identity():210-216` ends in `serde_json::from_value(publisher.clone()).ok()`, so any shape
error in `packageSecurity.publisher` yields `None` instead of an error. `validate()` then skips the
`is_safe_publisher_id` check (`:195-202`), `qualified_id()` (`:219-223`) falls back to the bare id,
and that bare id is both the registry key in `save_tool_inner:565-576` (so the Art can now collide
with an unpublished tool of the same name — see S6b2d1-3) and the credential scope key at
`framework_process.rs:269` (`let art_identity = tool.qualified_id();`), so the Art reads a different
credential namespace than it would with its publisher parsed. Fix: distinguish "absent" from
"malformed" and reject the latter in `validate()`.

**S6b2d1-7 (P3) — the Surface protocol and API versions are compared for exact equality, with no
negotiation.**
`:251-262` requires `surface.protocol_version` and `api_version` to equal the host's constants,
whereas frameworks go through `negotiate_framework_protocol`. Any host-side version bump therefore
orphans every installed Surface Art at once, with no compatibility window and no way for a package
to declare a supported range.

**S6b2d1-8 (P3) — no bound on manifest collection sizes.**
`validate_surface_package_manifest` never caps the number of `variants`, `views`, `actions`,
`migrations`, `required_nodes`, or `required_capabilities`. Each is validated element-wise and each
becomes part of the registry file that is re-read, re-validated, and re-serialized on every registry
operation, so a package with a pathological manifest inflates every subsequent read.

**S6b2d1-9 (P3) — the registry has no cross-process lock, so concurrent saves lose writes.**
`save_tool_inner` and `delete_tool` are read-modify-write sequences over the whole file
(`:556-580`, `:604-632`). `write_tools:692-709` makes each individual write atomic and durable
(nonce temp file, `sync_all`, `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` on Windows), but
two overlapping saves both read the same base state and the second silently discards the first.
`art_settings.rs:172-185` is weaker still: it uses the fixed temp path
`self.path.with_extension("json.tmp")`, so two concurrent settings writes clobber each other's temp
file, unlike the registry's `create_transient_file:723-745` nonce loop.

**S6b2d1-10 (P3) — the corruption backup is written to an unpredictable path that is never
reported and never cleaned up.**
`read_tools:657` calls `self.write_corruption_backup(&content)?` and drops the returned `PathBuf`, so
nothing logs or surfaces where the damaged registry went. The file lands next to the registry as
`tools.json.corrupt-<pid>-<nanos>-<seq>` (`:730-733`) and is never pruned, so repeated corruption
accumulates full copies of the registry in the control-plane `tools` directory. `read_tools` is also
a mutating operation in this path — a plain read rewrites `tools.json` — which combines badly with
the missing lock in S6b2d1-9.

**S6b2d1-11 (P3) — settings are re-read from disk once per tool per registry read.**
`apply_persisted_art_settings:668-690` constructs a fresh `ArtSettingsStore` and calls
`get_optional` for every tool, and `get_optional` calls `read_file` (`art_settings.rs:165-170`),
which reads and parses the entire settings file each time. Loading a registry of N Arts therefore
performs N full reads and parses of `art-settings.json`, and `read_tools` runs on every list, get,
save, and delete. Reading the settings file once per `read_tools` call is a one-line change.
The same function is also silently coupled to a directory name: it returns early unless the registry
root's file name is `tools` (`:669-676`), so a registry rooted anywhere else drops user settings with
no diagnostic.

Confirmed correct in this slice:

- `validate_surface_entry_path:416-435` rejects empty paths, `\`, absolute paths, and any component
  that is not `Normal` or `CurDir`, so a manifest cannot point outside its package.
- View ids, action ids, node types, and capability ids all go through `is_safe_surface_identifier`;
  duplicate view or action ids are rejected, and `defaultViewId` must be declared and present
  (`:350-385`).
- Actions flagged high risk must also require confirmation (`:397-402`).
- `ToolDefinition::validate:191-207` requires a non-empty id with no path separator, a safe
  publisher id when one is present, a valid Surface manifest, and a valid execution block;
  `ToolExecution::validate:515-524` requires `is_valid_framework_reference` for framework
  executions.
- `save_tool_inner` keys on `qualified_id()`, so two publishers may ship the same bare id without
  colliding (`:565-576`), and `get_tool:589-600` prefers an exact qualified match, falling back to a
  bare-id match only when it is unique and raising `AmbiguousToolId` otherwise. `delete_tool`
  applies the same rule (`:621-623`).
- `recover_tools_with_trailing_delimiters:748-760` is deliberately narrow: it accepts a recovered
  prefix only when the trailing bytes are whitespace and `}`/`]` characters, so it cannot silently
  truncate a registry that is damaged in any other way.
- `replace_registry_file` on Windows (`:768-826`) canonicalizes through the parent for a
  not-yet-existing destination and converts UNC and drive paths to extended-length form, so the
  atomic replace survives long paths and network shares.

### S6b2d2 — Loom tool dispatch, cloud API execution, network policy

Scope: `crates/loom_tool_registry/src/lib.rs:832-1400` (`execute_tool*` entry points and dispatch,
`prepare_tool_arguments`, MCP argument normalization, `execute_cloud_api_tool:1067-1203`,
`cloud_network_policy:1205-1233`, `build_cloud_multipart_form:1235-1298`, `parse_cloud_method`,
`normalize_cloud_response:1321-1390`) plus `substitute_cloud_template:2268-2291` and the policy it
depends on, `network_policy.rs:67-232`.

**S6b2d2-1 (P2) — cloud templates are raw text substitution, so caller-supplied arguments can
rewrite the request authority and inject JSON fields.**
`substitute_cloud_template:2268-2281` performs plain `str::replace` of `{{key}}`,
`{{inputs.key}}`, `{{inputs.key.value}}`, and `{{inputs.key.path}}` with the raw scalar
(`scalar_template_value:2283-2291`) — no percent-encoding, no JSON escaping, no re-validation of the
result's shape. It is applied to the endpoint (`:1077`), the header block (`:1112`), and the body
(`:1134`). Consequences:

- Authority override: for a template such as `https://api.example.com{{inputs.suffix}}`, an argument
  of `@127.0.0.1:8787/` yields `https://api.example.com@127.0.0.1:8787/`, whose host is `127.0.0.1`
  with `api.example.com` as userinfo. `domain_allowed` (`network_policy.rs:191-194`) returns `true`
  for an empty `allowed_domains`, and loopback is permitted by default for cloud Arts (S6b2d2-3), so
  the request goes to a local service — while still carrying the Art's credential headers, which
  makes it a credential-exfiltration primitive as soon as the injected authority is remote.
- JSON injection: a body template `{"prompt":"{{inputs.text}}"}` with `text` = `x","stream":true`
  still parses at `:1136`, so the caller can add or override sibling fields the Art author never
  declared, including fields that appear later in the same object.

Arguments on this path originate from the canvas and the model, i.e. from content that can itself be
attacker-influenced (search results, scraped pages), so this is not a purely author-trusted input.
No shipped Art currently places a placeholder in the authority — `art-packages/samples/remove-bg/manifest.json:63`
uses `package://remove-bg` — so this is P2 today and becomes P1 for any Art that ships such a
template. Fix: percent-encode substitutions destined for a URL, build the JSON body by value
insertion instead of by string splicing, and re-validate the rendered endpoint's host against the
declared domains after substitution.

**S6b2d2-2 (P2) — the multipart file-field heuristic will read and upload an arbitrary local file.**
`is_cloud_multipart_file_field:1300-1305` classifies a field as a file when the key is
`file`/`image`/`image_file`, ends with `_file`, or the template mentions `.path}}` or
`inputs.image}}`. For such a field, `build_cloud_multipart_form:1286-1289` does
`Path::new(&rendered_value).exists()` and then `form.file(key, &rendered_value)` — with no
containment check whatsoever, unlike the framework path, which confines every path to the package or
temp root (`framework_process.rs:479-490`). A caller-supplied argument of any readable absolute path
(a credential file, an SSH key, a save game) is therefore uploaded verbatim to the endpoint. The
heuristic also misfires by name alone: an ordinary text field called `image` whose value happens to
name an existing file becomes a file upload. Fix: only treat a field as a file when the manifest
declares it as one, and require the resolved path to sit under an allowed root.

**S6b2d2-3 (P2) — cloud Arts default to loopback-allowed and to no domain restriction, inverting the
secure default.**
`cloud_network_policy:1212-1215` reads `permissionPolicy.network.allowLocalhost` and
`unwrap_or(true)`, while `OutboundPolicy::default()` sets `allow_http_loopback: false`
(`network_policy.rs:78`). An Art that declares no network policy at all therefore may call
`http://localhost:*` and `http://127.0.0.1:*` in cleartext (`network_policy.rs:183`, `:213-218`) —
the Loom daemon's own HTTP surface, Hook, a local model server — and, because `allowed_domains` is
empty, any remote host as well (`:191-194`). The comment at `:1099-1102` documents why the system
proxy is bypassed but says nothing about why loopback is on by default. Fix: default
`allowLocalhost` to `false` and require the package to declare it, matching `OutboundPolicy::default`.

**S6b2d2-4 (P2) — cloud calls are capped at 30 s with no way to raise the ceiling.**
`:930` computes `timeout.unwrap_or(CLOUD_API_TIMEOUT).min(CLOUD_API_TIMEOUT)` with
`CLOUD_API_TIMEOUT = 30 s` (`:33`), so `execute_tool_with_timeout(tool, .., Duration::from_secs(120))`
silently becomes 30 s. Image generation and background removal — the cloud Art use cases this
product ships — routinely exceed 30 s, and neither the manifest nor the caller can extend it. Same
"declared deadline can only shorten" defect as S6b2c3-5 and S6b2d1-4, but here the ceiling is low
enough to break the primary use case. Fix: let the caller and the manifest raise the bound up to a
generous host maximum.

**S6b2d2-5 (P3) — cancellation is silently ignored for MCP and cloud executions.**
`execute_tool_with_optional_timeout` threads `cancellation` only into the framework arm
(`:932-946`); the MCP arm (`:887-915`) and the cloud arm (`:916-931`) never receive it. `loom_mcp`
already exposes `McpClient::cancel()` (`loom_mcp/src/lib.rs:496-501`) and it is never called from
here. So `execute_tool_with_timeout_and_cancellation` is a no-op for two of the three supported
execution kinds: cancelling a canvas run leaves a hung MCP or cloud request running until its
timeout. See also S6b2c3-6 for the third gap on the framework path.
**MCP half closed by F11i 2026-08-22**; the cloud half and the runner that never handed the flag over
**closed by F11j 2026-08-22**. Interrupting a request already in flight stays open on both transports.

**S6b2d2-6 (P3) — a failed `tools/list` silently degrades argument normalization.**
`:904` is `let tool_list = client.list_tools().ok();`. When listing fails, `input_schema` is `None`,
so `normalize_mcp_argument_value:997-1046` performs no coercion and string arguments are sent where
the server expects an integer, number, or boolean. The failure surfaces as a server-side validation
error about a type the host could have fixed, with nothing pointing at the swallowed list failure.
**Closed by F11g 2026-08-22** — the listing failure is now folded into the call error when the call
itself fails.

**S6b2d2-7 (P3) — multipart fields and bodies are dropped silently rather than reported.**
`:1253-1258` skips any rendered multipart value that is empty, equals `__DISABLED__`, or still
contains `{{`. An unresolved binding therefore removes the field from the request instead of
erroring, so the API answers with a confusing 4xx about a missing parameter; a legitimate value that
happens to contain `{{` is dropped for the same reason. Likewise `:1130` only attaches a body for
POST/PUT/PATCH, so a `body` declared on a GET or DELETE Art is silently ignored rather than rejected
at validation time.
**Closed by F11h 2026-08-22.**

**S6b2d2-8 (P3) — the 64 MiB response bound is not a memory bound, and error bodies are embedded
whole.**
The limit is correctly enforced twice (`:1169-1178` on `Content-Length`, `:1179-1191` on the actual
stream), but an image response is then base64-encoded in full at `:1334-1337`, so 64 MiB of bytes
becomes roughly 85 MiB of string on top of the original buffer, and the non-image path copies the
whole body through `String::from_utf8_lossy` at `:1340`. A non-success status puts the entire trimmed
body into `ToolRegistryError::CloudHttpStatus` (`:1193-1200`), which is the same unbounded-error-text
problem as S6b2c3-10 and additionally risks logging whatever the API echoed back, credentials
included.
**Error-text half closed by F11g 2026-08-22**; the memory half (base64 expansion and the
`from_utf8_lossy` copy) remains open.

**S6b2d2-9 (P3) — a JSON `data` string that merely looks like base64 is rendered as a PNG.**
`normalize_cloud_json_value:1376-1385` treats any top-level `data` string that starts with
`data:image/` **or** satisfies `looks_like_base64_payload` as an image, defaulting the MIME type to
`image/png` when the response does not state one (`:1372`, `:1381`). An API that returns an opaque
token, a signature, or an encoded cursor under `data` is therefore surfaced to the canvas as a broken
image instead of as text. **Closed by F11e 2026-08-22.**

Confirmed correct in this slice:

- Redirects are re-validated per hop against the same policy and capped
  (`network_policy.rs:95-103`), so a redirect cannot escape the domain and IP rules — the usual SSRF
  bypass is closed.
- `validate_outbound_url` resolves the host and checks every returned address against the loopback,
  private, link-local, broadcast, and documentation classes for both IPv4 and IPv6
  (`network_policy.rs:150-232`). A DNS rebind between validation and connect remains theoretically
  possible, but the check is otherwise complete.
- Only GET, POST, PUT, PATCH, and DELETE are accepted, and anything else is a typed error
  (`parse_cloud_method:1307-1319`).
- Argument coercion honours the server's declared `inputSchema`, including union `type` arrays and
  case-insensitive `enum` canonicalization, and leaves the value untouched when nothing matches
  (`:979-1057`).
- `prepare_tool_arguments:954-965` merges persisted defaults and resolves value bindings, converting
  a binding failure into a typed `ParameterBinding` error carrying the qualified id.
- Multipart data URLs are decoded through `loom_image_io::decode_data_url_bytes` with the MIME type
  mapped to a plausible file extension (`:1261-1285`).
- `normalize_cloud_json_value:1359-1365` passes a response that already carries a `content` array
  through untouched, so MCP-shaped cloud responses are not double-wrapped.

### S6b2d3 — Loom MCP result normalization, image candidate collection, download fallback

Scope: `crates/loom_tool_registry/src/lib.rs:1402-2321` (`normalize_mcp_result` and the image
heuristics, `collect_mcp_image_candidates*:1606-1683`, `image_candidate_from_object:1685-1726`, URL
normalization `:1728-1959`, `download_mcp_image_candidate*:1961-2144` including the Windows
PowerShell fallback, `image_response_from_mcp_candidate*:2146-2178`, MIME inference and response
builders `:2180-2266`).

**S6b2d3-1 (P2) — the image downloader explicitly enables loopback for URLs chosen entirely by the
MCP server, giving any MCP server an SSRF primitive into local services.**
Both download paths hardcode `allow_http_loopback: true` (`:1970-1973`, `:2027-2030`), and the URL
they fetch is whatever the MCP server put in its response — it is never author-declared and never
user-confirmed. `validate_url_without_dns` therefore accepts `http://127.0.0.1:<port>/...` and
`validate_ip` accepts the loopback address (`network_policy.rs:183`, `:213-218`), so a malicious or
compromised image-search server can make the daemon issue arbitrary local GETs: port scanning by
timing and error differentiation, plus content disclosure, because the response body is base64-encoded
into the canvas as an image whenever a MIME type resolves — and `infer_image_mime_type_from_url:2180-2207`
resolves one from the URL suffix alone, which the same server controls
(`http://127.0.0.1:9200/whatever.png`). Unlike the cloud path (S6b2d2-3), there is no policy block an
Art could set to turn this off. Fix: use `OutboundPolicy::default()` here (loopback denied) — image
candidates have no legitimate reason to be loopback URLs — or gate it behind an explicit developer
setting.

**S6b2d3-2 (P2) — candidate collection recurses without a depth or count bound over untrusted MCP
output.**
`collect_mcp_image_candidates_from_value:1638-1683` walks objects and arrays recursively and, at
`:1675-1679`, re-parses any string that starts with `{` or `[` and recurses into the result. serde_json
bounds a single parse to its own nesting limit, but each embedded-JSON layer restarts that budget on
top of the current native stack, so an MCP server can drive recursion arbitrarily deep and overflow
the stack — which aborts the process rather than returning an error. Nothing caps the number of
candidates collected either, so the `Vec<McpImageCandidate>` and the `loomMetadata.candidates.items`
array built from it at `:1898-1919` are as large as the server wants. Fix: pass an explicit depth
counter and a candidate ceiling through the walk and stop at both.

**S6b2d3-3 (P2) — a failing candidate set turns one tool call into an unbounded stall.**
`image_response_from_mcp_candidates:1591-1603` tries the requested candidate and then **every other
candidate in turn**. For each, `image_response_from_mcp_candidate:2146-2155` tries the image URL and
then the thumbnail URL; `image_response_from_mcp_candidate_url:2168-2176` tries the URL and its
stripped variant; and `download_mcp_image_candidate:1961-1964` tries reqwest and then the PowerShell
fallback. Each network attempt is bounded only by `CLOUD_API_TIMEOUT` = 30 s (`:33`, `:1977`,
`:2114`), so a single candidate can cost about two minutes and each PowerShell attempt additionally
spawns a process. Fifty dead candidates — a normal size for an image-search result set — is therefore
on the order of an hour of blocking work inside one tool call, with no overall deadline (the caller's
timeout was spent on the MCP client only) and no way to cancel it (S6b2d2-5). The user sees nothing
but a stalled node until "图片搜索已返回候选结果，但图片下载失败" finally appears. Fix: budget the whole
candidate loop against a single deadline and cap the number of candidates attempted.

**S6b2d3-4 (P3) — `looks_like_base64_payload` matches almost any short token, so ordinary text is
treated as an image.**
`:2260-2266` accepts any string of length ≥ 8 with no whitespace whose characters are alphanumeric or
in `+/=-_`. `"completed"`, `"12345678"`, a request id, a hex digest, or a slug all qualify. That
predicate short-circuits normalization in `mcp_result_already_contains_image:1445-1452` and turns a
text field into an image in `normalize_cloud_json_value:1382` (S6b2d2-9). `image_content_response:2232-2237`
then wraps the value in `data:image/png;base64,completed`, so the canvas receives a broken image with
no diagnostic anywhere. Fix: require a length in the kilobyte range, a length that is a multiple of 4,
and no `-`/`_` mixing with `+`/`/`. **Closed by F11e 2026-08-22.**

**S6b2d3-5 (P3) — a data-URL candidate is forwarded to the canvas without being decoded.**
`image_response_from_mcp_candidate_url:2161-2166` checks only an approximate length bound
(`MAX_MCP_IMAGE_BYTES * 4 / 3 + 4096`) and then returns the server's string verbatim with the MIME
type parsed out of the URL itself. Malformed base64, a truncated payload, or a MIME type that does
not match the bytes all reach the canvas unvalidated, whereas the downloaded path at least confirms
the bytes against `infer_image_mime_type_from_bytes:2209-2224`. **Closed by F11f 2026-08-22.**

**S6b2d3-6 (P3) — URL "modifier" stripping can replace a candidate's real URL with a wrong one.**
`normalize_image_candidate_url:1766-1771` stores the *stripped* URL as the candidate's `image_url`
when the original does not already end in an image extension, and
`strip_image_url_modifiers:1728-1756` truncates at the last image extension followed by `!`, `/`, or
end of path. That is right for the `.../a.jpg!600x400` CDN convention, but for a path like
`https://host/logo.png/v2/actual.jpeg` it yields `https://host/logo.png` — and because the stripped
form replaced the original in the candidate, the true URL is never retried. **Closed by F11k
2026-08-22.**

**S6b2d3-7 (P3) — SVG is accepted as an image candidate and delivered as a data URL.**
`looks_like_image_url:1950-1954` and `infer_image_mime_type_from_url:2199-2200` both accept `.svg`,
so `data:image/svg+xml;base64,...` can be handed to the Hook canvas from an untrusted search result.
SVG is an active-content format, and nothing downstream treats it differently from a raster image.
Note the inconsistency as well: `infer_image_mime_type_from_bytes` has no SVG branch, so an SVG whose
URL lacks the extension is rejected while the same bytes behind a `.svg` URL are accepted. Decide
deliberately whether SVG is in scope, and if it is, confirm how Hook renders it.
**Closed by F11f 2026-08-22** — decided out of scope in both the host and the Art.

**S6b2d3-8 (P3) — an out-of-range candidate index is silently clamped, and the reported index can
differ from both the request and the clamp.**
`selected_mcp_image_candidate_index:1857-1880` clamps the requested index to
`candidate_count - 1`, so a request for index 7 of 3 quietly returns the third. If that candidate
then fails to download, `image_response_from_mcp_candidates` falls through to another one and
`attach_mcp_image_candidate_metadata` records that fallback as `selectedIndex` (`:1494`, `:1903`), so
the canvas is told a different image was selected than the one asked for, with no explanation.
**Closed by F11h 2026-08-22.**

**S6b2d3-9 (P3) — the PowerShell fallback duplicates the timeout budget and cannot follow
redirects.**
`download_image_bytes_with_powershell_httpclient:2033-2119` sets a 30 s `HttpClient.Timeout` inside
the script *and* a 30 s `ProcessSpec` timeout outside it, so a slow host burns the budget twice
across the two download paths. The handler also sets `AllowAutoRedirect = $false`, so any URL that
legitimately redirects (most CDN and image-proxy URLs) fails in the fallback even though the reqwest
path would have followed and re-validated the hop — the fallback is therefore least likely to work
precisely for the URLs that made reqwest fail.

**S6b2d3-10 (P3) — user-facing strings are hardcoded Chinese in the daemon, with no locale
plumbing.**
`:1504`, `:1568`, and `:1571` embed Chinese messages that are returned into the canvas for every
user regardless of locale. Loom has no i18n layer at all, unlike Tea's frontend; at minimum these
should be structured codes the presentation layer can localize.

**S6b2d3-11 (P3) — the fetch user agent impersonates a specific Chrome build.**
`MCP_IMAGE_FETCH_USER_AGENT:34-35` is a fixed `Chrome/138.0.0.0` string. The intent is understandable
(image hosts reject non-browser agents), but it is a hardcoded version that will age into an
obviously-fake agent, and it misrepresents the client to every host. Consider a Loom agent string
with a browser-compatible fallback only where a host demands it.

Confirmed correct in this slice:

- The PowerShell script is a fixed literal and every input — URL, referer, byte cap, headers — is
  passed through the environment rather than interpolated into the script text (`:2090-2103`), so
  there is no script injection from server-supplied URLs.
- That script enforces the byte cap twice, on `Content-Length` and again while streaming
  (`:2054-2066`), and the host re-checks the decoded length afterwards (`:2132-2134`).
- The fallback process is fully bounded: timeout, stdout, stderr, memory, and process count
  (`:2114-2118`), and runs with `CREATE_NO_WINDOW` (`:2107-2112`).
- Both download paths validate the URL against the outbound policy before connecting (`:1975`,
  `:2032`), and the referer header is only attached when it is a remote URL (`:1986`, `:2100-2102`),
  so a local path can never leak into a request header.
- MIME resolution prefers the response header, then the URL, then the actual magic bytes
  (`:1999-2001`, `:2135-2142`), and unknown bytes cause the candidate to be rejected rather than
  guessed.
- Candidates are deduplicated by URL through a `BTreeSet` (`:1646`, `:1663`), and the requested index
  is attempted first with the remaining candidates as ordered fallbacks (`:1591-1603`).
- `mcp_result_already_contains_image:1429-1458` passes an already-image-bearing result through
  untouched, so a compliant MCP server's output is never rewritten.
- The empty-result path distinguishes a provider "might be offensive" flag from an ordinary empty
  result and reports both distinctly (`:1543-1572`), reading `items` whether it arrives as an array
  or as an embedded JSON string (`:1574-1582`).

### S7a — Loom MCP package install, trust, digest verification

Scope: `crates/loom_mcp/src/package.rs` (532 lines), cross-read against
`crates/loom_tool_registry/src/secure_zip.rs`, `crates/loom_tool_registry/src/install.rs`,
`apps/art-store/src/lib.rs:900-960`, and `crates/loom_mcp/src/lib.rs:225-241, 922-946`.

**S7a-1 (P2) — MCP package extraction reimplements zip handling and omits every hardening the Art
installer already has, including the actual-bytes bound.**
`extract_package:307-349` accounts only for `entry.size()` — the *declared* uncompressed size in the
zip header (`:330-335`) — and then writes the entry with an unbounded `std::io::copy` (`:345`). The
declared size is attacker-controlled and unrelated to what the decompressor actually emits, so an
archive whose entries declare a few kilobytes can expand to arbitrarily many gigabytes on disk before
any CRC mismatch is noticed at end of stream. The staging tree is removed afterwards (`:146-148`), so
this is disk exhaustion rather than a permanent leak, but the peak is unbounded.
Loom already solved this: `secure_zip::extract_zip_securely` bounds the real byte count with
`copy_bounded:125-128` (`Read::take(limit + 1)`), enforces a per-entry cap, rejects suspicious
compression ratios (`:91-96`), rejects duplicate and case-colliding paths (`:73-75`), rejects Windows
reserved names and trailing dot/space components (`:136-156`), checks parent directories for symlinks
(`:103-109`), opens with `create_new` so an entry can never overwrite an earlier one (`:110-113`), and
`sync_all`s each file (`:119`). `extract_package` has none of that and uses `fs::File::create`
(`:344`), which overwrites. The overwrite is the sharpest of these: an archive containing
`mcp.server.json` **twice** leaves the *last* copy on disk, which is what `install_server_package`
parses at `:121`, while a store-side or human reviewer reading the archive by name sees the *first* —
a manifest-confusion split between what was reviewed and what was installed. On Windows the missing
case-collision check gives the same split via `Mcp.Server.json`.
Fix: use one extractor for both package types. `secure_zip` is `pub(crate)` in `loom_tool_registry`,
which already depends on `loom_mcp` (`crates/loom_tool_registry/Cargo.toml:14`), so it has to move
down — into `loom_process` or a new small archive crate — rather than be imported upward.

**S7a-2 (P2) — MCP server packages have no signature, no certification, and no trust tier; the
publisher is whatever the manifest claims. Confirms S6b2b1-4.**
`McpServerPackageManifest:22-36` has no security block at all — no signature, no certification, no
`packageSecurity` counterpart to the Art manifest — and `validate_manifest:249-305` only checks that
`publisher.id` is a *syntactically* safe identity (`:258-267`). Any package can therefore claim
`publisher.id: "neuro.official"`. Compare the Art path, which verifies a detached signature over a
SHA-256 digest against a configured key and rejects a digest mismatch outright
(`apps/art-store/src/lib.rs:923-955`), and which carries trust tiers into the installer. What is being
installed here is not passive data: for `McpTransport::Stdio` the manifest names an executable inside
the package (`:291-295`) that becomes `config.command` (`:198-205`) and is later spawned by the daemon
with the user's credentials injected as environment variables (`:217-221`). So the weakest-verified
install path in the product is the one that ships code. Fix: require the same signature/certification
chain as Art packages before an MCP package may be installed, and bind the accepted `publisher.id` to
the verified key rather than to the manifest text.

**S7a-3 (P2) — the recorded digest is never re-derived and the state file that records it is never
read by any code. Confirms S6b2b1-5 and escalates it as planned.**
`install_server_package:112` hashes the archive bytes once, `write_active_state:364-392` persists
`{qualifiedId, version, digest, packageDir}` to `mcp/packages/<publisher>/<id>/active.json`, and
`config_from_manifest:236-242` copies the same digest into `McpServerPackageState`. A workspace-wide
search for `active.json` readers finds `read_art_activation` for Arts
(`loom_tool_registry/src/install.rs:938, 984, 1143, 1225, 1679`) and
`framework_process.rs:1108`, and **nothing at all** for the MCP path — the MCP `active.json` is
write-only. Nor is the digest checked anywhere else: the extracted files on disk are never hashed
individually, and the launch path takes command/args/env straight from `servers.json`
(S6b2c3-9). The consequence is that after install, any process able to write inside
`mcp/packages/<publisher>/<id>/versions/<version>-<prefix>/` can replace the server executable and the
daemon will spawn the replacement with credentials, with no detection at any layer. The Art path is
not only verified but has a regression test for tampering
(`install.rs:3765` `write_art_activation(..., &tampered_activation)`); the MCP path has neither. Fix:
record per-file digests at install, re-verify the entry command (at minimum) before each spawn, and
give the MCP `active.json` a reader so the persisted state is authoritative instead of decorative.

**S7a-4 (P3) — the version directory is keyed on a 48-bit digest prefix and, when it already exists,
the newly extracted content is thrown away with no verification of what is reused.**
`:137` builds `versions/{version}-{&digest[..12]}` — 12 hex characters, 48 bits — and `:138-142`
removes the staging tree and reuses the existing directory whenever that path exists. Nothing checks
that the reused tree matches the digest just computed, and nothing even checks that
`manifest.entry.command` exists inside it, although `validate_manifest` verified that only against
`staging_root` (`:293`). Two consequences: an attacker who controls two packages needs about 2^24 hashes
to find a pair sharing a version string and a 12-hex prefix, after which installing the malicious one
first makes the benign one execute the malicious files while recording the benign full digest; and an
earlier interrupted install that left a directory behind silently shadows the new content, producing a
`config.command` that points at a file that may not be there. Fix: key on the full digest, and verify
(or re-extract) whenever the target already exists. **Reuse half closed by F8u 2026-08-22; naming half
closed by F11l 2026-08-22.**

**S7a-5 (P3) — `active.json` uses a fixed temporary path and is not synced before the rename.**
`write_active_state:380` writes to `package_root.join("active.json.tmp")`, a constant, so two
concurrent installs of the same package interleave on the same temp file — the same class of defect as
`art_settings.rs` (S6b2d1-9), and unlike `create_transient_file`'s nonce-based naming. The payload is
also written with `fs::write` and never `sync_all`'d before `replace_file`, so
`MOVEFILE_WRITE_THROUGH` flushes the rename but not necessarily the content — unlike
`write_tools` and `secure_zip:119`, which both sync. **Closed by F11m 2026-08-22.**

**S7a-6 (P3) — `MAX_PACKAGE_FILES = 128` is far below what a real MCP server ships.**
`:17` caps the archive at 128 entries while the Art extractor allows 4096 (`secure_zip.rs:9`). Any MCP
server that vendors its dependencies — the normal shape for an npm or Python server — blows past 128
immediately, and the limit is discovered only when install fails. Either raise it to the Art limit or
document it as a packaging constraint. **Closed by F11m 2026-08-22 (raised to the Art limit).**

**S7a-7 (P3) — only identifiers are length-bounded; free text and list fields are unbounded.**
`is_safe_identity:443-450` caps ids at 128 bytes, but `name`, `description`, each credential `label`,
and the `tools` and `entry.args` vectors have no bound at all (`validate_manifest:271-289` checks only
that `name` is non-empty). A manifest can therefore push megabytes of `description` and thousands of
`tools`/`args` entries into `servers.json` and from there into the operator UI. `tools` entries are
also never validated as safe identifiers even though they determine which tools the server exposes.

**S7a-8 (P3) — no version ordering, no rollback protection, and no pruning.**
`Version::parse:268` is used only as a syntax check; its result is discarded. Installing 1.0.0 over an
already-active 2.0.0 succeeds and silently becomes the active version, so a downgrade to a known-bad
build is indistinguishable from an upgrade. Every distinct digest also keeps its own tree under
`versions/` indefinitely — nothing prunes superseded versions, and `uninstall_server_package` is the
only thing that ever removes them (all at once).

**S7a-9 (P3) — a missing or misplaced manifest is reported as an IO error.**
`:120-127` maps only *parse* failure to `InvalidManifest`; the `fs::read(&manifest_path)?` on
`:121` propagates through `#[from] std::io::Error`, so an archive with no root-level
`mcp.server.json` surfaces as "MCP server package IO failed: The system cannot find the file
specified" instead of naming the missing manifest. Check for the manifest explicitly before reading.

**S7a-10 (P3) — coverage is two tests, and the security-relevant branches are all unexercised.**
`tests:460-532` covers the stdio happy path plus `..` traversal. Nothing covers symlink entries,
duplicate or case-colliding entries, the `target_dir.exists()` reuse branch, `MAX_PACKAGE_FILES`,
`MAX_EXTRACTED_BYTES`, the streamable-http manifest branch, duplicate credential ids, or either
`InvalidManifest` guard in `uninstall_server_package:159-184` — that is, the two checks that stop a
tampered `servers.json` entry from pointing `remove_dir_all` outside the package root are never
tested. Feeds S9.

Confirmed correct in this slice:

- Traversal is blocked by `enclosed_name():320-323` and covered by
  `rejects_package_path_traversal:516-531`; symlink entries are rejected via the unix mode bits
  (`:324-329`).
- `safe_relative_path:351-362` rejects an empty, absolute, or non-`Normal`-component `entry.command`,
  and `validate_manifest:292-295` confirms the command exists before the install is accepted.
- `is_safe_identity:443-450` rejects `..`, bounds ids at 128 bytes, and allows only ASCII
  alphanumerics with `-`, `_`, `.`.
- `uninstall_server_package:152-189` re-validates the publisher, re-derives the id from
  `qualified_id` and requires it to match, and requires `package_dir.parent()` to equal the expected
  `versions/` root before calling `remove_dir_all` — a real defense against a tampered registry entry.
- `config.validate():244` routes the streamable-http case into `validate_remote_config`
  (`lib.rs:922-946`), which enforces an `http`/`https` scheme, rejects embedded credentials, requires
  a host, rejects fragments, rejects leftover `{`/`}` template markers, and validates the header map.
- The staging directory is unique per process and nanosecond (`staging_name:452-458`) and is removed on
  both the success and the failure path (`:146-148`).
- Windows `replace_file:399-441` canonicalizes source and destination parent and replaces with
  `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`.
- Credential targets map to environment variables and headers separately, and duplicate credential ids
  are rejected (`:276-289`).

### S7b1 — Loom MCP config, Windows spawn-command resolution, registry URL, handshake

Scope: `crates/loom_mcp/src/lib.rs:1-535` (error taxonomy, `McpServerConfig` and `validate`,
`spawn_command_spec` and the Windows resolution helpers, `build_registry_url`, the `initialize` /
`tools/list` / `tools/call` request builders, `McpClient` dispatch), plus `percent_encode:1124-1136`.

**S7b1-1 (P2) — the spawn path never re-anchors a packaged server's command inside its package
directory, so "installed package" provenance shown in the UI is not enforced at launch.**
`spawn_command_spec:250-257` takes `config.command` exactly as it appears in `servers.json`. For a
packaged server that string was originally `target_dir.join(entry.command)`
(`package.rs:198-205`), but nothing at spawn time checks that it still resolves inside
`config.package.package_dir`, and `McpServerConfig::validate:234-239` requires only that it be
non-blank. Chained with S6b2c3-9 (`servers.json` supplies command, args, and env) and S7a-3 (the
recorded digest is never re-derived), an edited registry entry can keep the `package` block — publisher,
version, digest — while pointing `command` at any executable on the machine; the operator UI still
presents it as the installed package. On Windows an extensionless command additionally goes through a
`PATH` search (`resolve_windows_command_path:283-294`), so the resolved binary depends on ambient
environment rather than on the package. Fix: when `config.package.is_some()`, canonicalize `command`
and require it to be inside `package_dir`, and reject a `PATH` search for packaged servers entirely.

**S7b1-2 (P3) — a relative command is resolved against the daemon's current directory.**
`is_windows_path_qualified:369-374` treats any path with a non-empty parent as qualified, and
`resolve_windows_command_path:286-288` then resolves it with an empty search-path list, i.e. relative
to the daemon's process CWD. The non-Windows fallback at `:256` has the same property. Which binary
runs therefore depends on where the daemon was started. Require absolute paths (or paths relative to
an explicit, validated root) for stdio commands. **Closed by F11n 2026-08-22.**

**S7b1-3 (P3) — protocol revisions are hardcoded per transport, differ by nearly two years, and are
never negotiated.**
`:22-23` pin stdio to `2024-11-05` and streamable-HTTP to `2026-07-28`.
`initialize_request_for_version:424-438` sends whichever applies and nothing compares the server's
returned `protocolVersion` against it, unlike Loom's own `negotiate_framework_protocol` (cross-ref
S6b2d1-7). Two problems: the transport should not determine the protocol revision at all, and the
stdio pin is stale enough that servers implementing only newer revisions may refuse or silently
degrade. Also worth noting here: `capabilities` is sent as `{}` (`:431`), so Loom advertises no roots,
sampling, or elicitation support and a server that needs them fails opaquely; and `clientInfo.version`
reports `LOOM_MCP_VERSION`, the crate version, not the application version.

**S7b1-4 (P3) — `.bat` and `.cmd` are resolvable MCP entry points, spawned through `cmd.exe`.**
`windows_path_extensions:324-349` keeps whatever `PATHEXT` contains (default includes `.BAT`/`.CMD`)
and `resolve_windows_spawn_command:276-279` hands the result to a direct spawn. On Windows,
`std::process::Command` runs batch files via `cmd.exe`, so argument safety rests entirely on the
standard library's batch-argument escaping (present since Rust 1.77, and the workspace is on 1.95).
That is a single mitigation between manifest-supplied `args` and a shell. Given that MCP entry commands
come from packages, prefer rejecting `.bat`/`.cmd` entry points outright. **Closed by F11n 2026-08-22
for packaged servers; unpackaged servers keep batch files, see that record for why.**

**S7b1-5 (P3) — `.ps1` is force-added to the executable-extension list, and script arguments are
appended where the script's own parameter binder sees them.**
`:341-346` appends `.ps1` to `PATHEXT` if absent, so an extensionless command can resolve to a script
that Windows itself would not treat as executable — the resolution rule diverges from the platform's.
`windows_powershell_spawn_spec:385-395` then appends the configured `args` after `-File <script>`, so
an argument beginning with `-` binds to the script's parameters (`-Verbose`, `-Debug`, or any declared
switch) rather than being passed as data. `powershell.exe` is also hardcoded, with no `pwsh` fallback,
so a server that needs PowerShell 7 semantics silently runs under 5.1.

**S7b1-6 (P3) — user-added servers are validated far more weakly than packaged ones.**
`validate:225-241` requires only that `id` and `name` be non-blank. There is no `is_safe_identity`
check on `id` even though the package path insists on one (`package.rs:258-267`), and `id` flows into
registry keys and into the credential name format `mcp-{digest[..16]}-{credential_id}`
(`apps/daemon/src/lib.rs:7337`). `name`, `description`, `args`, `env`, and `tools` are unbounded.
Apply the same identity rule and length bounds on both paths.

**S7b1-7 (P3) — `env` is a free-form environment map with no denylist for process-influencing
variables.**
`McpServerConfig.env:120-121` is deserialized straight from `servers.json` and carried to the child
process. Nothing prevents entries such as `PATH`, `PSModulePath`, `NODE_OPTIONS`, `PYTHONPATH`, or
`PYTHONSTARTUP`, each of which redirects what code the server process actually loads — which matters
precisely because the command itself is unverified (S7b1-1). Confirm how the map is applied in S7b2 and
add a denylist for loader-influencing names.

**S7b1-8 (P3) — a declared error variant is never constructed and an infallible function is typed as
fallible.**
`McpError::InvalidRegistryQuery:30-31` has no construction site anywhere in the workspace, and
`build_registry_url:398-416` ends in `Ok(...)` on every path while returning `McpResult<String>`. Every
caller therefore carries a `?` for an error that cannot occur, and the variant's name implies a
registry-query validation step that does not exist. Either validate `search`/`cursor` and use the
variant, or make the function infallible and delete it.

Confirmed correct in this slice:

- `percent_encode:1124-1136` encodes byte-wise against the RFC 3986 unreserved set
  (`A-Za-z0-9-_.~`), so it is UTF-8 safe and cannot leak `&`, `=`, `#`, or spaces into the query; it is
  applied to both `search` and `cursor` (`:407`, `:411`).
- `build_registry_url` clamps `limit` to `1..=100` with a default of 60 and targets a hardcoded
  `https://` endpoint (`:21`, `:403`).
- `connect_with_timeout:454-473` checks `enabled` and then `validate()` **before** spawning a process
  or opening a connection, so a disabled or malformed server never reaches either transport.
- `resolve_windows_command_candidates:316-321` requires `is_file()`, so a directory named like an
  executable is not selected, and `windows_path_extensions` normalizes extensions to lowercase with a
  leading dot and falls back to `.com/.exe/.bat/.cmd` when `PATHEXT` is unset.
- `McpClient:441-502` dispatches every operation to both transports exhaustively, including `cancel()`.
- `credential_bindings:131-132` is not a dead field — the daemon reads and writes it throughout
  (`apps/daemon/src/lib.rs:7069`, `:7296-7317`, `:7345`, `:8540-8561`).
- The error taxonomy (`:28-66`) distinguishes timeout, output-limit, process-exit, supervision, and
  JSON-RPC failures with the stderr tail attached where it is available, which is what makes the
  operator-facing messages in the daemon usable.

### S7b2 — Loom MCP stdio and streamable-HTTP clients, framing, bounded IO

Scope: `crates/loom_mcp/src/lib.rs:536-1136` — `configure_runtime_limits`, `BoundedStderr`,
`StdioMcpClient` (spawn, initialize, call, read/write framing, cancel),
`StreamableHttpMcpClient` (connect, initialize, call, header construction, SSE parsing),
and the shared helpers `read_bounded_http_body`, `bounded_error_body`, `parse_sse_messages`,
`result_from_messages`, `read_stdout_lines`, `drain_stderr`.
The in-file test module (`:1137-1733`) is deliberately out of scope for this slice.

**S7b2-1 (P2) — remote MCP URLs bypass Loom's outbound network policy entirely.**
`StreamableHttpMcpClient::connect_with_timeout:782-815` builds a bare `reqwest::blocking::Client`
and connects straight to `config.url`. There is no `OutboundPolicy`, no `validate_outbound_url`
call, and no DNS resolution or IP-class check — unlike every cloud-API request in
`loom_tool_registry`, which resolves the host and rejects loopback, link-local, and RFC1918
targets before connecting. `validate_remote_config` only requires the scheme to be `http` or
`https`, so `http://127.0.0.1:9200`, `http://192.168.1.1/admin`, and
`http://169.254.169.254/latest/meta-data/` are all accepted. Two things make this worse than a
generic SSRF: the URL for a *packaged* server comes from an unsigned `mcp.server.json`
(see S7a-2), so installing a package is enough to choose the target; and any credential headers
bound to that server (`credential_headers`, `build_remote_headers:948-984`) are attached to the
request, so a plain-`http` URL sends operator tokens to an attacker-chosen host in cleartext.
Fix: validate the URL through the same outbound policy the art/cloud paths use, and reject
plain `http` whenever credential headers are present (loopback excepted, and only when the
operator opted in explicitly). Note this has the same crate-direction obstacle as S7a-1:
`network_policy` lives in `loom_tool_registry`, which depends on `loom_mcp`
(`crates/loom_tool_registry/Cargo.toml:14`), so the shared security primitives — the hardened
zip extractor and the outbound URL validator — need to move down into a lower-level crate that
both can use. That single relocation unblocks S7a-1 and S7b2-1 together.

**S7b2-2 (P3) — `read_result` has no overall deadline, only a per-message one.**
`:698-750` loops on `recv_timeout(self.request_timeout)` and `continue`s on blank lines
(`:730-732`), unparseable JSON (`:734-736`), and id mismatches (`:738-740`). Every received line
resets the budget, so a server that prints one line per 59 s — progress chatter, a log banner,
notifications for a different id — keeps the call blocked indefinitely while never answering it.
The 60 s `DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS` therefore bounds silence, not call duration.
Fix: compute a deadline once before the loop and pass the remaining time to each
`recv_timeout`. Related: an unparseable line is silently dropped, so a server emitting
consistently malformed JSON is indistinguishable from one emitting nothing — worth a single
counter or a debug log so the resulting timeout carries a cause.

**S7b2-3 (P3) — one oversized stdout line is fatal to the whole server.**
`read_stdout_lines:1085-1101` marks the current line `oversized` once it passes
`MCP_MAX_MESSAGE_BYTES` (8 MiB) and emits `StdoutEvent::Oversized`; `read_result:702-707`
reacts by terminating the child and returning `OutputLimit`. The framing does not distinguish
an oversized *response* from an oversized *incidental* line, so a server that dumps a long
diagnostic to stdout takes the session down instead of losing one message. Fix: skip the
oversized line (resynchronize at the next newline) and only fail the pending request if the
oversized line was the one carrying its id — or at minimum keep the process alive and fail just
the in-flight call.

**S7b2-4 (P3) — the stdio client never inspects the negotiated protocol version.**
`initialize:665-671` sends `initialize`, reads the result, and immediately sends
`notifications/initialized` without comparing the server's `protocolVersion` against the
version Loom asked for. A server that answers with a different revision is treated as
compatible. The HTTP client at least records what came back (`:821-828`), so the two transports
disagree on rigor. Cross-ref S7b1-3, which covers the pinned-and-stale request versions; this
is the missing check on the response side.

**S7b2-5 (P3) — the HTTP client adopts the server's `protocolVersion` unconditionally.**
`:821-828` overwrites `self.protocol_version` with any non-empty string the server returns, and
`:865` echoes that string back in the `MCP-Protocol-Version` header of every later request.
There is no allowlist and no charset check, so a server can pick a revision Loom does not
implement — or a value containing characters that are invalid in an HTTP header, which makes
`request.header(...)` fail at send time and wedges every subsequent call behind an opaque
reqwest error. Fix: accept only a known set of revisions (falling back to the requested one),
and validate as a header value before storing.

**S7b2-6 (P3) — one malformed SSE event discards an otherwise valid response.**
`parse_sse_messages:1029` propagates the JSON parse error with `?`, so the first `data:` block
that is not valid JSON aborts the entire parse — including any later event in the same body
that carries the awaited request id. Keepalive comments are handled, but any non-JSON data
block is fatal. Fix: skip unparseable blocks and only fail when no event yields a message
matching the id.

**S7b2-7 (P3) — SSE is consumed as a one-shot bounded body, so streaming and
server-initiated messages are unsupported.** `read_bounded_http_body:986-998` buffers the whole
`text/event-stream` response — up to 8 MiB — into memory before `parse_sse_messages` sees any of
it. A server that holds the stream open for incremental progress or sends notifications outside
a request/response pair blocks until the byte cap or the request timeout, whichever comes
first. `Last-Event-ID` resumption is likewise unimplemented. This is a reasonable simplification
for one-shot `tools/call`, but it should be a documented limitation rather than an implicit one,
since the transport advertises the streaming content type.

**S7b2-8 (P3) — HTTP cancellation is structurally impossible, not just unimplemented.**
`cancel():843-847` is an honest no-op, but its comment ("dropping the blocking response cancels
an in-flight request") does not describe what happens here: the request is issued synchronously
inside `send_message(&mut self)`, and `cancel(&mut self)` also takes `&mut self`, so it cannot
be called while a request is in flight. A hung remote call runs to the full request timeout with
no operator escape. The stdio path has no such gap — `cancel():752-754` terminates the child.
Cross-ref S6b2d2-5. Fix: either move the HTTP call to a form that can be aborted (shared
cancellation token plus a client that observes it) or state plainly in the comment that HTTP
calls are uncancellable and bounded only by the timeout.

**S7b2-9 (P3) — three of the five process limits set at spawn are never enforced.**
`spawn_with_timeout:627-633` sets `limits.timeout`, `limits.stdout_bytes`, and
`limits.stderr_bytes` on the `ProcessSpec`, but `ManagedChild::spawn`
(`crates/loom_process/src/lib.rs:129-158`) only calls `ProcessIsolation::attach`, which on
Windows applies `max_processes` and `memory_bytes` and nothing else
(`:551-572`); on non-Windows targets `attach_process_isolation:608-613` is a no-op, so even the
memory cap is Windows-only. The deadline and byte-cap enforcement lives in `run_with_input`
(`:316-387`), which the MCP client does not use. Today this is harmless — the client
re-implements all three itself (`recv_timeout`, `MCP_MAX_MESSAGE_BYTES`, `BoundedStderr`) — but
it reads as defense in depth that is not there, and it is an active trap: if `ManagedChild` ever
starts honouring `limits.timeout`, every stdio MCP server is killed 60 s after spawn, because
the field carries a *per-request* value while the child is a *long-lived session*. Fix: leave
`timeout` at its default (or a deliberately large session lifetime) and comment why, rather
than assigning the request timeout to it.

**S7b2-10 (P3) — a write to a dead child loses the diagnostics the client worked to capture.**
`write_message` serializes straight into `self.stdin`, so when the server has already exited the
call surfaces as `McpError::Io` with an OS message such as "The pipe has been ended" — while the
exit code and the captured stderr tail, which `read_result:712-720` would have attached as
`ProcessExited`, are discarded. Fix: on write failure, `try_wait` the child and prefer the
`ProcessExited` variant with the stderr tail when the process is gone.

**S7b2-11 (P3) — bounded stderr keeps the head, not the tail.**
`drain_stderr:1106-1122` fills `BoundedStderr` until 1 MiB and then stops retaining
(`:573-587`), appending " [truncated]". For diagnosing a crash the interesting output is almost
always the last lines, so a server with a chatty startup banner produces a 1 MiB tail of banner
and no error. The true total is tracked correctly, so the fix is a ring buffer (or head + tail
window) rather than new bookkeeping.

**S7b2-12 (P3) — three smaller HTTP-transport gaps.** (a) The session id is extracted at `:875`
before the status check at `:893`, so a 4xx or 5xx response can still install a session id that
is then echoed on later requests. (b) Loom never sends `Origin`, even though
`build_remote_headers:966` reserves it as a managed header; the MCP specification asks clients
to send it so local servers can defend against DNS rebinding. (c) There is no retry or backoff
anywhere in either transport, so a single transient 502 or connection reset fails the tool call
outright.

**Confirms S7b1-7:** `spawn_with_timeout:626` assigns `process_spec.env = config.env.clone()`
verbatim, so the free-form env map from the server config reaches the child with no denylist for
loader-influencing variables.

Confirmed correct in this slice:

- `BoundedStderr:573-587` with `drain_stderr:1106-1122` caps retained stderr at 1 MiB while
  still tracking the true byte total, and marks the result " [truncated]" — the accounting is
  right even though the retention window is the wrong end (S7b2-11).
- `read_stdout_lines:1064-1104` frames newline-delimited JSON correctly, including flushing the
  trailing partial line at EOF, and reports EOF distinctly from an IO error.
- `read_result:698-750` maps EOF and channel disconnect to `ProcessExited` carrying the exit
  code and stderr tail, and terminates the child on both the timeout and the output-limit paths
  so no orphan survives a failed call.
- `ManagedChild` has a `Drop` that calls `terminate()` (`loom_process/src/lib.rs:176-180`), and
  the Windows job object is created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so dropping a
  client cannot leak the server process or its children.
- `configure_runtime_limits:553-556` floors both the timeout and the memory limit at 1, so a
  zero or negative configuration cannot produce an instantly-expiring or zero-memory child.
- `read_bounded_http_body:986-998` bounds with `take(limit + 1)` and then length-checks, which
  detects overflow instead of silently truncating; `bounded_error_body:1000-1008` caps error
  bodies at 2 KiB so a hostile server cannot flood the log through an error path.
- `build_remote_headers:948-984` lowercases every name, rejects empty names, and rejects a
  nine-entry managed-header denylist before validating the name and value through
  `HeaderName`/`HeaderValue`, so a credential value containing CR/LF cannot inject a header.
- `send_message:885-918` distinguishes non-success status, an empty body (legitimate for a
  notification, an error when a result is expected), and SSE versus JSON content types rather
  than guessing from the payload.
- `result_from_messages:1046-1062` matches strictly on the request id and surfaces JSON-RPC
  `error` objects as `McpError::Protocol` instead of returning a null result.
- `RedirectPolicy::none()` (`:802`) keeps a 30x from becoming a second, unvalidated fetch, and
  `connect_timeout = min(request_timeout, 15s)` (`:800-801`) keeps connection setup from
  consuming the whole request budget.

### S7c1 — Runtime-host MCP bridge: entry, config load, validation, env and header construction

Scope: `framework-packages/runtime-host/src/mcp.rs:1-550` — the `metadata.mcp` config structs,
`execute`, `load_config`, `validate_call_config`, `validate_surface_actions`,
`validate_argument_object`, `validate_identifier`, `validate_argument_name`,
`validate_binding_path`, `build_environment`, `build_headers`,
`available_credential_aliases`, `validate_header_name`, `validate_environment_name`, and
`expand_runtime_paths`.

**S7c1-1 (P2) — the Art's declared MCP server version is validated and then never enforced.**
`load_config:213-215` requires `metadata.mcp.version` to be non-empty, and
`FrameworkMcpServer.version` (`crates/loom_protocol/src/lib.rs:397`) carries the version the host
actually resolved, but `execute:102-113` compares only `id` and `package_id`. The version is
never compared, so an Art declaring `version: "1.2.0"` runs against whatever version happens to
be active — including an older one with a different tool surface, different argument names, or a
known bug. This lands directly on top of S7a-8: installation permits arbitrary downgrades and
never records an ordering, so downgrading a package silently re-points every dependent Art
without a single warning at any layer. The failure is quiet — a tool call against a mismatched
schema returns a wrong-shaped result rather than an error, and
`normalize_arguments`/`find_tool_input_schema` (see S7c2) will happily coerce against the old
schema. Fix: compare the resolved version against the declared one and fail with an actionable
message; if a range is intended, make the manifest field a range and check containment rather
than pretending an exact pin exists.

Owner: **Lane B** (claimed 2026-08-22 — this finding belonged to no batch; `src/mcp.rs` was loaned
out of Lane A's reserved path, see handoff H11). **Fixed 2026-08-22 / F13.** The second reading was
the right one: `metadata.mcp.version` is already a requirement (`^0.1`, `=2.9.0`), so the fix checks
containment in `validate_resolved_server`, rejects a resolved server reporting no version, and
re-checks the `metadata.dependencies.mcpServers` tie at load. `semver` could not be added to that
package (its `Cargo.lock` references Lane A's uncommitted `loom_security`), so the bound arithmetic
is local and deliberately incomplete — it never rejects what
`framework_process.rs:785-813` accepts. Record: `### F13` in `phase-78-lane-sync.md`.

**S7c1-2 (P3) — required and optional credential mappings can collide, and the optional one
wins.** `build_environment:393-430` inserts every `credential_env` entry and then every
`optional_credential_env` entry into the same map, so when both map the same variable name the
optional mapping overwrites the required one — with a *different* credential, silently, and only
if that optional alias happens to be granted. `build_headers:449-486` has the identical shape
for `credential_headers` / `optional_credential_headers`. Nothing rejects the collision, so a
manifest can point `API_KEY` at a strict credential in the required map and at a weaker or
attacker-chosen alias in the optional map, and the effective value depends on which grants the
operator happens to hold. Fix: reject any name appearing in both maps at validation time (and
note S7a-10 already records that the duplicate-credential path has no test).

Owner: **Lane B / F13** (co-located with S7c1-1). **Fixed 2026-08-22** in both maps — a name in the
required and the optional map is now rejected at validation time, in `build_environment` and
`build_headers` alike, with a test covering each shape.

**S7c1-3 (P3) — `{artDir}` placeholders are expanded in args and the URL but not in the
command.** `execute:147-151` maps `expand_runtime_paths` over `resolved.args`, and `:144`
expands `resolved.url`, but `:138` passes `resolved.command` through verbatim. A packaged server
whose entry point is `{artDir}/bin/server.exe` — the natural way to express a bundled binary,
and the shape the args and URL syntax invites — is spawned as that literal string and fails to
resolve. Fix: expand the command on the same path as the args, which also gives the packaged
server a way to name its own binary without an absolute path (and is a prerequisite for the
package-anchoring fix in S7b1-1).

**S7c1-4 (P3) — identifiers are validated trimmed but stored and compared untrimmed.**
`validate_identifier:329-339` trims before checking, so `" call-a"` passes validation, yet
`call.id` is stored untrimmed: it becomes the `results` map key serialized back to the Surface
(`execute:174-186`), and it is what `validate_surface_actions:274-278` puts into `call_ids` and
compares against the untrimmed strings in `action.calls`. A Surface action selecting `"call-a"`
for a call declared as `" call-a"` therefore fails with "selects unknown call". The same
trim-then-store gap applies to `server_id`, `package_id`, and `version` in `load_config:207-215`,
whose untrimmed values are then compared byte-for-byte against the resolved server at
`execute:102-113`. Fix: normalize once at deserialization and store the normalized value, so
validation and comparison operate on the same bytes.

Owner: **Lane B / F13** (co-located with S7c1-1). **Fixed 2026-08-22.** `normalize_config` trims
`server_id`, `package_id`, `version`, `toolName`, every call id and tool name, every Surface action
id and every selected call id, once, before validation. One hazard the fix had to answer: re-keying
`surface_actions` after trimming can collapse two ids into one, so that case is now an error rather
than a silent drop.

**S7c1-5 (P3) — the legacy single-call `toolName` skips the checks the multi-call path
applies.** `validate_call_config:222-235` only requires `config.tool_name` to be non-empty when
`calls` is empty, whereas `calls[].tool_name` gets a 256-byte cap and a control-character
rejection (`:251-259`). So a legacy MCP Art can declare a megabyte-long tool name, or one
containing control characters, and it is sent verbatim in `tools/call`. Fix: extract the tool
name check into a helper and call it from both branches.

Owner: **Lane B / F13** (co-located with S7c1-1). **Fixed 2026-08-22** exactly as suggested —
`validate_tool_name` is now called from the legacy branch and from the multi-call loop, so both get
the 256-byte cap and the control-character rejection.

**S7c1-6 (P3) — environment name validation has no denylist for loader-influencing
variables.** `validate_environment_name:531-542` checks only the identifier charset, so `PATH`,
`PYTHONPATH`, `NODE_OPTIONS`, `LD_PRELOAD`, and `LD_LIBRARY_PATH` all pass and reach the child
through `server.env`. This is the runtime-host half of S7b1-7 — neither layer filters, so the
combined effect is that an unsigned `mcp.server.json` (S7a-2) chooses which libraries and
interpreter options the spawned server loads. Fix the denylist once, at the `loom_mcp` layer, so
both entry points inherit it.

**S7c1-7 (P3) — header name validation is duplicated with different rules than the
transport's, so managed-header conflicts fail late.** `validate_header_name:504-529` accepts any
RFC 7230 token, while `loom_mcp::build_remote_headers:948-984` rejects a nine-entry managed
denylist (`accept`, `content-*`, `mcp-*`, `host`, `origin`, …). An Art declaring `Content-Type`
therefore passes config validation, passes `execute`, spawns/connects, and only fails when the
first request is built — as an MCP protocol error rather than a manifest error naming the
offending header. Fix: expose the denylist from `loom_mcp` and check it during
`build_headers`, so the diagnostic points at the manifest.

**S7c1-8 (P3) — no cap on the number or size of env variables and headers.**
`build_environment` and `build_headers` copy every entry from the resolved config with no count
limit and no per-value length limit, while every other dimension of this manifest is bounded
(≤8 calls, ≤32 surface actions, ≤8 selected calls, ≤32 bound arguments, ≤64-byte identifiers).
A manifest with thousands of variables, or one 10 MB value, produces either a wasted allocation
or — on Windows, where the whole environment block is capped around 32 KiB — a spawn failure
whose OS error says nothing about which manifest field caused it. Fix: bound both maps and both
value lengths, and report the offending key.

**S7c1-9 (P3) — expanding runtime placeholders into the remote URL discloses absolute host
paths.** `execute:144` runs `expand_runtime_paths` over `resolved.url`, so a manifest whose URL
contains `{artDir}`, `{cacheDir}`, or `{tempDir}` sends the operator's real filesystem layout —
including the Windows user name — to the remote endpoint, and the substituted text (backslashes,
spaces, non-ASCII) is inserted without percent-encoding, which can also produce a URL that
fails to parse. Placeholder expansion makes sense for a stdio server's arguments; for a remote
URL it is a leak with no legitimate use. Fix: drop placeholder expansion on the URL, or restrict
it to a percent-encoded allowlist.

Confirmed correct in this slice:

- `execute:102-113` verifies the resolved server id and package id against the Art's declared
  dependency *before* building any environment, headers, or process — a mismatched resolution
  cannot reach a spawn (the missing version check is S7c1-1, but these two are enforced).
- `execute:123-127` treats the transport as a closed allowlist and rejects anything that is not
  `stdio` or `streamable-http`, and `:132-134` rejects an empty stdio command rather than
  spawning an empty string.
- `validate_call_config:236-264` enforces `toolName` and `calls` as mutually exclusive, caps
  `calls` at 8, rejects duplicate ids, and rejects control characters in tool names.
- `validate_surface_actions:268-318` caps actions at 32, selected calls per action at 8, and
  bound arguments at 32, rejects a call selected twice, and rejects a selection naming a call
  that does not exist — including the `"default"` pseudo-id for the legacy single-call shape.
- `validate_binding_path:353-375` is the important one: bindings must be rooted at `payload` or
  `authoritativeState`, must have 2 to 8 segments, each ≤64 bytes and restricted to
  alphanumerics with `_`/`-`. A Surface cannot bind an MCP argument to an arbitrary location in
  the invocation, so the binding mechanism cannot be used to read fields the Art was not
  granted.
- Every config struct carries `#[serde(deny_unknown_fields)]`, so a mistyped or injected key is
  a load error instead of a silently ignored setting.
- A missing required credential produces an error naming the requested alias and the available
  aliases (`available_credential_aliases:490-502`) — names only, never values.
- `validate_argument_name:341-351` and `validate_environment_name:531-542` both reject an empty
  name, because the first-byte check fails on an exhausted iterator.
- Credential entries are inserted after the literal `env`/`headers` entries, so a manifest
  cannot shadow a credential-derived value with a literal one.
- `execute:155-156` routes `execute_tools` failures through `redact_credentials`, so a transport
  or protocol error carrying an echoed token is scrubbed before it reaches the operator.
- Header *values* are not validated here, but `HeaderValue::from_str` in
  `loom_mcp::build_remote_headers` rejects control characters downstream, so CR/LF injection
  through a manifest header value is blocked before a request is built.

### S7c2 — Runtime-host MCP bridge: call resolution, argument binding, schema normalization, execution

Scope: `framework-packages/runtime-host/src/mcp.rs:551-930` — `resolve_calls`,
`find_surface_action`, `resolve_surface_argument_bindings`, `value_at_binding_path`,
`validate_bound_value`, `value_is_within_depth`, `validate_resolved_arguments`,
`build_call_arguments`, `merge_argument_object`, `execute_tools`, `find_tool_input_schema`,
`normalize_arguments`, `normalize_argument`, `canonical_enum_value`, `schema_type_matches`,
and `redact_credentials`.

**S7c2-1 (P2) — the Surface argument-binding allowlist is bypassed on every invocation that
does not carry a `surfaceAction`.** The two branches of `resolve_calls` build arguments from
different sources. The Surface branch (`:596-611`) merges `metadata.mcp.arguments`, the
per-call arguments, and *only* the values produced by `resolve_surface_argument_bindings` — which
is exactly the point of `validate_binding_path` (S7c1, confirmed): a Surface may only feed an MCP
argument from `payload.*` or `authoritativeState.*`. The fallback branch (`:622-634` into
`build_call_arguments:748-749`) merges `request.inputs` and `request.params` **wholesale**, so any
top-level key the caller supplies becomes a tool argument. Because the Surface branch is entered
only when an invocation object is present *and* `config.surface_actions` is non-empty
(`:569`), an Art that declares `surfaceActions` still falls into the wholesale-merge branch
whenever it is executed without one — which is the normal path for a plain render or a
non-Surface execution. The binding allowlist is therefore advisory rather than enforced: it
constrains what a Surface *action* can send while leaving a completely unconstrained channel
open next to it. Fix: make the merge policy a property of the Art (declared bindings mean
declared bindings only, on every path), or apply the same key allowlist to `inputs`/`params`
when `surfaceActions` is declared.

Owner: **Lane B** (claimed 2026-08-22 — unowned by any batch; `src/mcp.rs` loaned out of Lane A's
reserved path, see handoff H11). **Fixed 2026-08-22 / F13** via the second of the two suggested
shapes. The first — bindings only, on every path — was tried and rejected: the plain path has no
invocation object to bind against, so Stock Monitor's `code` argument would have been filtered out
and the Art could not run outside a Surface action at all. So an Art that declares `surfaceActions`
now has `inputs`/`params` filtered through the union of every argument name its manifest spells out
(config arguments, per-call arguments, binding targets); an Art that declares none has stated no
policy and is unfiltered, as before. Today that changes exactly one shipped Art: Stock Monitor stops
forwarding its `interval_seconds` slider to `get_stock`. Record: `### F13` in
`phase-78-lane-sync.md`.

**S7c2-2 (P3) — a supplied `surfaceAction` is silently ignored when the Art declares none, and
the whole invocation object then leaks in as a tool argument.** `:569` filters the invocation
away when `config.surface_actions.is_empty()`, with no error, so the request proceeds down the
fallback branch. But `surfaceAction` lives in `request.inputs`/`request.params`, which that
branch merges wholesale — so the entire invocation object, including `payload` and
`authoritativeState`, is passed to the MCP server as an argument literally named
`surfaceAction`. For a remote server that is Surface state shipped off-host; for any server it is
an argument the tool never declared. Fix: reject a `surfaceAction` the Art cannot handle
instead of ignoring it, and strip the control keys before merging.

Owner: **Lane B / F13** (co-located with S7c2-1). **Half fixed 2026-08-22.** The leak is gone:
`surfaceAction` is a reserved control key and is never merged as a tool argument, so Surface state no
longer leaves the host under that name. The other half — rejecting an invocation the Art cannot
handle — is **accepted backlog**, because turning today's silent ignore into an error changes
behaviour for any caller that sends invocations to a legacy Art, which is a compatibility decision
rather than a fix. Reason recorded in `### F13` in `phase-78-lane-sync.md`, together with the
remaining fourteen P3s in the S7c1/S7c2 slices that F13 did not take.

**S7c2-3 (P3) — `disabled_params` silently deletes bound arguments the Art declared as
required.** `resolve_surface_argument_bindings:667-671` treats a binding whose every source path
is absent as a hard error — and then `:608-610` removes any name listed in
`request.disabled_params` from the merged map, including one that binding just produced. The
call proceeds with the argument missing. The two policies contradict each other: absent at the
source is fatal, disabled by the caller is silent. Fix: reject a `disabled_params` entry that
names a declared binding, or make the missing-source case non-fatal too and pick one story.

**S7c2-4 (P3) — arguments the tool does not declare are dropped silently.**
`normalize_arguments:820-828` filters out every key absent from `schema.properties` when the
schema sets `additionalProperties: false`. A misspelled argument name therefore behaves exactly
like an argument that was never provided: the tool runs with its defaults and returns a
plausible-looking wrong result. Fix: collect the dropped names and surface them (an error, or at
minimum a warning on the execution record) so a manifest typo is visible.

**S7c2-5 (P3) — normalization is top-level only and never checks `required`.**
`normalize_arguments` looks up `schema.properties[name]` for each top-level key and never
descends into object or array members, so a nested string that should be an integer is passed
through uncoerced while its top-level sibling is fixed. Nothing validates that the schema's
`required` properties are present before the call, so a missing required argument surfaces as
whatever error the server chooses. Both are defensible simplifications, but the asymmetry means
the coercion layer helps flat tool schemas and quietly does nothing for nested ones.

**S7c2-6 (P3) — a vendor-specific `search_lang` rewrite is hardcoded in the shared bridge.**
`normalize_argument:838-856` intercepts any argument named `search_lang` (case-insensitively,
for *every* MCP server) and rewrites `zh`/`zh-cn` to `zh-hans` and `zh-tw` to `zh-hant`. That is
image-search product knowledge living in the generic framework runtime host: a server whose
schema legitimately expects `zh` — and does not declare an enum, so `canonical_enum_value`
cannot rescue it — receives `zh-hans` and fails or returns wrong-language results, with nothing
in the manifest to explain why. Same class as S6b2d3-10 (hardcoded Chinese operator strings in
`loom_tool_registry`): product specifics leaking into a shared layer. Fix: express the mapping as
an argument-alias table in the Art's own manifest, or keep it inside the image-search framework.

**S7c2-7 (P3) — every execution re-runs connect + initialize + `tools/list` and then tears the
client down.** `execute_tools:775-799` opens a client, initializes, fetches the full tool list
purely to look up input schemas, issues the calls, and then calls `client.cancel()`. For stdio
that is a process spawn, handshake, and kill per Art execution; for streamable HTTP it is a new
session per execution. Nothing at this layer can reuse a session, and nothing caches the tool
list even though it is keyed by a package version that cannot change during a run. This is the
dominant cost of the MCP framework path and belongs in the S9 performance queue: cache
`tools/list` per resolved `(package_id, version)` and keep a warm client for the duration of a
render batch.

**S7c2-8 (P3) — a streamable-HTTP MCP session is never terminated.** The teardown at `:798` is
`client.cancel()`, which for stdio kills the child (correct) but for HTTP is a documented no-op
(S7b2-8). The MCP Streamable HTTP transport defines an explicit session-termination request for
exactly this moment; Loom never sends one, so remote server sessions accumulate until the server
expires them on its own. Fix: give `McpClient` a `close()` distinct from `cancel()` and send the
session termination on the HTTP path.

**S7c2-9 (P3) — the first failing call discards the results of calls that already succeeded,
and calls never overlap.** `:785-796` collects into `Result<Vec<_>, _>`, so call 4 failing throws
away the results of calls 1 through 3 — including any side effects they performed on the server,
which cannot be undone and are now invisible to the Art. With up to 8 calls per Art (S7c1
confirmed limit) and strictly sequential execution, latency is also the sum of all eight round
trips even when the calls are independent reads. Fix: return per-call outcomes so a partial
batch is reportable, and consider issuing independent calls concurrently.

**S7c2-10 (P3) — credentials are redacted from error strings but not from tool results.**
`execute:155-156` wraps `execute_tools` errors in `redact_credentials`, yet the successful result
value returned at `:791-794` flows into `McpExecution.result`/`results` completely unfiltered. MCP
servers routinely echo request context into result payloads (and embed their own error objects
inside a *successful* `tools/call` result), so an echoed token lands in the Art's execution
output and from there into whatever the Art persists or renders. The Art was granted that
credential, so this is not privilege escalation — but it moves a secret from process environment
into project data that gets baked, cached, and possibly shared. Redaction is also exact-substring
only, so a value the server returns URL-encoded, base64-encoded, or truncated passes through
untouched. Fix: run results through the same redaction, and treat encoded forms as part of the
redaction set.

**S7c2-11 (P3) — a bound value that is legitimately `null` is indistinguishable from a missing
one.** `value_at_binding_path:683` returns `None` for a present-but-null value, so binding falls
through to the next declared path and then fails with "is missing from all declared source
paths". A Surface cannot express an intentional null, and the diagnostic misdescribes what
happened. Fix: distinguish "path absent" from "value is null" and let the binding declare which
it accepts.

Confirmed correct in this slice:

- `validate_bound_value:686-701` caps each bound argument at 64 KiB encoded and depth 16, and
  `validate_resolved_arguments:718-728` caps the merged object at 256 KiB and depth 24 — and it
  is applied on **both** resolution branches (`:612` and `:627`), so neither path can hand an
  unbounded payload to the transport.
- `value_is_within_depth:703-716` tests `depth > max_depth` on entry, so recursion is bounded at
  the limit and adversarial nesting cannot overflow the stack. This is the correct shape that
  `collect_mcp_image_candidates_from_value` is missing (S6b2d3-2) — the fix for that finding can
  copy this function's structure.
- `find_surface_action:637-653` rejects a conflicting invocation supplied through both `inputs`
  and `params`, and requires the invocation to be a JSON object rather than coercing it.
- The merge order is deliberate and consistent: Art-level `arguments`, then per-call
  `arguments`, then the caller-supplied layer, so a more specific declaration always wins over a
  more general one.
- `normalize_arguments:816-821` refuses to filter undeclared keys when the schema uses
  `patternProperties`, `allOf`, `anyOf`, `oneOf`, or `$ref` — the conservative choice, since
  `additionalProperties: false` under a composed schema does not mean what a naive reader assumes.
- `schema_type_matches:907-913` handles both the string and the array form of `type`, and the
  integer/number/boolean coercions all fall back to the original value rather than substituting a
  default when parsing fails.
- `canonical_enum_value:897-905` matches case-insensitively but returns the schema's exact
  spelling, so casing differences are repaired without inventing a value the enum does not list.
- `execute_tools:788-790` verifies the tool exists in `tools/list` before calling it, producing
  "MCP server does not expose tool `x`" instead of an opaque server-side error.
- `redact_credentials:915-928` dedups and sorts by descending length, so a credential that is a
  prefix of another is not half-replaced, and empty values are skipped rather than replacing every
  character boundary in the message.
- `:778-799` captures the closure result before `client.cancel()`, so teardown runs on both the
  success and the failure path — no leaked child process on error.

### S8a1 — Image-search MCP server package runtime

Scope: `mcp-server-packages/image-search/runtime/image-search-mcp.ps1` (364 lines) and its
`mcp-server-packages/image-search/mcp.server.json` manifest.

**S8a1-1 (P2) — the Brave API key is sent to whatever endpoint the package manifest names.**
The script's only guard on its `-Endpoint` parameter is `[ValidatePattern('^https?://')]`
(`:3-4`), and the key is attached as `X-Subscription-Token` with
`TryAddWithoutValidation` (`:160`). `entry.args` is copied into the spawn command with no
validation and no length bound (S7a-7), from a manifest that carries no signature and a
self-declared publisher (S7a-2), so an installed package can set
`-Endpoint http://collector.example/` and the operator's Brave subscription key leaves the
machine in cleartext on the first search. The pattern also permits plain `http` for the real
endpoint. Fix: remove the parameter (the endpoint is not a per-deployment concern) or pin the
host and require `https`.

**S8a1-2 (P3) — a query string on the endpoint can neutralize `safesearch=strict`.**
`New-SearchUri:127-133` *appends* `q`, `count`, and `safesearch=strict` to any query already
present on `$Endpoint` instead of replacing conflicting keys. Brave, like most APIs, honours the
first occurrence of a repeated parameter, so an endpoint ending in `?safesearch=off` wins and the
strict setting the script believes it is enforcing has no effect — with `count` duplicated the
same way. Fix: parse the existing query and overwrite the keys the script owns.

**S8a1-3 (P3) — the 8 MiB response cap exceeds what the host PowerShell can actually parse.**
The script caps the Brave response at 8 MiB (`:10`, `:170-183`) and then calls
`ConvertFrom-Json` (`:190`). A `.ps1` entry point is spawned through `powershell.exe`
(S7b1-5) — Windows PowerShell 5.1, whose `ConvertFrom-Json` is backed by `JavaScriptSerializer`
with a default `MaxJsonLength` around 2 MB. A response between 2 MB and 8 MiB therefore passes
the script's own limit and dies inside the parser with an error that says nothing about size.
Fix: lower the cap to match the parser, or declare a `pwsh` 7 requirement in the manifest and
have the host honour it (today nothing in `mcp.server.json` can express a required host version).

**S8a1-4 (P3) — "no results" is reported as a protocol error instead of a tool error.**
`Invoke-ToolCall:294-296` throws when Brave returns nothing usable, and the top-level handler
turns every throw into a JSON-RPC error with code `-32000` (`:361-362`). MCP reserves protocol
errors for protocol failures and expects a failed *tool* to answer with a result carrying
`isError: true` — which this script already knows how to emit (`:308`). Loom maps the error to
`McpError::Protocol`, so the Art cannot tell "the server is broken" from "the search found
nothing", which is exactly the distinction the image-search candidate/fallback logic needs
(cross-ref S6b2d3). Fix: return `isError: true` with an explanatory `content` block for tool-level
failures and keep `-32000` for genuine faults.

**S8a1-5 (P3) — the server replies to notifications.** Only `notifications/initialized` is
skipped (`:333-335`). Any other id-less message falls through to the `default` arm or the `catch`
and is answered with an error carrying `id: null` (`:356-358`, `:361-362`), which JSON-RPC 2.0
forbids for notifications; a `tools/call` sent as a notification is executed *and* answered. Fix:
treat a missing `id` as a notification everywhere — never respond, and never execute a method
that has side effects.

**S8a1-6 (P3) — `protocolVersion` is answered without reading the request.** `:339-345` returns a
hardcoded `2024-11-05` and never inspects the client's requested `protocolVersion` or
capabilities. It agrees with Loom's current stdio pin (S7b1-3) by coincidence rather than by
negotiation, so the day either side moves the two disagree silently — and Loom's stdio client
does not check the response either (S7b2-4), so nothing anywhere would notice.

**S8a1-7 (P3) — the system proxy is disabled unconditionally.** `$handler.UseProxy = $false`
(`:150`) means the server cannot reach Brave from a machine whose only egress is a corporate
proxy, and its traffic is invisible to any host-level inspection an operator has configured. If
the intent was to avoid a misconfigured proxy breaking the call, that belongs behind a parameter
with a documented default.

**S8a1-8 (P3) — stdin has no line-length bound.** `:312` reads whole lines with
`[Console]::In.ReadLine()` and buffers each one entirely, while the Loom side caps inbound
messages at 8 MiB (`MCP_MAX_MESSAGE_BYTES`). Over a private pipe from the daemon this is low
risk, but it is the one unbounded input left in the script, and the asymmetry means the two ends
disagree about what an acceptable message is.

**S8a1-9 (P3) — exception text is echoed verbatim to the client.** `:362` returns
`$_.Exception.Message`, and for DNS or TLS failures the .NET message embeds the full request
URI — which contains the user's search terms. The query also appears in `structuredContent`
(`:298`). Neither is a secret leak, but it does mean search text lands in whatever the client
logs on failure; worth a deliberate decision rather than an accident.

Confirmed correct in this slice:

- The response is bounded twice: the declared `Content-Length` is checked (`:170-173`) *and* the
  streaming accumulation is checked against the same cap (`:179-181`), so a lying or absent
  `Content-Length` cannot get past the limit.
- `ResponseHeadersRead` plus a 45 s `HttpClient.Timeout` (`:154`, `:162-165`) means neither
  headers nor body can hang the server indefinitely.
- `Invoke-ToolCall:274-278` rejects any argument other than `query`/`count`, so the server
  enforces its own `additionalProperties: false` rather than trusting the client to filter — which
  matters because Loom's `normalize_arguments` drops undeclared keys silently (S7c2-4).
- The ordering of checks saves the null-`arguments` case under `Set-StrictMode -Version Latest`:
  the "query is required" throw at `:268-270` runs before `$arguments.PSObject.Properties` is
  touched at `:274`.
- `ConvertTo-HttpUrl:31-45` requires an absolute URI whose scheme is `http` or `https`, so a
  `file:`, `data:`, or `javascript:` URL in a Brave result never reaches Loom as a candidate. This
  is precisely the check the Loom-side consumer is missing (S6b2d3-5) — the server is stricter
  than its client.
- Candidate mapping falls back `url` → `properties.url` → `thumbnail.src` and drops a result with
  no usable URL (`:68-78`) instead of emitting a candidate with an empty `imageUrl`.
- `count` is bounded to 1..6 in both the advertised schema (`:249`) and the handler (`:286-288`),
  and a non-integer is rejected rather than coerced.
- UTF-8 without BOM is set on both console streams (`:12-13`) and a leading BOM is stripped from
  every input line (`:313`) — the classic failure mode for PowerShell-hosted stdio JSON-RPC is
  handled deliberately.
- Every disposable — request, response, stream, memory stream, client, handler — is released in a
  `finally` block (`:161-200`), including on the throw paths.
- The manifest side is sound: `validate_manifest:292-295` runs `entry.command` through
  `safe_relative_path` and requires the file to exist in the staging tree, and
  `config_from_manifest:201-204` joins it onto the install directory, so the shipped relative
  `runtime/image-search-mcp.ps1` becomes an absolute path anchored inside the package and a
  manifest cannot point its entry point outside its own tree.

### S8a2a — Stock-api MCP server package: constants, test fixture hook, parsers, aggregation

Scope: `mcp-server-packages/stock-api/runtime/stock-api-entry.js:1-500`, with call-site evidence
from `:815` and `:979`.

**S8a2a-1 (P3) — the shipped runtime contains an environment-activated global `fetch` override.**
`configureLoopbackFixture` (`:120-145`) runs unconditionally at startup (`:979`) and, when
`LOOM_STOCK_API_TEST_BASE_URL` is set, replaces `globalThis.fetch` so every outbound request is
rewritten to `http://127.0.0.1:<port>/proxy?url=<original>` with the caller's `init` — headers
included — forwarded intact. The URL validation is genuinely tight (see below), so this is not a
remote exfiltration path, but the switch ships inside the production package and the runtime-host
environment builder applies no denylist to manifest-declared variable names (S7c1-6), so an
installed Art can turn it on. Fix: gate the fixture behind a build-time flag or move it into a
separate test entry point that the manifest does not reference.

**S8a2a-2 (P3) — activating the fixture changes the production code path, so fixture-based tests
never exercise the primary provider.** `:815` skips `executeStableMarketSeries` entirely whenever
`loopbackFixtureEnabled` is true, because the vendored dist path does not route through
`globalThis.fetch`. Every loopback test therefore covers only the direct-Eastmoney fallback, and
the primary path the package prefers in production has no coverage from this harness. Queue for
S9.

**S8a2a-3 (P3) — `parseKline` trusts field position with no arity check.** `:246-258` destructures
six comma-separated positions from a row whose shape is pinned only by the `fields2=f51,...,f56`
query parameter (`:493`, `:807`). Too *few* fields fails safely — `Number(undefined)` is `NaN` and
the row is rejected at `:257` — but a reordered or inserted upstream field yields
plausible-but-wrong OHLC that passes every check. Fix: assert the split length before mapping.

**S8a2a-4 (P3) — provider percentages are divided by 100 with no plausibility check, even though
the code already computes an independent value.** `parseQuote:291-293` and
`parseXueqiuRealtime:335-337` both use `providerPercent / 100` and fall back to
`now / yesterday - 1`. Since the derived value is right there, the cheap fix is to prefer the
provider figure only when the two agree within a tolerance. As written, a change in either
provider's scale ships a 100×-wrong percentage straight to the Stock Monitor Surface.

**S8a2a-5 (P3) — `keepLatestTradingDays` silently depends on provider row order.** `:383-387`
builds a `Set` of distinct dates in insertion order and takes `.slice(-days)`; rows are never
sorted. Descending input would select the *oldest* days rather than the newest. It is called for
`minute` and `five-day` on both the Sina path (`:545`) and the Eastmoney path (`:836-837`). Fix:
sort the distinct dates before slicing.

**S8a2a-6 (P3) — intraday aggregation groups by array index rather than by clock time.**
`aggregateTwoHourRows:393-394` and `aggregateRows:430-431` step `index += groupSize` through a
day's rows, so a single missing bar shifts every later boundary in that day. This bites hardest on
`minute-120`, which is served by fetching 60-minute bars — `PERIOD_CODES` maps both `minute-120`
and `minute-60` to `"60"` (`:28-29`) — and pairing them, so a truncated session produces two-hour
candles that do not correspond to two-hour windows. Fix: bucket by the bar's own timestamp.

**S8a2a-7 (P3) — `aggregateTwoHourRows` duplicates `aggregateRows`.** `:389-408` and `:426-445`
are the same algorithm; the only difference is a hardcoded `source: "eastmoney"` versus the
`source` parameter. Both are live (`:839` and `:546`). Collapse the first into
`aggregateRows(rows, 2, "eastmoney")` so the index-vs-time fix above only has to be made once.

**S8a2a-8 (P3, performance) — all three aggregators are quadratic in row count.** For every
distinct date they re-`filter` the entire array (`:392`, `:429`); only `aggregateCalendarRows`
uses a `Map`. With `count` bounded at 2000 that is up to roughly four million comparisons per
cache-missing request. A single grouping pass is a few lines. → S9. **Reassigned 2026-08-22 to Lane B
with the rest of `mcp-server-packages/stock-api/**`.**

**S8a2a-9 (P3, performance) — each result is deep-copied four times and sent twice.**
`rememberSuccess:186` clones on write, `readRememberedSuccess:202` clones on read, and
`createToolResult:172-174` clones once more and then emits the same payload twice on the wire:
pretty-printed into `content[0].text` via `JSON.stringify(value, null, 2)` *and* verbatim as
`structuredContent`. For a 2000-row series that roughly doubles the bytes crossing the pipe for no
consumer benefit. Fix: drop the indentation, and consider a summary in `content` with the data
only in `structuredContent`. → S9. **Reassigned 2026-08-22 to Lane B with the rest of
`mcp-server-packages/stock-api/**`.**

**S8a2a-10 (P3) — the `stale` flag can never be true.** `markFreshResult:220` sets
`stale = false`, `markCachedResult:231` also sets `stale = false`, and
`readRememberedSuccess:196-199` evicts anything past its TTL, so no code path emits `stale: true`.
Either implement the behaviour the field implies — serve an expired entry as stale when upstream
fails, which is exactly what a market widget wants — or remove it, because a consumer branching on
it today is branching on a constant.

Confirmed correct in this slice:

- The loopback fixture's own validation is strict: `http` scheme only, hostname drawn from an
  explicit loopback set, and any userinfo, query string, or fragment rejected (`:126-135`), so the
  rewrite target cannot be aimed off-box even though the switch itself is reachable.
- `optionalCount:156-162` enforces exactly the 1..2000 bound the schema advertises, rejects
  non-numbers and non-finite values instead of coercing them, and floors rather than rounds.
- `adjustCode:164-169` throws on anything outside its enum, so an argument that somehow bypassed
  schema validation cannot silently degrade to `fqt=0`.
- Both tool schemas declare `additionalProperties: false` with explicit `required` and `enum` sets
  (`:69-84`, `:89-117`), and the enums are built from the same frozen constants the handlers switch
  on (`LIVE_SOURCES`, `MARKET_PERIODS`), so the advertised contract cannot drift from the
  implementation.
- `quoteNumber:271-275` maps Eastmoney's `"-"` placeholder to null rather than `NaN`, and every
  numeric field in every parser goes through it.
- `parseQuote:282` and `parseXueqiuRealtime:324` reject a quote whose price is not positive instead
  of emitting a zero-priced row, and `parseXueqiuOrderBook:367` rejects a book with neither side
  populated.
- `xueqiuTimestamp:313-319` bounds the epoch value before constructing a `Date`, so a garbage
  timestamp yields null instead of an `Invalid Date` serializing to null further downstream.
- `parseXueqiuOrderBookSide:354-359` reads a fixed ten levels and skips any level without a
  positive price, so a partial book cannot produce phantom levels.
- Every shared table — period lists, host lists, live-source list, and both tool definitions — is
  `Object.freeze`d (`:5`, `:20`, `:35`, `:42`, `:50`, `:66`, `:86`), so one request cannot mutate
  configuration another request will read.
- `rememberSuccess:184-190` is a correct LRU: delete-then-set to refresh insertion order, with
  front eviction while over `SUCCESS_CACHE_LIMIT`.

### S8a2b — Stock-api MCP server package: fetch layer, host rotation, tool dispatch, JSON-RPC loop

Scope: `mcp-server-packages/stock-api/runtime/stock-api-entry.js:501-985`.

**S8a2b-1 (P2) — an oversized request permanently desynchronizes the server's framing.** The line
reader caps a request at `MAX_REQUEST_BYTES` (1 MiB) in two places. The second (`:947-955`) is
safe: it has a complete line and discards exactly that line. The first (`:934-942`) is not — when
the buffer holds no newline yet and exceeds the cap, it sets `buffer = ""` and returns, with no
state recording that the *rest* of that message is still arriving. The tail then accumulates as
though it were a fresh message, and at the next newline the fragment is `JSON.parse`d, fails, and
returns `-32603`; every subsequent message is offset by whatever remained. The server never
resynchronizes and stays useless until restarted. This is reachable rather than theoretical: Loom
accepts messages up to `MCP_MAX_MESSAGE_BYTES` (8 MiB), eight times this server's limit, and the
runtime-host bridge merges `request.inputs` into `tools/call` arguments wholesale with no size
bound (S7c2-1, S7c1-8), so a Surface carrying a large state object produces exactly this request.
Fix: on overflow, set a "discard until newline" flag and drop bytes until the framing boundary is
reached; also report `-32600` consistently in both paths rather than `-32603` from the parse
failure.

**S8a2b-2 (P3) — the server's own retry budget can exceed the client's request timeout.** For a US
code, `marketSecidCandidates` yields three candidates (`:266-267`), and `executeMarketSeries` runs
`fetchFromHosts` once per candidate (`:822-831`), each with its own
`HOST_OPERATION_TIMEOUT_MILLIS` deadline of 18 s — 54 s — after already spending up to ~16 s in
`executeStableMarketSeries` (an upstream call plus a Sina fallback at 8 s each). Roughly 70 s worst
case against Loom's `DEFAULT_MCP_REQUEST_TIMEOUT_SECONDS` of 60. On total upstream failure the Art
therefore sees a client-side timeout instead of the clean tool error this code carefully
constructs, and `read_result` has no overall deadline of its own to make that failure crisp
(S7b2-2). Fix: give the whole tool call one budget derived from the client's timeout and share it
across candidates.

**S8a2b-3 (P3) — hand-maintained version constants misreport what is actually running.**
`handleWrapperRequest:874` overwrites the vendored server's `serverInfo.version` with
`WRAPPER_VERSION` ("2.9.0", `:3`), so the real version of `vendor/stock-api/dist` is not observable
anywhere; and `PYSNOWBALL_VERSION` ("0.1.8", `:4`) is reported as `pysnowballVersion` in every
order-book result (`:768`) without ever being read from the vendored package. Both drift silently
the moment `vendor/` is refreshed. Fix: report both versions — wrapper and upstream — and read the
vendored versions from their package metadata.

**S8a2b-4 (P3) — advertised schema and executing handler can diverge for the same tool name.**
`tools/list` appends the wrapper's two tools only when upstream does not already expose that name
(`:881-883`), but `tools/call` intercepts by name unconditionally (`:902`, `:913`). If the vendored
server ever ships its own `get_market_series`, clients see upstream's schema while the wrapper's
handler still executes the call. Fix: either override on conflict, or stop intercepting a name
upstream owns.

**S8a2b-5 (P3) — one argument silently selects between two entirely different implementations.**
`get_stock` and `get_klines` are handled by the wrapper only when `arguments.source ===
"eastmoney"` (`:892`, `:914`) and otherwise pass through to the vendored server. The two paths
differ in result shape, caching, and error semantics, and neither tool description mentions it, so
a caller changing `source` gets a different contract rather than a different data provider.

**S8a2b-6 (P3) — `count` is silently overridden for two periods.** `:799-800` forces `count` to
2000 for `five-day` and 800 for `minute`, discarding an explicit caller value that
`MARKET_SERIES_TOOL` advertises as an honoured 1..2000 parameter (`:103-107`). Fix: clamp rather
than override, or document the fixed windows in the schema description.

**S8a2b-7 (P3) — a degraded upstream result overwrites a better cached one.** The cache is only
consulted when the fetch produced *zero* rows (`:842-849`); a partial result — three rows where 240
were requested — is treated as success, published, and written over a complete cached entry by the
unconditional `rememberSuccess` at `:866`. Fix: only replace a cached entry when the new result is
at least as complete, and treat a suspiciously short series as a fallback-worthy failure.

**S8a2b-8 (P3) — the three caches disagree about what belongs in a key.**
`executeMarketSeries:804` keys on `code|period|count|adjust` and omits `source`;
`executeOrderBook:774` includes `requestedSource`; `executeMarketQuote:720` keys on `code` alone.
All three are correct today only because the non-order-book handlers reject any `source` other than
`"eastmoney"`, so the omission is latent rather than live — it becomes a wrong-provider cache hit
the first time either schema's `source` enum grows.

**S8a2b-9 (P3) — each inbound line is handled in a detached async IIFE with no concurrency
bound.** `:958-971` fires `void (async () => { ... })()` per line and never tracks the promise, so
a burst of requests starts an unbounded number of concurrent upstream fetches, responses may be
written in any order, and nothing applies backpressure. Loom's stdio client happens to serialize
requests (`send_message` takes `&mut self`), so this is dormant today, but it is dormant by the
client's accident rather than by the server's design. Related: `:929-976` registers no `end` or
`error` handler on stdin, so at EOF the process drops in-flight work with no response — harmless
only because `ManagedChild::Drop` kills the tree anyway.

**S8a2b-10 (P3) — error responses are sent for messages that may be notifications.** The catch at
`:964-970` always writes a response with `id: request?.id ?? null`, and the dispatch guards
(`:872`, `:877`, `:888`) require `request.id !== undefined` only for the intercepted methods — a
notification falls through to `upstreamHandle(request)` at `:926` and whatever it returns is
written back (`:963`). Same JSON-RPC violation as S8a1-5, in the other packaged server. Fix: treat
a missing `id` as a notification at the top of the handler and never respond.

**S8a2b-11 (P3) — the internal upstream call hardcodes `id: 0` and never checks the response
id.** `callUpstreamKlines:509-517` synthesizes a `tools/call` with `id: 0` and reads
`response?.result` without verifying the id came back (`:518-526`). It is an in-process function
call today so no correlation is needed, but the id is also the one field that would matter if
`handleMcpRequest` ever became asynchronous or multiplexed, and `0` is exactly the value most
likely to collide with a real client request.

**S8a2b-12 (P3) — whole-body buffering multiplies peak memory, and credential presence is
disclosed.** `fetchJson:580-600` accumulates every chunk, copies them into one `Uint8Array`,
decodes to a string, then parses — roughly four times the body resident at peak, with a 5 MiB cap,
which combined with the unbounded concurrency of S8a2b-9 scales linearly in concurrent requests
against `DEFAULT_MCP_MEMORY_LIMIT_BYTES` (512 MiB). Separately, `providerMetadata` reports
`pysnowballTokenConfigured: Boolean(pysnowballCookie())` in every order-book result (`:769`),
disclosing whether an operator credential exists to any consumer of the tool output — and Loom
redacts credentials in errors only, never in results (S7c2-10). → S9 for the memory half.
**Reassigned 2026-08-22: this slice's scope is `mcp-server-packages/stock-api/**`, which belongs to
Lane B, so the memory half goes to that lane's queue rather than staying in Lane A's S9 batch.**

Confirmed correct in this slice:

- `fetchJson` bounds the response twice — the declared `content-length` (`:575-578`) and the
  streaming accumulation (`:587-590`) — cancels the reader on overflow, and clears its
  `AbortController` timer in a `finally` (`:563-565`, `:604-606`).
- A malformed body yields a labeled provider error rather than leaking the raw parse exception
  (`:599-603`).
- `fetchFromHosts` copies the URL per attempt and mutates only `hostname` (`:675-676`), so path and
  query are untouched and the host always comes from a frozen allowlist (`KLINE_HOSTS`,
  `QUOTE_HOSTS`).
- The retry loop enforces a real wall-clock deadline checked before every attempt (`:670-673`),
  passes the smaller of the per-request and remaining budgets to `fetchJson` (`:677`), and clamps
  its linear backoff to the remaining budget (`:683-687`).
- `pysnowballCookie:612` rejects CR/LF in the token, so an env-supplied credential cannot inject
  additional headers, and the token is only ever attached to the two hardcoded `https` Xueqiu
  constants.
- `fetchXueqiuLike:633-637` validates the provider's `error_code` envelope instead of treating
  HTTP 200 as success.
- `executeOrderBook:740-743` uses `Promise.allSettled`, so a failure on one leg still yields the
  other, and when both fail with nothing cached it re-throws the real upstream reason (`:785-787`)
  rather than a generic message.
- `executeOrderBook:749` matches the realtime row by `symbol` rather than trusting array position.
- Every handler validates `source` and `period` against the frozen enums before use (`:696-698`,
  `:736-738`, `:795-798`), so an argument that bypassed schema validation still cannot reach a URL.
- `:835` sorts rows by date before slicing — which is precisely the fix S8a2a-5 asks for inside
  `keepLatestTradingDays`; note the Sina path at `:545` calls it without that compensation.
- The line reader enforces a per-line byte cap with a JSON-RPC error rather than silent truncation
  (`:947-955`) — the framing bug in S8a2b-1 is in the pre-newline branch only.
- Startup wraps both the fixture and the vendored `require` in try/catch, writes the message to
  stderr, and sets a non-zero `exitCode` (`:978-985`), so a broken vendor bundle fails
  diagnosably instead of hanging. This is also the one case where `BoundedStderr` keeping the
  *head* rather than the tail (S7b2-11) is the right behaviour, since the failure is the first
  thing written.

### S8b1 — Image-search Art runtime

Scope: `art-packages/samples/image-search/runtime/main.ps1` (358 lines).

**S8b1-1 (P2) — the Art is a confused deputy: it fetches whatever URL the MCP server names and
returns the response body to the caller.** `Convert-ImageLocationToDataUrl:199-264` performs an
unrestricted `GetAsync` on `$candidate.imageUrl`, which comes entirely from the MCP tool result
harvested by `Add-McpImageCandidates`, and base64-encodes the response into the Art's output. There
is no host allowlist, no loopback or private-range rejection, and `AllowAutoRedirect = $true`
(`:210`) lets the final destination differ from the URL that passed validation. The only filter is
`Test-ImageLocation`, which requires an image-looking extension (`:36-39`) — trivially satisfied by
`http://127.0.0.1:8787/anything.png`. So a third-party MCP server, which installs without any
signature check (S7a-2), can make the Art read from services reachable only from the user's machine
and hand the bytes back as an "image". Fix: apply the same outbound policy S7b2-1 asks for on the
Loom side — reject loopback, RFC1918, and link-local targets, and re-validate after each redirect.

**S8b1-2 (P2) — `Add-McpImageCandidates` recurses without any bound.** `:116-169` descends into
every array element (`:153-155`) and every property (`:163-168`) of MCP-supplied JSON, and — the
multiplier — parses any string that starts with `{` or `[` and recurses into the result
(`:141-147`), so nested JSON-in-string chains stack depth arbitrarily. There is no depth limit, no
visited-node budget, and no cap on the candidate list. A deeply nested tool result exhausts the
PowerShell call stack and the Art dies with a stack overflow rather than a structured error. This is
the art-side twin of S6b2d3-2 on the Rust side, and `value_is_within_depth`
(`framework-packages/runtime-host/src/mcp.rs:703-716`) is the fix shape to mirror here.

**S8b1-3 (P3) — `data:` URLs bypass every download control.** `Test-ImageLocation:29-31` accepts
any `data:image/...` string and `Convert-ImageLocationToDataUrl:205-207` returns it verbatim before
the 32 MiB cap, the timeout, and the content-type handling can apply, so an MCP-supplied payload
becomes the Art's `output_base64` unexamined. `svg` is in both the accepted-extension list (`:37`)
and the MIME table (`:194`), so the pipeline's implicit assumption that `output_base64` holds a
raster image is not enforced anywhere. Loom's 8 MiB inbound message cap bounds the size, so this is
a trust issue rather than a resource one. Fix: cap the decoded length explicitly and restrict the
accepted set to raster formats unless something downstream actually needs SVG.

**S8b1-4 (P3) — downloaded bytes are never checked against the MIME type claimed for them.**
`Get-ImageMimeType:171-197` derives the type from the URL extension or the response
`Content-Type`, and `:257-258` base64-encodes whatever arrived, so an HTML error page served with
`Content-Type: image/png` and HTTP 200 becomes `data:image/png;base64,<html>` and fails only when
something tries to decode it. Fix: sniff the leading magic bytes and reject a mismatch.

**S8b1-5 (P3) — the `Referer` sent upstream is MCP-controlled.** `:291-293` passes
`$candidate.sourcePageUrl` — a value harvested from the tool result — into
`DefaultRequestHeaders.Referrer` (`:215-221`), so the attacker choosing the target host also
chooses the referrer presented to it. Minor alone; it belongs to the same confused-deputy picture as
S8b1-1.

**S8b1-6 (P3) — candidates are truncated before downloading, so recoverable failures reduce the
result count.** `:289` takes the first `$requestedCount` raw candidates and then downloads them;
each failure `continue`s (`:298`, `:306`). A request for three images where two of the first three
URLs are dead returns one image even when the harvested list held twenty usable ones. Fix: iterate
the full list until `$requestedCount` downloads have succeeded.

**S8b1-7 (P3, performance) — base64 payloads are held and serialized several times over.** Each
candidate stores the same data URL in both `thumbnail` and `data` (`:318-319`), the selected one is
copied again into `output_base64` and `content[0].data` (`:344-350`), and `Write-SuccessResponse`
serializes the candidate list *and* the output. With the 32 MiB per-image cap and up to six
candidates, base64 expansion alone puts roughly 260 MiB of strings in play before serialization
doubles the selected image. Fix: store each payload once and reference it. → S9. **Closed by F11p
2026-08-22.**

**S8b1-8 (P3) — `result_index` is silently coerced.** `:336-341` clamps an out-of-range index and
`:335-336` falls back to 0 when `TryParse` fails, so asking for index 5 among two candidates
succeeds and quietly returns index 1. `selectedCandidate` (`:347`) makes it recoverable, but the
caller has no signal that its selection was not honoured.

**S8b1-9 (P3) — the `url` field is overloaded three ways in one function.**
`Convert-ToMcpImageCandidate` treats `url` as an `imageUrl` fallback inside `properties` (`:75`), as
an `imageUrl` fallback at top level when width or height is present (`:88`), and as the
`sourcePageUrl` fallback (`:102`) — then nulls `sourcePageUrl` when the two coincide (`:103-105`).
The outcome is right, but only because of the order the three rules happen to run in; a result
carrying only `url` and `width` depends on all three interacting correctly. Fix: decide what `url`
means per shape and branch once.

**S8b1-10 (P3) — the provider name is hardcoded in a provider-agnostic runtime.** Candidate ids are
built as `"brave-search-$($index + 1)"` (`:311`) even though the MCP server behind this Art is
swappable, and the harvesting code deliberately handles many result shapes. Same class as S7c2-6 and
S6b2d3-10: vendor specifics leaking into a generic layer. Related: the runtime dot-sources
`runtime/common.ps1` at `:1`, a file that does not exist in the source tree — it is copied in from
`art-packages/shared/image-runtime-common.ps1` by
`scripts/Build-LoomSampleArtPackages.ps1:61` — so the dependency is implicit, undeclared in the
manifest, and the source tree cannot be executed as-is.

**S8b1-11 (P3) — download failures are swallowed with no diagnostic.** Both `catch` blocks at
`:295-307` discard the reason, and the terminal message "returned candidates, but none could be
downloaded" (`:327`) names neither a URL nor a status code. The final handler reports only
`$_.Exception.Message` (`:357`). Fix: accumulate per-candidate failure reasons and include them in
the error payload.

Confirmed correct in this slice:

- `Test-ImageLocation:22-40` requires `http`, `https`, or `data:image/` and an image extension, so
  `file:`, `javascript:`, and UNC paths are rejected — precisely the check the Rust-side collector
  is missing (S6b2d3-5). It is the URL *scheme* filtering that is right here; the missing host
  policy is S8b1-1.
- The download is bounded twice — declared `ContentLength` (`:231-234`) and streaming accumulation
  (`:240-242`), both at 32 MiB — with `ResponseHeadersRead` (`:225`) so the cap applies before the
  body is buffered.
- A 30 s client timeout is set (`:213`), the stream and memory stream are disposed in an inner
  `finally` (`:247-250`), and the client and handler in an outer one (`:260-263`), including on
  every throw path.
- A non-2xx response throws with the status code (`:227-229`) instead of base64-encoding an error
  page.
- `Get-McpPropertyValue:4-20` null-guards every property read, so a missing field cannot fault the
  strict-mode parser regardless of how irregular the MCP result is.
- The recursion skips re-descending into the fields it already consumed (`:164-166`), so one node
  cannot yield the same candidate twice.
- `Seen` is `OrdinalIgnoreCase` (`:282`), collapsing case-varying duplicate URLs.
- The thumbnail retry only fires when the thumbnail differs from the URL that just failed
  (`:296-299`), so a failing URL is not fetched twice.
- `requestedCount` is clamped to 1..6 (`:272-275`), matching the bound the MCP server's own schema
  advertises.
- The entire body is wrapped so any failure becomes a structured `Write-ErrorResponse` (`:356-358`)
  rather than a PowerShell error record on stdout, which would otherwise corrupt the Art's response
  framing.

### S8b2 — Shared PowerShell image helper

`art-packages/shared/image-runtime-common.ps1` (371 lines) is the only shared library in the Art
sample set. `scripts/Build-LoomSampleArtPackages.ps1:61` and
`scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1:1032` copy it into every built package as
`runtime/common.ps1`, which is the file each sample dot-sources at its first line. It supplies four
groups of helpers: request accessors (`:5-88`), input resolution (`:90-144`), GDI+ image operations
(`:146-243`), and response construction (`:245-371`). Three samples consume it — `image-blend`,
`remove-bg`, `image-compress` — while `image-search` uses only the request/response halves.

S6b2c3-2 already covered `New-ImageOutput:245-266` emitting `output_base64` beside
`content[0].data`; that duplication is not repeated here.

**S8b2-1 (P2) — resolved input paths are confined to nothing, so an Art invocation can name any
readable image on the machine or a remote UNC share.** `Resolve-ImagePath:127-129` accepts a string
if `Test-Path -LiteralPath $text -PathType Leaf` succeeds and returns
`[System.IO.Path]::GetFullPath($text)` — no comparison against `$WorkRoot`, the art directory, the
cache directory, or any allowlisted root, and no check that the file is inside the sandbox the host
built at `framework_process.rs:258-263`. The value it resolves comes from `request.inputs`
(`Get-RequestInputValue:54-73`), which per S7c1-8 and S7c2-1 is merged from tool-call arguments and
Surface action payloads without an allowlist. Two consequences follow:

- Arbitrary local image read. `remove-bg/runtime/main.ps1:7` and `image-compress/runtime/main.ps1:7`
  derive their output from the resolved input, and `New-ImageOutput` base64-encodes that output into
  the response, so pointing `input` at any GDI+-decodable file (PNG, JPEG, GIF, BMP, TIFF, ICO,
  WMF/EMF) under the user's account returns its contents to whoever supplied the input.
- Outbound SMB with implicit authentication. `:119-126` converts a `file://` URI through
  `[System.Uri]::LocalPath`, so `file://attacker.example/share/x.png` becomes
  `\\attacker.example\share\x.png`, and the `Test-Path` at `:127` is itself enough to make Windows
  open an SMB session and offer the user's NTLM credentials. This escapes the HTTP-only framing of
  S8b1-1 and S7b2-1 — an outbound-URL policy that only inspects `http`/`https` will not see it.

Fix: resolve the candidate path, canonicalize it, and require that it sit under `$WorkRoot`, the art
package directory, or an explicitly granted root; reject UNC paths (`[System.Uri]::IsUnc`) and
reject `file://` hosts other than empty/`localhost` before any `Test-Path` runs.

**S8b2-2 (P2, performance) — `Blend-Bitmaps` blends per pixel through `GetPixel`/`SetPixel`, which
exceeds the framework process timeout on ordinary photographs.** `:210-219` runs a nested
PowerShell loop over `Source.Height × Source.Width`, and each iteration performs two `GetPixel`
calls, four `[Math]::Round` calls, a `Color::FromArgb`, and a `SetPixel`. Each pixel accessor locks
and unlocks the bitmap's bits individually, and each loop iteration additionally pays PowerShell
interpreter and interop overhead. At 1920×1080 that is 2,073,600 iterations and roughly 6.2 million
GDI+ interop calls; at 4000×3000 it is 12 million iterations and 36 million calls, which lands well
past `DEFAULT_FRAMEWORK_PROCESS_TIMEOUT` (120 s, `framework_process.rs:24`) — the host kills the
process and the user sees a timeout rather than a slow blend. `Resize-BitmapArgb:171-193` already
demonstrates the right idiom: one `DrawImage` call. Fix: either blend with a single
`DrawImage` pass using a `ColorMatrix` alpha on the reference layer, or `LockBits` both bitmaps and
walk one `byte[]` with `Marshal.Copy`. Both remove the per-pixel interop entirely.

**S8b2-3 (P3) — dot-sourcing the helper mutates the caller's scope and loads GDI+
unconditionally.** `:1` sets `$ErrorActionPreference = "Stop"` and `:3` runs
`Add-Type -AssemblyName System.Drawing` at file scope, so every Art that dot-sources
`common.ps1` — including ones that never touch a bitmap, such as `image-search` — pays the
`System.Drawing` assembly load on every process start, and any caller that deliberately chose a
different error preference has it silently overwritten (dot-sourcing runs in the caller's scope, so
the assignment is not scoped to the library). Neither the library nor the samples set
`Set-StrictMode`, unlike the packaged MCP server reviewed in S8a1. Fix: move `Add-Type` into the
bitmap helpers (or a `Initialize-ImageRuntime` function the image samples call), and let each entry
point own its error preference. **Closed by F11a 2026-08-22.**

**S8b2-4 (P3) — the "first non-null input" fallback in `Get-RequestInputValue` is unreachable for
real requests and wrong if it ever becomes reachable.** `:65-71` iterates `$inputs.GetEnumerator()`
and returns the first non-null value when none of the requested names matched, but the guard is
`$inputs -is [System.Collections.IDictionary]`, and every sample builds `$request` with
`ConvertFrom-Json` (`image-blend/runtime/main.ps1:2`), which in Windows PowerShell 5.1 yields
`PSCustomObject` — never an `IDictionary`. So the branch is dead on the shipped path. Should a
caller ever pass a hashtable, the branch would pick an arbitrary entry by hash-table enumeration
order, silently treating an unrelated input (a mask, a caption, a stray parameter) as the source
image. Fix: delete the fallback, or make it explicit and order-stable by matching on a declared
input-kind rather than "whatever came first". **Closed by F11a 2026-08-22.**

**S8b2-5 (P3) — an empty string counts as a hit, so a blank alias masks a populated one.**
`Get-JsonPropertyFromNames:30-36` returns the first value that is merely non-null. `Resolve-ImagePath`
consults nine aliases in one list (`:133-135`: `path`, `filePath`, `imagePath`, `url`, `source`,
`value`, `data`, `base64`, `imageBase64`), so an object shaped `{ "path": "", "url": "<real>" }` —
exactly the shape an upstream provider emits when it knows a URL but no local path — resolves to
`""`, falls through `:109-113`, and returns `$null`. The Art then reports "input and reference
images are required" (`image-blend/runtime/main.ps1:8-10`) while holding a perfectly good value.
`Get-RequestWorkRoot:44` shows the intended semantics — it rejects whitespace explicitly. Fix: skip
null *and* whitespace-only values inside `Get-JsonPropertyFromNames`. **Closed by F11a 2026-08-22.**

**S8b2-6 (P3) — the fallback work root is shared and its filenames are fixed, so off-host callers
collide and leave files behind.** When `context.tempDir` and `context.cacheDir` are both absent,
`:48` falls back to a single `%TEMP%\loom-art-package-runtime` for every Art and every invocation,
and the filenames written into it are constants: `"$Label-input.png"` (`:115`) with labels hardcoded
per sample (`blend-source`, `blend-reference`, `remove-bg`, `compress`) and outputs such as
`image-blend-output.png` (`image-blend/runtime/main.ps1:18`). Two concurrent executions therefore
overwrite each other's input and output files, and nothing ever deletes them. This is latent rather
than live: `framework_process.rs:258-263` always supplies a per-request
`%TEMP%\loom-framework\<request_id>` wrapped in `TempDirectoryGuard`, so production takes the unique
branch and gets cleanup for free. The exposure is the smoke harness, manual runs, and any future
caller that omits `context`. A second, smaller issue in the same function: the requested root is
trusted verbatim and created with `New-Item -Force` (`:50`), so whatever path the request names is
brought into existence. Fix: derive the fallback root from a per-process GUID, include the same
suffix in the generated filenames, and validate a requested root the way the host validates output
paths. **Closed by F11b 2026-08-22.**

**S8b2-7 (P3) — decoded data-URL inputs are unbounded and always written as `.png`.** `:114-117`
matches `^data:image\/[A-Za-z0-9.+-]+;base64,(?<data>.+)$` with no length limit, decodes the whole
capture with `[Convert]::FromBase64String`, and writes it to `"$Label-input.png"` regardless of the
subtype that was declared. The subtype pattern admits `svg+xml`, so an SVG payload is written under
a `.png` name and then fails inside `Bitmap::new` with a generic GDI+ message rather than a useful
"unsupported image type". Peak memory for one input is roughly the base64 string plus the decoded
byte array plus the file, all unbounded from this file's point of view. Fix: cap the encoded length,
map the declared subtype to a real extension, and reject subtypes GDI+ cannot decode up front.
**Closed by F11c 2026-08-22.**

**S8b2-8 (P3) — three bitmap helpers leak GDI+ objects when the operation fails partway.**
`Load-BitmapArgb:146-169` creates `$bitmap` at `:151` outside the `try`, whose `finally` disposes
only `$loaded`; `Resize-BitmapArgb:178` creates `$resized` before its `try`, whose `finally` disposes
only `$graphics`; `Blend-Bitmaps:203` creates `$output` before its `try`, whose `finally` disposes
only `$referenceSized`. Any failure in the intervening work — an out-of-memory `DrawImage`, a GDI+
error, the timeout kill landing mid-loop — abandons a full-size 32bpp surface. The blast radius is
small because these are one-shot processes that exit on error (`$ErrorActionPreference = "Stop"`),
but the pattern is wrong and will matter the moment a helper is reused inside a loop. Fix: allocate
inside the `try` and dispose on the failure path. **Closed by F11d 2026-08-22.**

**S8b2-9 (P3, performance) — the output builders decode the entire image just to read its
dimensions, and one of them reads the file twice.** `New-ImageOutput:251` and
`New-ImagePathOutput:285` construct a `System.Drawing.Bitmap` from the path solely to reach
`.Width`/`.Height` (`:257-258`, `:289-290`); `Bitmap`'s path constructor decodes the image and holds
a lock on the file for the lifetime of the object. `New-ImageOutput` then calls
`Convert-ImagePathToDataUrl:242`, which does `ReadAllBytes` on the same file and base64-encodes it.
Peak footprint for one response is therefore the decoded surface (width × height × 4 bytes,
unrelated to the compressed size) plus the raw byte array plus the base64 string at 1.37× plus the
copy `ConvertTo-Json` makes — and per S6b2c3-2 the base64 appears twice in that JSON. Fix: read
dimensions from the header only (`Image.FromStream(stream, false, false)` on a `FileStream`, or parse
the PNG IHDR), and stream the base64 instead of materializing the byte array. Queued for S9 with
S6b2c3-3, S8b1-7, and S8a2b-12. **Closed by F11q 2026-08-22 for the decode and the double read; the
streaming half was measured and declined, see that record.**

**S8b2-10 (P3) — the response writers have no size bound and their non-ASCII correctness depends on
which PowerShell runs them.** `Write-SuccessResponse:350` serializes with
`ConvertTo-Json -Depth 40 -Compress` and writes through `[Console]::Out.WriteLine` with no cap on
the serialized length, so the candidate-array amplification described in S6b2c3-2 is unbounded at
its source as well as at the host. Separately, Windows PowerShell 5.1's `ConvertTo-Json` escapes
every non-ASCII character as `\uXXXX`, which is what makes this safe today given the hardcoded
`powershell.exe` of S7b1-5; PowerShell 7 emits raw UTF-8 instead, and `[Console]::Out` in a spawned
child defaults to the console's OEM code page (cp936 on this machine), so the same code under `pwsh`
would mangle any non-ASCII title or error message on the way out. Fix: bound the serialized length
before writing, and set `[Console]::OutputEncoding` to UTF-8 explicitly so the behaviour does not
depend on the interpreter or the machine's code page.

**S8b2-11 (P3) — small robustness gaps in the file-writing helpers.** `Save-Png:234-235` computes
`Split-Path -Parent $Path` and passes it to `New-Item`; for a bare filename that parent is the empty
string and `New-Item -Path ''` throws a parameter-binding error under
`$ErrorActionPreference = "Stop"`, turning a valid relative save into an obscure failure.
`New-PlaceholderImage:313-320` hardcodes 256×160 and `Segoe UI` 16pt, so the placeholder silently
font-substitutes on any image where that face is unavailable and cannot adapt to the size the caller
actually needs. Fix: skip directory creation when the parent is empty, and take the placeholder size
as parameters with a documented default.

Confirmed correct in this file:

- `Get-JsonPropertyValue:5-22` null-guards its input and checks `IDictionary.Contains` before
  indexing, so it never throws on a missing key regardless of which shape the JSON deserializer
  produced.
- `Resolve-ImagePath` never fetches over HTTP. An `http`/`https` string fails `Test-Path` and returns
  `$null`, so this library is not a second copy of the S8b1-1 confused deputy — the only network
  reach it has is the UNC path of S8b2-1.
- The data-URL pattern at `:114` is anchored at both ends and requires the `image/` media type and
  `;base64,`, so `data:text/html`, `javascript:`, and unencoded data URLs are all rejected.
- The `file://` conversion is wrapped in `try/catch` (`:120-125`) and returns `$null` on a malformed
  URI rather than propagating a parse error.
- `Get-RequestParamValue:75-88` compares against `$null` rather than testing truthiness, so a
  legitimate `0` or `$false` parameter is not silently replaced by the default.
- `Load-BitmapArgb:146-169` copies into an explicit `Format32bppArgb` surface with
  `CompositingMode::SourceCopy` before any processing, which normalizes indexed and CMYK sources and
  avoids the "cannot create Graphics from an indexed bitmap" failure that would otherwise hit
  `Blend-Bitmaps`.
- Every `Graphics` object is disposed in a `finally` (`:161-163`, `:189-191`, `:326-328`), and
  `New-ImageOutput`/`New-ImagePathOutput`/`New-PlaceholderImage` dispose their bitmaps in `finally`
  blocks too (`:274-276`, `:299-301`, `:332-334`) — the leaks of S8b2-8 are strictly on the
  exceptional path.
- `Blend-Bitmaps:208` clamps alpha to 0..1 before use, so `image-blend`'s `mix_ratio` cannot produce
  out-of-gamut arithmetic even when the caller sends a negative or greater-than-100 ratio.
- `Blend-Bitmaps:202` resizes the reference to the source's dimensions rather than assuming they
  match, so mismatched inputs blend instead of throwing an index error.
- `Write-ErrorResponse:353-371` always emits a single compact JSON object with `status`, `code`, and
  `message`, and adds `detail` only when it is non-blank — the shape `framework_process.rs` expects,
  which is why the samples' outer `catch` blocks produce parseable failures instead of the raw
  PowerShell error records that would break response framing.

### S8c1 — Stock-monitor Art runtime: request handling, upstream fetch, parsing

Owner of every fix in this slice: **Lane B / F7** (`art-packages/samples/stock-monitor/runtime/main.ps1` is reserved by Lane B from 2026-08-21). **All fixed 2026-08-21 — the batch record is `### F7` in `phase-78-lane-sync.md`.**

`art-packages/samples/stock-monitor/runtime/main.ps1:1-500`. Unlike the image samples this Art does
not dot-source `common.ps1`; it is self-contained, opens with `$ErrorActionPreference = "Stop"` plus
`Set-StrictMode -Version Latest` (`:1-2`), and declares its limits as script-scope constants
(`:4-13`). This half contains the request accessors (`:15-97`), numeric and code normalization
(`:99-173`), timestamp and market-session logic (`:175-287`), the MCP result reader (`:289-333`), and
the shape converters that turn upstream payloads into the Art's own model (`:335-500`).

**S8c1-1 (P2) — `Find-SurfaceAction` searches the entire request for a `surfaceAction` key, so
untrusted upstream data can inject one, and the search itself is unbounded.** `:32-60` walks every
dictionary value (`:38`), every `PSObject` property (`:47`), and every non-string enumerable (`:53-57`)
looking for the first `surfaceAction` it can find, at any depth, in enumeration order. Two problems
compound:

- The request also carries `frameworkData.mcp.results` (read at `:295-298`), which is upstream
  content the Art does not author. A tool result that contains a `surfaceAction` object anywhere
  inside it is indistinguishable, to this function, from the genuine surface action the host
  attached — and because the traversal returns the *first* match in enumeration order, an injected
  one can win. The Art then treats a value chosen by a remote server as the user's interaction
  intent.
- There is no depth bound and no visited set, the same shape as S8b1-2 and S6b2d3-2. Depth is
  incidentally limited by `ConvertFrom-Json`'s own recursion limit rather than by anything this file
  does.

Fix: read the surface action from its declared location only (the host writes a fixed key on the
request root — resolve it with `Get-ObjectPropertyValue`, not a search), and if a search must be
kept, bound the depth the way `framework-packages/runtime-host/src/mcp.rs:703-716` does and refuse to
descend into `frameworkData`.

Reassigned 2026-08-21: this finding was listed under F2 (Lane A), but the file is reserved by
Lane B and the fix edits the same `Find-SurfaceAction` region as S8c1-2 and S8c1-3, so it is
now part of **F7 / Lane B**. Lane A should not patch this function; F2 keeps its other six
call sites. The PowerShell fix ports the depth-bound shape from
`framework-packages/runtime-host/src/mcp.rs:703-716` by hand — it cannot call `loom_security`.

**S8c1-2 (P2) — a missing upstream timestamp is replaced by "now", so data of unknown age is
presented as fresh.** `ConvertTo-OrderBook:462-463` normalizes `fetchedAt` and, when the result is
null, substitutes `[DateTimeOffset]::UtcNow`; `observedAt` then falls back to that same value
(`:464`), `ageSeconds` comes out at roughly 0 (`:465`), and `stale` at `:479` evaluates to `$false`.
`ConvertTo-LiveTape:494-497` does exactly the same. The consequence is a fail-open staleness
indicator: an upstream that omits timestamps entirely — or one whose timestamps fail
`Resolve-UtcTimestamp`'s parse for any reason — produces an order book and a live tape that claim to
be seconds old no matter how old they actually are, in a display whose whole purpose is to tell the
user whether what they are looking at is current. The disclaimer at `:13` acknowledges delay in
general but the per-field `stale` flag is what the Surface renders. Fix: keep the age unknown when it
is unknown — emit `ageSeconds = $null` and `stale = $true` when no upstream timestamp could be
parsed, and never synthesize `observedAt` from local clock time.

**S8c1-3 (P3) — `AssumeUniversal` silently reinterprets naive local timestamps as UTC.**
`Resolve-UtcTimestamp:185-189` parses with `DateTimeStyles::AssumeUniversal`, which is correct for a
string that carries an offset (the flag is ignored) but wrong for one that does not. A provider that
emits market-local wall-clock time without an offset — `"2026-08-21 09:35:00"` from a mainland source
— is read as 09:35 UTC, i.e. eight hours in the past, so `Get-ObservationAgeSeconds:201` returns
~28,800 and every consumer marks the data stale against `MaxLiveAgeSeconds` (90) and
`MaxOrderBookAgeSeconds` (120). Together with S8c1-2 the two failure modes point in opposite
directions: no timestamp reads as perfectly fresh, a naive timestamp reads as eight hours rotten.
The path is live because `Get-StockFromActionState:359-360` and `ConvertTo-FavoriteQuotes:395-396`
re-ingest `observedAt`/`fetchedAt` out of Surface-supplied `authoritativeState`, which round-trips
whatever was previously stored. Fix: require an offset or a trailing `Z`, and reject a naive string
rather than assuming a zone for it.

**S8c1-4 (P3) — the market-session calculation uses Windows-only time zone ids and its fallback
computes a wrong answer instead of no answer.** `Get-MarketSessionState:268` selects
`"Eastern Standard Time"` or `"China Standard Time"`, which are Windows registry ids; on any
non-Windows PowerShell `ConvertTimeBySystemTimeZoneId` throws and the `catch` at `:272-274` falls back
to `[DateTimeOffset]::UtcNow`. The minute-of-day arithmetic at `:277-282` then runs against UTC, so
the A-share window 09:30–11:30 is evaluated in UTC and the function reports "open" at 17:30–19:30
Beijing time and "closed" during the actual session. This is latent today — `.ps1` entry points are
spawned through a hardcoded `powershell.exe` per S7b1-5 — but the failure is silent and inverted
rather than visible. Fix: fall back to a fixed `TimeSpan` offset per market (or use the IANA ids
available in .NET 6+ and map both ways), and if no zone can be resolved, return an explicit
`"unknown"` state the Surface can render as such.

**S8c1-5 (P3) — the trading date is normalized and then thrown away.** `ConvertTo-HistoryRows:409`
computes `$normalizedDate = Resolve-TradingDate -Value $date` and uses it only as a null check at
`:414`; the row emitted at `:422` stores the raw `$date` string. Since `Resolve-TradingDate:247`
matches on a *prefix* (`^(\d{4}-\d{2}-\d{2})`), an upstream value such as `"2026-08-21 15:00:00"` or
`"2026-08-21T15:00:00+08:00"` validates successfully and is then emitted whole, so the history array
can carry three different date formats depending on which provider answered — and the Surface has to
cope with all of them. Fix: emit `$normalizedDate`, and keep the original in a separate field if the
intraday component is actually needed.

**S8c1-6 (P3) — every array conversion truncates by position and drops rows without saying so.**
`ConvertTo-FavoriteQuotes:369` keeps `Select-Object -First 12` with the 12 hardcoded and unrelated to
any declared input limit; `ConvertTo-HistoryRows:407` keeps `Select-Object -Last 2000`, which assumes
the provider returned rows in chronological order and never sorts — the same order-dependence as
S8a2a-5, and the same fix (sort by date before slicing, as `stock-api-entry.js:835` already does on
one of its two paths); `ConvertTo-OrderBookLevels:438` keeps `-First 10` in whatever order arrived
rather than sorting bids descending and asks ascending by price, which is the only ordering an order
book is meaningful in. On top of the truncation, `:414-419` and `:440` silently `continue` past rows
that fail validation, so a suspended-trading day with zero prices simply disappears and the chart
closes the gap as if the days were contiguous. None of the three functions reports how many items it
dropped. Fix: sort before slicing, derive the favorites cap from the declared input schema, and
return a dropped-row count the Surface can surface.

**S8c1-7 (P3) — percentage and fraction representations of the same quantity travel together with
asymmetric rounding.** `Get-StockFromActionState:349-357` reads `changePercent` (a percentage),
rounds it to 8 digits, and emits `percent` as `changePercent / 100.0`;
`ConvertTo-FavoriteQuotes:376-383` reads *both* `percent` (8 digits) and `changePercent` (4 digits)
and reconstructs whichever is missing from the other, or from `(price - previousClose) /
previousClose * 100`. So the same number exists in two units, with two different rounding
precisions, on two adjacent code paths, and it round-trips through Surface state — exactly the
ambiguity flagged as S8a2a-4 on the MCP side, now duplicated on the Art side. A single missed
conversion is a 100× error in a financial display. Fix: pick one unit at the boundary (fraction, per
the wire model), convert once on render, and round at one precision.

**S8c1-8 (P3) — three distinct upstream failures collapse into one opaque string, and the upstream's
own text is echoed verbatim.** `Get-McpToolContent` throws for "no result at all" (`:300-302`), for
`isError` (`:304-308`), and for "result present but no `structuredContent`" (`:309-311`);
`Try-Get-McpToolContent:315-333` catches all three into a single `error` string, so the caller cannot
tell a quota/timeout miss from a server-reported error from a schema mismatch, and cannot choose a
different degradation for each. The `isError` branch also interpolates the upstream `message`
directly into the Art's error text (`:307`), the same echo problem as S8a1-9 and S6b2c3-10 — whatever
the remote server put there ends up in the Art's output. Fix: return a discriminated reason code
alongside the message, and either drop or length-clamp the upstream text.

**S8c1-9 (P3) — the version constants are hand-maintained and already disagree with the package they
describe.** `:11-12` declare `ProviderVersion = "2.9.0"` and `UpstreamVersion = "2.7.3"`. The MCP
wrapper this Art consumes declares `WRAPPER_VERSION = "2.9.0"` and `PYSNOWBALL_VERSION = "0.1.8"`
(`mcp-server-packages/stock-api/runtime/stock-api-entry.js`), so `2.7.3` corresponds to nothing that
ships. This is the same defect as S8a2b-3, now on both sides of the same interface. Fix: read the
version out of the manifest at runtime, or drop the constant rather than displaying a number nobody
updates.

**S8c1-10 (P3) — `$input` is assigned inside a function, and several functions use unapproved
verbs.** `Resolve-StockCode:119` writes to `$input`, which is PowerShell's automatic pipeline
enumerator; the function happens to be called positionally everywhere so nothing breaks today, but
the assignment silently destroys pipeline input for any future caller that pipes into it, and it is
the exact pattern `PSAvoidAssignmentToAutomaticVariable` exists to catch. `Try-Get-McpToolContent`
(`:315`) stacks two verbs, and the shared helper reviewed in S8b2 adds `Load-BitmapArgb` and
`Blend-Bitmaps`; none of these are approved verbs, so `Get-Verb`-based tooling and any future module
manifest will flag them. Fix: rename the local to `$text`, and use `Get-`/`Resolve-`/`ConvertTo-`
prefixes consistently.

**S8c1-11 (P3) — numeric coercion is looser than the fields it feeds.** `Convert-NullableNumber`
parses with `NumberStyles::Float`, which accepts exponents and surrounding whitespace but not
thousands separators, so a provider that formats `"1,234.50"` yields `$null` and the row is dropped
by `:414` or `:440` with no diagnostic. In the other direction, `ConvertTo-OrderBookLevels:442` casts
the provider-supplied `level` to `[int]` after rounding with no clamp, so a value beyond
`Int32.MaxValue` throws an overflow inside a `Stop`-preference script and takes the whole render
down; the `$levels.Count + 1` default is also evaluated on every iteration whether or not the
provider sent a level. Fix: allow `NumberStyles::Float, AllowThousands` on parse, and clamp `level`
to `1..$script:MaxOrderBookLevels` instead of casting blind.

Confirmed correct in this half:

- `Set-StrictMode -Version Latest` alongside `$ErrorActionPreference = "Stop"` (`:1-2`) — the
  strictness the shared image helper lacks (S8b2-3), so an unset variable or a missing property fails
  loudly instead of evaluating to `$null`.
- `Convert-NullableNumber:99-114` parses with `InvariantCulture`, uses `TryParse` rather than a cast,
  and explicitly rejects `NaN` and infinity before rounding — the parse cannot throw and cannot leak
  a non-finite value into the JSON output.
- `Resolve-RefreshInterval:205-211` and `Resolve-MarketPeriod:214-220` validate against the
  allowlists at `:5-6` and fall back to a declared default, so no caller-supplied interval or period
  ever reaches the upstream request unchecked.
- `Resolve-StockCode:116-146` throws on an unrecognized format instead of guessing, and its A-share
  market inference (`:129-138`) covers the real prefix ranges (4x/8x NEEQ, 5/6/9 Shanghai including
  688 STAR and 900 B-shares, everything else Shenzhen).
- `Get-MarketFromCode:150` uses `Substring(0, 2)` without a length guard, which is safe because every
  `Resolve-StockCode` return path produces at least three characters.
- `Resolve-TradingDate:249-259` confirms the extracted prefix with `ParseExact` rather than trusting
  the regex, so `2026-02-31` is rejected.
- The missing holiday calendar in `Get-MarketSessionState` is neutralized by the `$isLatestDay` guard
  (`:276`, `:283`): on a holiday the last trading date is not today, so the function reports "closed"
  even though the weekday check passes.
- The session windows themselves are right for all three markets (`:279-281`): 09:30–16:00 US,
  09:30–12:00 plus 13:00–16:00 HK, 09:30–11:30 plus 13:00–15:00 mainland.
- `ConvertTo-HistoryRows:417` rejects non-positive prices *and* `high < low`, so a transposed or
  zero-filled row cannot reach the chart.
- `ConvertTo-OrderBook:461` returns `$null` when both sides are empty rather than emitting a hollow
  book, and `:474` reports `levels` as the max of the two side counts rather than assuming symmetry.
- The order book's `stale` flag is computed from a real age against a declared `maxAgeSeconds`
  (`:477-479`) — the flag the MCP wrapper could never set true (S8a2a-10) is genuinely computed here.
- `Get-ObjectPropertyValue:15-30` handles the dictionary and `PSObject` shapes separately and takes
  an explicit default, so it behaves identically whether the request arrived through
  `ConvertFrom-Json` or was constructed in a test.

### S8c2 — Stock-monitor Art runtime: rendering, output assembly, entry point

Owner of every fix in this slice: **Lane B / F7** (same reserved file as S8c1). **All fixed 2026-08-21 — the batch record is `### F7` in `phase-78-lane-sync.md`. S8c2-1 was fixed without changing `apps/daemon`; see handoff H7 there for why the result patch must stay an explicit empty object.**

Scope: `art-packages/samples/stock-monitor/runtime/main.ps1:501-1000` — the tail of
`ConvertTo-LiveTape`, the snapshot assembler `Get-StockSnapshot:522-716`, the display formatters
`Format-Price`/`Format-SignedNumber:718-738`, the patch helper `New-SetOperation:740-747`, the three
response writers `Write-SurfaceResponse`/`Write-RuntimeSuccess`/`Write-RuntimeError:749-780`, the
error-state builder `Write-SurfaceErrorState:782-826`, the flattener `New-FormalQuote:828-867`, and
the `try`/`catch` entry point `:869-999`.

**S8c2-1 (P2, performance) — the same payload is serialized three times in one response.**
The entry point builds `$statePatch` (`:950-971`), which embeds the full `quote`, `history` (up to
`MaxHistoryRows = 2000` rows), `orderBook`, `liveTape`, and `favoriteQuotes`. The final call at
`:983-991` then passes that one object to *both* `-Patches` (as the patch's `statePatch`) and
`-Result` (as `result.statePatch`), and additionally passes `$formalQuote` — which
`New-FormalQuote:828-867` built by flattening the same snapshot, history rows included. So a
2000-row history crosses stdout three times in a single response, and `Write-SurfaceResponse:763`
serializes the whole thing with `ConvertTo-Json -Depth 100 -Compress` under no size bound. At seven
numeric fields per row that is roughly 6000 row objects where 2000 would do. The host then carries
that payload through the store (S6b2b1-1 clones it with the mutex held) and the Surface receives it
twice. Fix: emit the state patch once — either in `patches` or in `result.statePatch`, not both —
and let `formalQuote` reference the state rather than re-embedding the arrays.

**S8c2-2 (P2) — a rejected action still writes its own attacker-chosen text into the Surface.**
`:877-878` allowlists five action ids and `throw`s on anything else. That throw lands in the `catch`
at `:993`, which calls `Write-SurfaceErrorState` with the *same* rejected action (`:995`).
`Write-SurfaceErrorState` then reads `payload.value` off that unvalidated action to decide the symbol
(`:789-794`) — falling back to the raw trimmed string when `Resolve-StockCode` throws (`:795`) — and
writes the thrown message, which interpolates the caller's `$actionId` verbatim at `:878`, into both
the persisted state (`error`, `:810`) and the `quote_change` display node (`:819`). Net effect: an
undeclared action id round-trips up to 400 characters of caller-controlled text into rendered Surface
props and into stored state, and sets the symbol field to an arbitrary string that later runs pass
back through `Resolve-StockCode`. The validation is present but its failure path is more permissive
than its success path. Fix: on an unknown action id, write a fixed generic error state that does not
echo the action id and does not adopt its payload.

**S8c2-3 (P3) — `Write-SurfaceErrorState` truncates by UTF-16 code unit and can emit a lone
surrogate.** `:788` does `$Message.Substring(0, 400)` when the message is longer. If character 400
falls between the high and low halves of a surrogate pair — any emoji or astral character in an
echoed upstream message (and upstream messages *are* echoed verbatim, S8c1-8) — the result holds an
unpaired surrogate. PowerShell 5.1 escapes it as a bare `\udXXX`, which is not valid JSON, so the
host's parse of the whole response fails and the user sees a framework error instead of the error
state that was being reported. Fix: truncate on a text-element boundary
(`[System.Globalization.StringInfo]`), or strip non-BMP characters before the cut. Note also that
`Write-RuntimeError:774-780` applies no truncation at all, so the non-Surface path passes an
unbounded upstream string straight out.

**S8c2-4 (P3) — `historyError` is computed and then dropped.** `Get-StockSnapshot` returns
`historyError` at `:714`, populated only when the history call failed *and* no fallback rows were
found. The entry point never reads it: the state patch hardcodes `error = $null` at `:969`, and the
only user-visible trace is the generic `"曲线将在下次刷新补齐"` chosen at `:942-944`. So the actual
upstream reason for the missing curve is discarded, and when history *did* fall back to stale state
rows (`:572-584`) nothing anywhere records that the fresh fetch failed. Fix: put `historyError` into
the state patch as a non-fatal warning field and let the Surface show it.

**S8c2-5 (P3) — the synthetic latest kline stamps the machine-local date.** When history is empty,
`:605-613` fabricates a kline from the quote and sets `date = (Get-Date).ToString("yyyy-MM-dd")` —
the *host's* local date, not the market's. That value becomes `lastTradingDate` (`:615-620`), which
`Get-MarketSessionState` compares against the market-local today via its `$isLatestDay` guard
(S8c1's `:276`/`:283`). For a US symbol fetched at 09:00 Beijing time the host date is already the
next calendar day in New York terms, so `$isLatestDay` is false and the panel reports the market
closed during the actual US session. Fix: derive the date in the market's own zone, the same
conversion `Get-MarketSessionState:268` already performs.

**S8c2-6 (P3) — `fetchedAt` falls back to "now", repeating the S8c1-2 fail-open pattern.**
`:549` computes `$fetchCompletedAt = [DateTimeOffset]::UtcNow.ToString("o")` and `:550-553` uses it
whenever the upstream omitted `fetchedAt`. Every downstream freshness decision then treats a
timestamp-less response as having just arrived. This is the third site with the same shape
(`ConvertTo-OrderBook:462-463`, `ConvertTo-LiveTape:494-497`, here); the fix is the same — leave the
timestamp null and mark the record stale rather than substituting the current clock.

**S8c2-7 (P3) — `marketStatus` can be forced open by upstream but never forced closed.**
`:649-651` overwrites the computed session state with `"open"` whenever `liveTape.isTrade` is true,
discarding the weekday/session-window calculation. The flag is upstream-controlled and the override
is one-directional, so a provider that leaves `isTrade` set outside trading hours makes the panel
claim the market is open on a weekend — and because the gate also requires `$liveTapeFresh`
(`:647`), which S8c1-2 makes trivially true for a timestamp-less tape, the two defects compose into
"upstream can pin the display to live mode at any hour". Fix: treat `isTrade` as a signal that can
only narrow the computed state (open→closed), or record it as a separate `providerIsTrade` field
instead of overwriting `marketStatus`.

**S8c2-8 (P3) — the closed-market branch reports a meaningless `ageSeconds` with `stale = $false`.**
`:665` derives `$effectiveObservedAt` from `$latest.date`, a bare `yyyy-MM-dd` string, so
`Resolve-UtcTimestamp` yields midnight UTC of that day (S8c1-3's `AssumeUniversal`) and `:669`
computes an age of eight to thirty-odd hours. `$quoteStale` at `:670` only trips when
`marketStatus -eq "open"`, so the response carries `ageSeconds` in the tens of thousands next to
`stale = $false`. The two fields contradict each other and the number does not correspond to the
close time. Fix: when the market is closed, report the trading day rather than a synthetic age, or
set `ageSeconds = $null`.

**S8c2-9 (P3) — a benign upstream normalization difference is a hard error.** `:909-917` re-resolves
the requested code and `throw`s `"stock-api 返回的股票代码与请求不一致"` when it differs from the code
on the returned quote; `:919-931` does the same for `period`. The echo check is right in principle,
but it compares against whatever spelling the upstream chose, so any provider-side normalization
(`BRK.A` returned as `BRK-A`, a period alias collapsed to its canonical form) discards a valid
response and renders an error. `Resolve-StockCode` at `:915` can also throw on its own here, unlike
the guarded call sites at `:579` and `:795`, so a malformed stored code surfaces as a format error
rather than the targeted mismatch message. Fix: compare after normalizing both sides through
`Resolve-StockCode`/`Resolve-MarketPeriod`, and treat a mismatch as a warning attached to the state
unless the market differs.

**S8c2-10 (P3) — `Format-Price` hardcodes two decimals for every market.** `:718-722` formats with
`"0.00"`, and `:979` feeds the headline price through it. Hong Kong penny stocks quote in
thousandths (`0.023` renders as `0.02`, a 13% display error) and several US instruments quote to four
decimals. `Format-SignedNumber:724-738` takes a `Digits` parameter, so the asymmetry is unnecessary.
Fix: pick the precision from the market (or from the number of decimals the provider sent) instead
of fixing it at two.

**S8c2-11 (P3) — the action/code/period resolution triplet is copy-pasted at three call sites.**
The `if ($actionId -eq "stock_symbol_commit") { payload.value } else { authoritativeState.code }`
shape appears at `:573-578`, `:789-794`, and `:909-914`; the period equivalent at `:559-564` and
`:919-924`; and `$actionId` is re-read from `$script:SurfaceAction` four separate times (`:525`,
`:876`, `:909`, `:918`). The three code sites already differ — only `:579` and `:795` wrap
`Resolve-StockCode` in a `try`, `:915` does not (S8c2-9) — which is exactly how duplicated resolution
logic drifts. Fix: resolve the requested code and period once at the entry point and pass them down.

Confirmed correct in this slice:

- `authoritativeState` is host-owned, not client-supplied: the daemon persists it per instance
  (`apps/daemon/src/lib.rs:4671`, `:6511`) and `surface_actions.rs:403` copies it from the stored
  instance into the dispatched action. So the snapshot's fallbacks onto state (`:535-537`, `:582`,
  `:632-645`, `:702`) read host state, not attacker input — the trust boundary is where it should be.
  The `runtime-host` binding validator independently enforces that Surface bindings root at
  `payload` or `authoritativeState` (`framework-packages/runtime-host/src/mcp.rs:357-372`).
- Cached order book and live tape are reused only when the cached code matches the resolved code and
  the cached record is not stale (`:630-646`), so the optional-enhancement fallback cannot silently
  show another symbol's depth — and the Chinese comment at that site records *why* the book is
  optional (Xueqiu returns no ten-level depth for Hong Kong names).
- History reuse from state is gated on both the code and the period matching (`:572-584`), so
  changing the interval cannot render the previous period's curve.
- `$displayPrice` (`:653-659`) cannot be null on any path: `$latest.close` is validated non-null by
  `ConvertTo-HistoryRows`, the synthetic kline copies the already-checked `$price`, and
  `ConvertTo-LiveTape` rejects a null price — so the arithmetic at `:684-685` never silently coerces
  `$null` to zero.
- `lastTradingDate` is validated rather than defaulted: `:615-624` tries the latest kline, then the
  quote's own date, then throws `"stock-api 返回的最近交易日无效"` instead of inventing a day.
- The interval-commit early exit (`:880-895`) deliberately skips the upstream fetch and patches only
  the interval, which is the right call for a control that does not change what is displayed.
- `Write-RuntimeSuccess:766-772` and `Write-SurfaceResponse:749-764` both emit exactly the envelope
  `framework_process.rs` expects, and the Surface path stamps
  `protocolVersion = "loom.surface.v1"` explicitly rather than relying on a host default.
- `Format-Price`/`Format-SignedNumber` render nulls as `"--"` and use `InvariantCulture`, so the
  display never depends on the host locale's decimal separator.

### S8d1 — Stock-monitor Surface: bootstrap, state ingest, helpers

Owner of every fix in this slice: **Lane B / F5 and F6** (`art-packages/samples/stock-monitor/surface/main.js` is reserved by Lane B from 2026-08-21).

Scope: `art-packages/samples/stock-monitor/surface/main.js:1-450` — the constant tables (`:4-57`),
the module-level mutable state (`:59-83`), the state accessors and formatters (`:85-209`), the
stylesheet source (`:211-325`), the markup template (`:327-376`), the metric table (`:378-387`), and
the action emitter `emitAction:389-450`.

**S8d1-1 (P2) — the client gives up long before the host does, so slow-but-successful fetches
produce a false timeout and two concurrent in-flight actions.** `ACTION_TIMEOUT_MILLIS = 50000`
(`:55`) is a hardcoded guess about the host's budget — its only use is deriving
`PENDING_TIMEOUT_MILLIS = 52000` at `:56`; it is never sent to the host and the host never sends its
budget down. The real budgets are larger: the framework process gets 120 s
(`DEFAULT_FRAMEWORK_PROCESS_TIMEOUT`), and each of the four MCP calls the Art makes per snapshot
(`quote`, `history`, `orderbook`, `favorites`) gets 60 s. `TICK_TIMEOUT_MILLIS = 32000` (`:34`) is
shorter still. So on any fetch slower than 52 s the timer at `:413-422` fires, clears `pending`,
and writes `"刷新超时"` — while the request is still running. The controls unlock, a second refresh
can be emitted against the same instance, and when the first response finally lands the error state
is overwritten by data, leaving the user with a transient error that had no cause. Fix: have the host
publish its effective deadline in the snapshot and derive the client timer from it, or set the client
timer above the host ceiling so it only ever catches a genuinely lost response.

**S8d1-2 (P2, performance) — the entire chart series is re-derived from raw state on every render.**
`chartRowsOf:105-115` walks the full history array, allocates a fresh six-field object per row, then
filters; `downsampleRows:116-135` walks the result again, and per output bucket does
`bucket.slice`, two spread calls (`Math.max(...bucket.map(...))`, `Math.min(...)`) and a `reduce`.
Both run at `:762` inside the draw path, which `render:1035` invokes on every snapshot — and
`emitAction` additionally renders twice on the rejection path (`:424`, `:447`). With the runtime's
`MaxHistoryRows = 2000` and the minimum refresh interval of one second, that is 2000 object
allocations plus a full re-bucket every second in a WebView, for data that changed by at most one
row. Fix: cache the derived series keyed on (code, period, revision) and recompute only when the key
changes; the tick path in particular mutates only the last point.

**S8d1-3 (P3) — `normalizeCode` is a second, independent implementation of `Resolve-StockCode`, and
its failure path forwards raw text.** `:171-185` re-derives the market prefix in JavaScript
(`/^[48]/` → `BJ`, `/^[569]/` → `SH`, else `SZ`, plus HK zero-padding and a US pattern) duplicating
the PowerShell inference at `main.ps1:116-146`, with no shared fixture pinning the two together —
the same two-language duplication as S8a2a-4 and S8c1-7. When nothing matches, `:184` returns the
input verbatim, so unnormalizable text is emitted as the action payload (`:1141`, `:1145`) and is
only rejected one process later by the runtime's `throw`. Fix: validate locally and refuse to emit,
or move normalization to one side and let the other treat the code as opaque.

**S8d1-4 (P3) — rows are dropped silently and the row count shown does not match the row count
drawn.** The filter at `:115` discards any row with a null open/close/high/low or `high < low`,
without counting. The `周期` metric at `:383` reports `history.length` from the *unfiltered* array,
so the panel can read `"日 K · 2000 条"` while the chart plots fewer points. Third occurrence of the
silent-truncation pattern (S8a2a-5, S8c1-6). Fix: derive the label from the filtered series and
surface the drop count when it is non-zero.

**S8d1-5 (P3) — display precision is pinned to two decimals for every market.** Every metric formats
with `formatNumber(…, 2)` (`:379-382`), `formatSigned` hardcodes 2 (`:148`), and `formatNumber`
pins the locale to `zh-CN` (`:139`). Hong Kong names quoting in thousandths and US instruments
quoting to four decimals are rendered wrong — `0.023` becomes `0.02`. This is the client-side twin of
S8c2-10 (`Format-Price`'s `"0.00"`), so the same defect must be fixed on both sides of the boundary.

**S8d1-6 (P3) — the percent/fraction ambiguity reaches the DOM, and the colour logic hides it.**
`formatSigned(value, "%")` (`:144-148`) appends `%` to whatever number arrived, so if the runtime's
percent-vs-fraction handling (S8c1-7) is off by a factor of 100 the panel prints `"+0.01%"` for a
1.23% move. `movement:186-190` only reads the sign, so the up/down colour stays correct and the
magnitude error looks like a legitimately tiny move rather than a bug. Fix: have the runtime declare
the unit explicitly and assert it here.

**S8d1-7 (P3) — bare-date timestamps are parsed as UTC and rendered in local time.** `:159` and
`:166` both do `new Date(value)`; for the date-only strings the runtime produces on the closed-market
path (`$latest.date`, S8c2-5/-8) that is UTC midnight, so a 15:00 Beijing close displays as
`08:00:00`. `formatTimestamp` also echoes an unparseable upstream string back verbatim (`:161`)
rather than falling back to its `"时间未知"` sentinel. Fix: detect the `yyyy-MM-dd` shape and render
it as a date, not a clock time.

**S8d1-8 (P3) — a throw from `NeuroSurface.emit` locks the controls for 52 seconds.** `:406-422`
sets `pending`, `pendingAction`, `pendingPeriod`, `pendingRevision` and arms `pendingTimer`
*before* calling `NeuroSurface.emit` at `:425`, and that call is not wrapped. The rollback at
`:440-446` runs only when `emit` returns a falsy value, not when it throws — so a host API that
rejects a payload by throwing leaves every flag set, the exception escapes into the DOM event
handler, and the UI stays locked until the timer fires and reports a timeout that never happened.
Fix: `try`/`catch` around the emit and route the catch through the existing rollback.

**S8d1-9 (P3) — suppressed actions are silent and indistinguishable from failures.** `emitAction`
returns `false` with no user-visible trace for a disposed or suspended surface (`:390`), a tick while
another request is in flight (`:393`), and a network action while one is pending (`:394`). A click
during an in-flight refresh therefore does nothing at all — no status text, no disabled cursor beyond
whatever the last render set. Fix: distinguish "busy, ignored" from "rejected" in the status line.

**S8d1-10 (P3) — the sample's default symbol `SZ000034` is hardcoded in five places.** On this side:
the input's `value` attribute (`:334`), the code label (`:347`), and the render fallback (`:1058`).
On the runtime side: `Write-SurfaceErrorState`'s two defaults (`main.ps1:790`, `:793`, per S8c2-2).
The `runtime-host` tests carry it as a fixture as well
(`framework-packages/runtime-host/src/mcp.rs:1093`). Changing the sample default means editing five
literals across two languages. Fix: one declared default in the package manifest, read by both sides.

**S8d1-11 (P3) — CSS grid column counts are hardcoded against JavaScript array lengths.** `:228`
declares `repeat(8,minmax(0,1fr))` for the interval strip, which happens to equal
`INTERVALS.length === 8`, while `:237` declares `repeat(7,…)` for the 13 entries of `PERIODS`, so the
period row silently wraps to two lines. Adding one interval breaks the interval strip's alignment
with no error anywhere. Related: `isIntradayPeriod:104` calls `.indexOf` on its argument with no
string guard, so it throws on a null period — safe today only because every call site
(`:548`, `:759`, `:1120`, `:1123`) passes the allowlisted `periodOf(state)`. Fix: generate the column
count from the array length via a CSS custom property.

Confirmed correct in this slice:

- Both enumerations arriving from state are allowlisted with a safe default: `viewOf:94` against
  `VIEW_IDS` falling back to `"full"`, and `periodOf:95-98` against `PERIOD_VALUES` falling back to
  `"minute"`. `pendingPeriod` likewise only accepts an allowlisted value out of the emitted payload
  (`:408-410`).
- `asObject:85` rejects arrays and non-objects and `asNumber:86-89` rejects `NaN`/`Infinity`, so an
  unexpected state shape degrades to `"--"` instead of throwing mid-render.
- Every value interpolated into `markup` comes from a frozen constant table (`:336-342`); no upstream
  text reaches the template, so the `data-interval-value` / `data-period-value` attributes cannot
  carry injected content.
- The red-up/green-up market convention is implemented per market rather than globally, and the
  reason is recorded in a comment at `:191`: `RED_UP_MARKETS` (`:35`) drives `paletteFor:193-195`,
  and both `movementColor:196-198` and `deltaColor:199-203` take the palette as a parameter.
- `emitAction`'s rejection rollback (`:432-448`) clears every flag and timer it had set and
  re-renders, so a host refusal cannot leave the tick or pending latch stuck.
- `"use strict"` inside an IIFE (`:1-2`) with `Object.freeze` on all constant tables means the script
  leaks no globals and cannot have its tables mutated by another script in the same realm.
- `formatVolume:150-156` uses the 万/亿 units a Chinese-market reader expects, with the thresholds in
  the right order so `1e8` is not reported as `10000 万`.

### S8d2 — Stock-monitor Surface: rendering and chart drawing

Owner of every fix in this slice: **Lane B / F5 and F6** (same reserved file as S8d1; S8d2-9 is fixed here, not in F7). **All fixed 2026-08-21 — records `### F5`, `### F6` and, for S8d2-9, `### F7` in `phase-78-lane-sync.md`.**

Scope: `art-packages/samples/stock-monitor/surface/main.js:451-900` — the two request helpers
(`:452-465`), the polling planner `refreshPlan`/`setRefreshTimer` (`:467-504`), the four DOM
updaters (`updateMetrics:506`, `updateHistoryTable:534`, `updateFavorites:563`,
`updateOrderBook:643`), the order-book helpers (`:602-642`), and the canvas renderer
`drawChart:707-888` with `overlayContext:890-898`.

**S8d2-1 (P2) — one rejected tick permanently downgrades the second-level channel into a full
refresh at the same cadence.** `emitAction` sets `tickSupported = false` when the host declines a
tick (`:435`), and nothing ever sets it back to `true`. `requestTick:457-458` then delegates straight
to `requestRefresh()` on every subsequent call, while `setRefreshTimer` keeps firing at the tick
cadence (`:496`) because `plan.usesTick` was computed from the interval, not from `tickSupported`. So
after a single transient rejection a 1-second interval issues a *full* snapshot — four MCP calls,
history included — once per second, which is precisely what the comment at `:456` and the separate
`FULL_REFRESH_SECONDS` channel exist to avoid. Fix: fold `tickSupported` into `refreshPlan` so an
unsupported tick also raises the cadence, and re-probe support on the next full refresh instead of
latching for the lifetime of the surface.

**S8d2-2 (P2) — while ticking, the 60-second channel that refreshes the K-line is systematically
starved.** `setRefreshTimer` arms both intervals in the same turn (`:494` at `plan.cadence`, `:501`
at 60 s), and every tick-capable cadence — 1, 3, 5, 15, 30 — divides 60 exactly. Their firings
therefore coincide, the tick timer was created first so it runs first, it sets `tickPending`, and the
full-refresh callback's guard at `:502` (`if (!pending && !tickPending)`) skips. The result is that
the slow channel described by the comment at `:500` almost never runs: the price updates every
second and the candles stop advancing. Fix: promote every *N*-th tick to a full refresh using the
`liveTickCount` counter already maintained at `:462`, rather than racing two independent timers, or
offset the slow timer by half a cadence.

**S8d2-3 (P2, performance) — every render rebuilds the whole DOM subtree and reallocates the canvas
backing store.** All four updaters begin with `replaceChildren()` and recreate every node:
`updateMetrics:507-519` (8 cells, 16 elements), `updateHistoryTable:536-558` (9 rows × 6 spans),
`updateFavorites:565-599` (up to 8 cards × 4 elements), `updateOrderBook:650-704` (up to 20 book rows
× 3 elements plus 6 tape items × 2). That is roughly 200 elements created and thrown away per
render. `drawChart` then assigns `canvas.width`/`canvas.height` unconditionally at `:721-722` and
mirrors them onto the overlay at `:723-726` — and assigning those properties reallocates the backing
store and clears the surface even when the dimensions are unchanged. At the clamped maximum
(2048×1024 CSS pixels with `ratio = 1.414`, so 2896×1448 device pixels) that is about 16 MB of
freshly allocated pixel buffer per canvas per render. At the 1-second cadence, with `render` also
called twice on `emitAction`'s rejection path, this is the dominant cost of the panel. Fix: assign
the canvas dimensions only when they change, diff the metric/tape/book rows in place instead of
rebuilding, and gate the four updaters on the state slice each one reads.

**S8d2-4 (P3) — time order is trusted from array position, and the runtime does not sort.** The
history array is truncated by position upstream (`-Last 2000` with no sort, S8c1-6) and the Surface
compounds it: `:541` takes `history.slice(-8).reverse()` and labels it `"最近 8 条"` (`:559`), and
`drawChart` plots `points` in array order (`:788-816`) while writing the first and last element's
dates as the x-axis range (`:864`, `:866`). If the provider ever returns rows out of order the table
shows the wrong eight rows, the polyline zigzags backwards in time, and the axis labels describe a
range that is not monotonic. Fix: sort by timestamp once when ingesting history, then truncate.

**S8d2-5 (P3) — the 均价 line and the 均价 figure are computed two different ways and will
disagree.** For intraday periods `:826-830` draws a yellow line that is the running mean of the
*downsampled bucket closes* (`:762` reduced the series to at most 240 buckets), while the tape prints
the provider's own `avgPrice` (`:636`). The market's 均价 is the volume-weighted average price, which
neither matches. So the legend's 均价 line and the tape's 均价 number sit on the same screen showing
different quantities. Fix: draw the provider's average when it is supplied, or relabel the line as a
simple moving average.

**S8d2-6 (P3, performance) — MA5 is recomputed with three allocations per point.** `:831-835`
does `points.slice(index - 4, index + 1).map(...).reduce(...)` for every point — three intermediate
arrays per point, up to 240 points per draw, for a five-term window. The intraday branch immediately
above (`:827-829`) already demonstrates the rolling-accumulator idiom. Fix: keep a rolling sum and
subtract the departing term.

**S8d2-7 (P3) — the depth bar's sell side is inferred rather than read, so bar and text can
disagree.** `:675-681` clamps `buyPercent` into `buyShare` and then sets the sell width to
`100 - buyShare`, ignoring `sellPercent` — which `:668` prints verbatim in the same widget. When the
provider's two values do not sum to 100 (rounding, or a provider that reports them over a different
base) the bar and the text describe different splits. Both are also consumed as percentages with no
unit check, so they inherit the percent-versus-fraction ambiguity of S8c1-7: fractions would render
a 0.55%-wide bar. Fix: compute both widths from their own value after normalizing the pair, and
assert the unit.

**S8d2-8 (P3) — the closed-market anti-spin floor can be defeated by the upstream.** `:473` raises
the cadence to at least `CLOSED_MARKET_MIN_SECONDS = 30` only when `marketStatus !== "open"`, and
S8c2-7 showed the runtime lets the provider's `isTrade` flag force `marketStatus` to `"open"` at any
hour, with S8c1-2 making the freshness gate on that path trivially true. Composed, a provider that
leaves `isTrade` set keeps the panel polling every second around the clock — the exact behaviour the
comment at `:472` says the floor prevents. Fix once at the runtime (S8c2-7) and keep the floor as
defence in depth.

**S8d2-9 (P3) — the runtime's staleness verdict is computed and then never displayed.** The Art emits
`stale`, `ageSeconds` and `maxAgeSeconds` for both the order book and the live tape (`main.ps1:477-479`,
`:512-518`), but the Surface renders only `formatClock(observedAt)` (`:672` for the book, `:688` for
the tape-only branch) and never reads `stale`. A record the runtime has explicitly judged stale is
therefore presented identically to a fresh one, leaving the user to compare a wall-clock string by
eye. Second instance of a computed signal being dropped at the boundary (S8c2-4 drops
`historyError`). Fix: badge the widget when `stale` is true.

**S8d2-10 (P3) — `refreshPlan` reads module state implicitly and the armed timers capture a stale
plan.** `:471` falls back to `stateOf(snapshotValue).marketStatus` when the caller omits it, so
`effectiveIntervalSeconds:483` silently depends on whichever snapshot is current, and the `plan`
object is captured by both interval closures (`:494-503`) and never re-read. Nothing inside those
callbacks notices a market that has since opened or closed; correctness depends entirely on some
caller comparing `plan.key` (`:477`) and re-arming. That contract is real but undocumented and lives
in another slice. Fix: pass `marketStatus` explicitly and re-derive the plan inside the callback, or
assert the re-arm contract where the key is compared.

**S8d2-11 (P3) — row and level counts are hardcoded in four places across two languages, and level
`0` is treated as missing.** The eight-row history window appears as `slice(-8)` (`:541`), the pad
loop's `< 9` (`:556`), the literal `"最近 8 条"` (`:559`), and the CSS `repeat(8,minmax(0,1fr))`
(`:291`); the favorites cap is `slice(0, 8)` (`:566`) against a two-column grid. In
`renderBookSide`, `asNumber(level.level) || index + 1` (`:617`, `:628`) renders a provider level of
`0` as `买1`, and the same expression plus `formatNumber(level.price, 2)` and
`formatVolume(level.volume)` are each evaluated twice per row (`:617-630`). Fix: one constant per
window size, referenced from both the JS and a CSS custom property, and compute each row's label
once.

Confirmed correct in this slice:

- Every string that reaches the DOM in this slice goes through `textContent` or `title` —
  `updateMetrics:513/516`, `appendHistoryRow:527`, `updateFavorites:584-596`,
  `renderBookSide:617-630`, `updateOrderBook:662-701` — so upstream-controlled names, codes and dates
  cannot inject markup here. (`innerHTML` does appear in this file, but only at `:1014`, `:1120` and
  `:1226`, which S8d3 covers.)
- The canvas pixel cap actually holds: `:716-717` clamp CSS dimensions to 2048×1024, so
  `width * height` never exceeds `MAX_CANVAS_PIXELS`, `pixelRatio` at `:719` is therefore always
  ≥ 1.414, and the `Math.max(1, …)` at `:720` never has to defeat the cap. Device ratio is
  independently clamped to 2 (`:718`).
- `drawChart` handles the degenerate cases explicitly: fewer than two points draws a waiting label and
  clears the geometry (`:763-771`), and a flat series (`minimum === maximum`) is widened before the
  scale divides by the range (`:775-778`), so `yAt` cannot divide by zero.
- `bookLevelsOf:602` filters out levels with an unparseable price before rendering, so
  `renderBookSide`'s arithmetic at `:623` cannot see a null.
- `orderBookOf:603-607` returns null unless at least one side has levels, which is what lets
  `updateOrderBook` distinguish "no depth for this market" (Hong Kong) from "depth not yet loaded"
  and show the tape-only layout at `:686-689`.
- `setRefreshTimer` clears both timers before re-arming (`:489-492`) and refuses to arm at all when
  disposed or suspended (`:493`), so re-entrant calls cannot leak intervals.
- `refreshPlan` allowlists the interval against `INTERVALS` with `DEFAULT_INTERVAL_SECONDS` as the
  fallback (`:468-470`), so a corrupt stored interval cannot produce a zero or negative period.
- The volume-bar and candle colours are taken from the market palette rather than a global up/down
  convention (`:789`), consistent with the red-up handling verified in S8d1.

### S8d3 — Stock-monitor Surface: event wiring, action dispatch, teardown

Owner of every fix in this slice: **Lane B / F5 and F6** (same reserved file as S8d1).

Scope: `art-packages/samples/stock-monitor/surface/main.js:901-1264` — the overlay and hover layer
(`:900-1033`), `render:1035-1133`, `bindEvents:1135-1166`, the teardown helpers
(`clearScheduledWork:1168`, `cleanup:1182`), the test hooks (`:1196-1217`), and the
`NeuroSurface.define` lifecycle (`:1219-1263`).

**S8d3-1 (P2) — upstream text reaches `innerHTML` unescaped in two places.** The chart tooltip is
assigned with `refs.tip.innerHTML = buildTipContent(geometry, index)` (`:1014`), and
`buildTipContent`'s title row interpolates the point's date directly:
`'<div class="chart-tip-title">' + formatPointDate(point.date, geometry.intraday) + '</div>'`
(`:956`). That date is whatever the provider sent — `chartRowsOf:108` keeps it as
`text(row.date, "")` with no character validation, and `formatPointDate:204-209` only does
`String(...).replace("T", " ").trim()` and a `slice`. Nothing escapes `<` or `&`. The only reason
this is not script execution today is the incidental length cap: `slice(0, 16)` for intraday and
`slice(0, 10)` otherwise, which is too short for a working `<svg onload=…>` payload — but it is
long enough to inject an element and break out of the title `div`, and the cap is a formatting
detail that any future change to the date format would lift. The legend assignment at `:1120-1122`
is worse in principle: it interpolates `periodLabel`, and `periodLabelOf:99-103` prefers
`state.periodLabel` verbatim over the closed `PERIODS` table with **no length cap at all** — safe
today only because the runtime derives that label from its own allowlist
(`main.ps1:222-241`), i.e. safety depends on a property of the other process. Fix: build both
fragments with `document.createElement` + `textContent` like every other updater in this file already
does, or escape at minimum; and validate `state.periodLabel` against `PERIODS` instead of trusting it.

**S8d3-2 (P2) — clicking an interval while a refresh is in flight unlocks the refresh button and
allows two concurrent refreshes.** Interval buttons are never disabled (`:1077-1081` toggles only the
active class, unlike `:1086` for periods and `:1088`/`:1089` for the two refresh buttons), and
`emitAction` deliberately treats `stock_interval_commit` as non-network so it bypasses the `pending`
guard (`:392-394`). The runtime's interval-commit branch then writes a state patch and returns
without fetching (`main.ps1:880-895`), which bumps the instance revision. Back in `render`, the
unlock condition is `revision > pendingRevision` with no check that the new revision belongs to the
pending action (`:1039-1047`), so the interval commit's revision clears `pending`, re-enables the
refresh button and the period buttons, and the still-running first refresh is forgotten. A second
click now emits while the first is outstanding, and whichever response lands last wins. Fix: track
the pending action's own correlation id (or have the host echo which action produced the revision)
instead of inferring completion from any revision bump.

**S8d3-3 (P3) — `resume()` never re-renders, so stale status text survives suspension.** `suspend`
clears `pending` and the timers (`:1242-1249`) but leaves the DOM exactly as the last render wrote
it — including a `"正在刷新"` status and disabled buttons. `resume` (`:1250-1255`) re-arms the timer
and redraws the canvas but does not call `render`, so the stale status line and the disabled controls
persist until the host happens to push an update. Fix: call `render(snapshotValue)` in `resume`.

**S8d3-4 (P3, performance) — the resize observer is unthrottled and `drawChart` is expensive.**
`resizeObserver = new ResizeObserver(drawChart)` (`:1229`) invokes the full redraw on every observed
size change, and per S8d2-3 each call reassigns `canvas.width`/`height` and reallocates up to ~16 MB
of backing store for two canvases. Dragging a window edge therefore triggers dozens of full
reallocations per second. The file already contains the fix idiom: `handleChartPointer:1027-1032`
coalesces into a single `requestAnimationFrame`. Fix: route the observer through the same coalescing.

**S8d3-5 (P3) — test-only affordances ship in the production script, gated on a writable global.**
`:1196-1217` reads `globalThis.__LOOM_STOCK_MONITOR_TEST_HOOKS__` and, if present, installs handles
that mutate real module state — `beginTick` fabricates `snapshotValue` outright
(`:1208-1210`) and sets the tick latch. Any script that can define that global before this one runs
gains a handle on the surface's internals. This is only safe because the host isolates each surface
in its own realm; the script itself does not check. Fix: strip the block at package time, or require
a host-provided capability token rather than a bare global.

**S8d3-6 (P3) — the tick-latch test exercises a copy of the logic, not the shipped path.**
`applyRevision` (`:1199-1206`) reimplements the same four statements `render` uses to clear the tick
latch (`:1048-1053`). A regression in `render`'s copy leaves the test green. Fix: have the hook call
`render` with a synthetic snapshot, or extract the latch logic into one function both call. Queued
for S9 as a coverage gap.

**S8d3-7 (P3) — pressing Enter in the symbol field emits the commit twice.** The `keydown` handler
emits and then calls `refs.symbol.blur()` (`:1138-1143`); the blur fires the input's `change` event,
whose handler emits the same action again (`:1144-1146`). The duplicate is swallowed only because the
first emit set `pending`, so `emitAction` returns false at `:394` — silently, per S8d1-9. The
correctness of a user-visible path therefore rests on two incidental behaviours. Fix: remember the
last committed value and skip the `change` handler when it is unchanged.

**S8d3-8 (P3) — the surface installs document-global styles and unscoped class names.** `mount`
appends its sheet to `document.adoptedStyleSheets` (`:1223-1225`), and `styleSource` opens with
`:root{color-scheme:dark}` and `html,body{background:transparent}` (`:212-213`) — rules that mutate
the host document, not just this widget. Every selector is a bare `.stock-*` / `.quote-*` /
`.book-*` class. Two instances in one document would install two copies of the sheet and share the
class namespace. This is safe only under the host's per-surface isolation; nothing in the script
depends on it explicitly. Fix: attach to a shadow root, or scope the rules under the shell's own
`[data-ref="shell"]`.

**S8d3-9 (P3) — the mount-time auto-refresh uses a falsy price check.** `:1232` reads
`if (!quoteOf(stateOf(snapshot)).price)`, so a legitimate price of `0` — a halted name, or an
instrument that genuinely prints zero — is treated as "no data" and schedules an extra fetch 80 ms
after mount. Every other price test in the file goes through `asNumber(...) !== null` (`:1068`,
`:647`). Fix: use the same predicate.

**S8d3-10 (P3) — optimistic UI is applied without reconciliation.** `displayedPeriod =
pendingPeriod || period` (`:1064`) highlights a period the host has not accepted; if the commit is
lost the highlight stays wrong for the full `PENDING_TIMEOUT_MILLIS` window (52 s, S8d1-1) and then
reverts with only a status-line message. Symmetrically, `:1076` overwrites `refs.symbol.value` from
state whenever the input is not focused, so an edit the user made and then clicked away from without
committing is silently reverted. Fix: reconcile the optimistic period against the state on every
render and only revert when the host confirms a different value.

**S8d3-11 (P3) — `render` recomputes everything unconditionally and the two latches are cleaned up
in different places.** Every update walks all eight interval buttons and all thirteen period buttons
through `querySelectorAll` (`:1077-1087`), rewrites every text node, and calls all four updaters plus
`drawChart` (`:1127-1132`), with no dirty-checking of the state slices each one reads — the
structural cause of S8d2-3. Separately, `clearScheduledWork:1168-1180` resets `tickPending` and
`tickPendingRevision` but not `pending`, `pendingAction` or `pendingRevision`, which `suspend` has to
reset itself (`:1244-1246`) and `dispose` never resets at all. Fix: gate each updater on a cheap
comparison of the slice it renders, and reset both latches in one place.

Confirmed correct in this slice:

- Both delegated click handlers validate before emitting: `closest("[data-…-value]")`, an
  `instanceof HTMLButtonElement` check, and membership in `INTERVALS` / `PERIOD_VALUES`
  (`:1147-1160`), so a stray click target cannot produce an out-of-band payload.
- `mount` returns `cleanup` (`:1237`), and `cleanup` clears all four timers, cancels the pending
  animation frame, hides the tip, drops the geometry, disconnects the `ResizeObserver`, and removes
  its own stylesheet from `document.adoptedStyleSheets` (`:1168-1194`) — nothing leaks on dispose.
- The first render arms the polling timer through the `plan.key !== activeTimerKey` comparison at
  `:1131` (with `activeTimerKey` starting as `""`), so there is no separate arming path to keep in
  sync — and that same comparison is what re-arms when the market opens or closes, which is the
  contract S8d2-10 depends on.
- `handleChartPointer` coalesces `pointermove` into a single `requestAnimationFrame` and re-checks
  `disposed` inside the callback (`:1027-1032`); `indexAtPointer` clamps the pointer into the plotted
  range (`:995-997`) so hovering the padding cannot index out of bounds.
- Every numeric value in the tooltip passes through `formatNumber`/`formatSigned`/`formatVolume` and
  every colour comes from the frozen palette (`:945-971`), so the date is the *only* unescaped value
  in that fragment — which is what makes S8d3-1 a bounded fix rather than an audit of the whole tip.
- `drawCrosshair` saves and restores the canvas state around its dash and alpha changes
  (`:922-942`), so overlay drawing cannot leak state into the next frame.
- `render` re-establishes `snapshotValue` before anything else reads it (`:1037`), and every accessor
  it uses (`stateOf`, `quoteOf`, `historyOf`) tolerates a missing state object.

### S9 — Performance and coverage gaps

Scope: what the two repositories do and do not verify automatically. Evidence is the checked-in CI
definitions (`Loom/.github/workflows/ci.yml`, `Hook/.github/workflows/*.yml`), the package
manifests, and test counts taken from the sources.

**S9-1 (P2) — Loom has no performance gate of any kind; Hook does.** Hook ships
`.github/workflows/runtime-performance.yml` with a weekly schedule, `npm run test:performance`,
native mouse-queue stress tests, a real WebGL shader gate with explicit budgets
(`HOOK_SHADER_BENCH_MAX_REHYDRATION_P95_MS: 10000`,
`HOOK_SHADER_BENCH_MAX_ADJUSTMENT_P95_MS: 3000`) and an optional process soak. Loom's `ci.yml` has
no performance step at all, and `grep criterion --include=Cargo.toml` across the workspace returns
nothing — there is not one benchmark in 25 crates. Every performance finding in this review can
therefore regress silently: S8a2a-8 (quadratic aggregation), S8a2a-9 (quadruple cloning), S8a2b-12
(whole-body buffering), S8b1-7 (base64 duplication), S8b2-9 (decode-for-dimensions plus double file
read), S8c2-1 (triple serialization of one payload), S8d1-2 (per-render series re-derivation),
S8d2-3 (DOM and canvas churn), S8d2-6 (MA5 allocation) and S8d3-4 (unthrottled resize redraw). First
budgets worth pinning, in the order the review would trust them: end-to-end art execution wall time
for the sample package, peak resident memory for one framework process, and response bytes for a
single surface action. **Closed: response bytes by F9a 2026-08-22, peak memory by F9b 2026-08-22,
wall time by F9c 2026-08-22.**

**S9-2 (P2) — `framework-packages/runtime-host` is never built or tested by CI.** It is a detached
manifest, so `cargo check --locked --workspace --all-targets` (`ci.yml:56`, `:109`) and
`cargo test --locked --workspace` (`:59`, `:112`) both skip it, and it appears in no workflow — the
only reference anywhere is `scripts/Build-LoomArtFrameworkPackages.ps1:45`, i.e. package time. Its
`src/mcp.rs` is 1352 lines holding the MCP bridge, the Surface binding root check
(`mcp.rs:357-372`) and `value_is_within_depth` (`:703-716`) — the bounded-recursion primitive this
review cites as the fix for S6b2d3-2, S8b1-2 and S8c1-1 — plus 11 inline tests that no CI job runs.
A compile break or a regression in either guard surfaces only when someone rebuilds the packages by
hand. Fix: add `cargo fmt --check`, `cargo check --locked` and `cargo test` steps for that manifest
to `ci.yml`, next to the two the Tauri wrapper already has.

**S9-3 (P2) — the Tauri wrapper is checked and formatted but never tested.** `ci.yml:74` runs
`cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml` without `--all-targets`,
and `:77` runs `cargo fmt`. There is no `cargo test` for that manifest, and because `--all-targets`
is absent its test code is not even compiled — a test that no longer builds would pass CI silently.
The workspace check two steps earlier does pass `--all-targets`, so the omission is inconsistent
rather than deliberate.

**S9-4 (P2) — three Hook npm scripts exist and are executed by nothing.** Owner: **Lane B / F4**, taken 2026-08-21. `package.json` defines
`lint` (`eslint src`), `typecheck:test` (`tsc --noEmit -p tsconfig.test.json`) and
`test:surface-browser` (`node scripts/run-javascript-surface-browser-smoke.mjs`). None appears in
`build-hook-exe.yml`, `release-hook-tag.yml` (which runs only `audit:licenses` and `typecheck`),
`runtime-performance.yml` or `signpath-signing.yml`, and none is in the documented local gate
`verify:local`. So Hook has an ESLint configuration that never runs, test files that are never
typechecked, and — most relevant here — the only browser-level exercise of the JavaScript surface
bootstrap is opt-in. That smoke harness is exactly what would have caught the `innerHTML` sink in
S8d3-1. Fix: add all three to `build-hook-exe.yml` and to `verify:local`.

**S9-5 (P2) — the modules this review flagged as security-relevant are the least covered.** Test
counts against file length: `crates/loom_mcp/src/package.rs` 532 lines / 2 tests — the archive
extraction, signature and size branches S7a-10 called out; `apps/daemon/src/surface_resources.rs`
590 / 2; `apps/daemon/src/surface_actions.rs` 2370 / 7, including the `authoritativeState` copy at
`:403` that three findings depend on. Compare `crates/loom_mcp/src/lib.rs` 1733 / 19 and
`crates/loom_tool_registry/src/framework_process.rs` 1644 / 14, which are adequately covered. Named
gaps with no test at all: remote-binary download, signed-plus-remote-binary install, and the
framework-path candidate-shape resolution.

**S9-6 (P3) — `apps/daemon/src/lib.rs` is 28,225 lines with 150 tests inside it.** One translation
unit that every daemon change recompiles, and the reason this review had to slice the daemon by line
range rather than by responsibility. Instance-state persistence (`:4671`, `:6511`) sits in the same
file as everything else, while the neighbouring `surface_actions.rs` and `surface_resources.rs` show
the extraction pattern already exists — it is simply unfinished. Fix: continue that split, starting
with the instance-state and surface-dispatch area, so the parts three findings touch can be tested
in isolation.

**S9-7 (P3) — Loom's desktop test command only discovers one directory.**
`apps/desktop/package.json` has `test :: node --test src/services/*.test.ts`: a single non-recursive
glob over `src/services`. A test added under `src/components/`, or nested one level deeper in
`services/`, is silently never run, and `ci.yml:67` invokes exactly this command. Hook uses vitest
with real discovery. Fix: `node --test` with a recursive pattern, or move to the same runner Hook
uses.

**S9-8 (P3) — Loom has exactly one integration test file.**
`apps/daemon/tests/daemon_cli_contract.rs` is the whole of `tests/` across 622 tracked files;
everything else is inline `#[cfg(test)]`, which tests crates through their internals rather than the
public API a consumer sees. The PowerShell contracts do cover the release layout, tamper detection,
run persistence, daemon concurrency and the sample art package (`ci.yml:42-46`, `:83-85`), but
nothing drives an art plus its MCP dependency end to end the way the stock-monitor sample does at
runtime — which is why S8b through S8d had to be reviewed by reading rather than by running.

**S9-9 (P3) — two known inefficiencies also weaken test fidelity.** `tools/list` is re-fetched on
every execution with no warm client (S7c2-7), so both latency and the tool-shape assumptions are
re-derived per run; and `build_arguments` (`framework_process.rs:730-733`) is a `#[cfg(test)]`-only
shim, meaning the argument construction the tests validate is not the code path production takes.
Fix the shim first — a test that exercises a parallel implementation is worse than no test, because
it reports confidence it has not earned (same defect as S8d3-6).

**S9-10 (P3) — the review's two recurring fixes are both blocked on missing structure.** The
hardened zip extractor (`secure_zip`) and the outbound URL validator (`network_policy`) live above
`loom_mcp` in the dependency order, and `crates/loom_tool_registry/Cargo.toml:14` makes
`loom_tool_registry` depend on `loom_mcp`, so the reverse import needed by S7a-1, S7b2-1 and the
outbound half of S8b1-1 / S8b2-1 is impossible. Likewise the bounded-recursion helper wanted by
S6b2d3-2, S8b1-2 and S8c1-1 exists only in the crate CI does not build (S9-2). Recommendation, and
the single structural change of this review: create a leaf `loom_security` crate holding the zip
extractor, the URL/UNC policy (S8b2-1 proves it must cover SMB and UNC paths, not only `http`/
`https`) and the depth guard, and have both `loom_mcp` and `loom_tool_registry` depend on it. One
relocation unblocks five findings, so it should be the first change of the fix phase.

**S9-11 (P3) — nothing tests the PowerShell version the arts actually run under.** Entry points are
spawned through a hardcoded `powershell.exe`, i.e. Windows PowerShell 5.1 rather than `pwsh` 7
(S7b1-5), and 5.1-specific behaviour underpins several findings: `ConvertFrom-Json` is
`JavaScriptSerializer`-backed with a ~2 MB `MaxJsonLength` and ~100-level recursion, it yields
`PSCustomObject` and never `IDictionary` (making the `-is [IDictionary]` branches dead code), and
`ConvertTo-Json` escapes non-ASCII as `\uXXXX` while a spawned child's `[Console]::Out` defaults to
the OEM code page (cp936 here). CI runs the sample art contract exactly once (`ci.yml:85`), on
whatever `powershell` resolves to. Fix: run that contract as a 5.1-plus-7 matrix, or pin the host to
one interpreter and document it.

What CI does well, for balance: the clean-host malicious-plugin matrix runs six independent cases
(`archive`, `signature`, `dependency`, `network`, `process`, `lifecycle`) on a fresh runner
(`ci.yml:114-139`); the plugin CLI has a real sign/trust/pack/install/revoke end-to-end test plus
schema validation for all five public schemas (`:141-164`); the Rust workspace is validated on both
Windows and Linux; release integrity has a dedicated tamper test (`:44`); and Hook's performance
work is genuinely budgeted rather than advisory. The gaps above are omissions at the edges of an
otherwise serious pipeline, not an absence of one.

## Fix plan

The review closed with 307 findings across 34 slices. They are not all worth fixing, and
fixing them in one pass would be reckless, so the fix phase runs as its own sequence of
small batches with the same rule as the review: one batch per task, verified and recorded
before the next starts.

Triage rule: every P2 gets fixed. A P3 gets fixed when it is local, cheap, and in a file a
P2 already opens; otherwise it is left recorded as accepted backlog with the reason. No
finding is silently dropped — anything not fixed is listed in the batch that touched its
file.

Batch order is chosen so earlier batches unblock later ones, not by severity alone.

| Batch | Scope | Findings addressed | Depends on |
| --- | --- | --- | --- |
| F1 | New leaf crate `loom_security`: hardened zip extraction, outbound URL/UNC policy, bounded-recursion depth guard. Rewire `loom_mcp`, `loom_tool_registry` and `framework-packages/runtime-host` onto it. | S9-10 (structural) | — |
| F2 (Lane A) | Apply the F1 primitives at their call sites. **S8c1-1 moved out of this batch to F7 / Lane B** — see "Parallel lanes" below. | S7a-1, S7b2-1, S8b1-1, S8b2-1, S6b2d3-2, S8b1-2 | F1 |
| F3 (Lane A) | Loom CI: add `fmt`/`check`/`test` for the `runtime-host` manifest, add `cargo test` plus `--all-targets` for the Tauri wrapper, make the desktop test glob recursive. | S9-2, S9-3, S9-7 | — |
| F4 (Lane B) | Hook CI: add `lint`, `typecheck:test` and `test:surface-browser` to `build-hook-exe.yml` and to `verify:local`, then fix what they report. | S9-4 | — |
| F5 (Lane B) | Stock-monitor Surface correctness: escape or DOM-build both `innerHTML` sinks, correlate pending state with the action that produced a revision, negotiate the client timeout against the host budget, reset the tick-capability latch, stop the full refresh from being starved by ticks, re-render on resume. | S8d3-1, S8d3-2, S8d1-1, S8d2-1, S8d2-2, S8d3-3 | F4 (so the browser smoke gates it) |
| F6 (Lane B) | Stock-monitor Surface performance: memoize the derived series, dirty-check the four updaters, keep the canvas backing store when the size is unchanged, coalesce resize redraws, make MA5 a rolling accumulator. | S8d1-2, S8d2-3, S8d2-6, S8d3-4 | F5 |
| F7 (Lane B) | Stock-monitor runtime `main.ps1`: serialize the payload once, stop echoing a rejected action's text into stored state, truncate without splitting surrogate pairs, make the staleness verdict single-directional and surface it, consume `historyError`. | S8c2-1, S8c2-2, S8c2-3, S8c1-2, S8c1-3, S8c2-4, S8d2-9 | — |
| F8 (Lane A) | Sweep S1 through S7c2 for every remaining P2 and fix it. The list is in the per-slice sections above and must be re-read at fix time rather than recalled. **Excludes the four Hook-side P2s S1-2, S2-1, S2-2 and S3-3, which Lane B owns.** | all remaining P2 | F1, F2 |
| F9 (Lane A) | First performance budgets for Loom: end-to-end art execution wall time for the sample package, peak resident memory for one framework process, response bytes for one surface action. | S9-1 | F3, F6, F7 |
| F10 (single owner, last) | Full verification in both repos, then the two release builds. | — | all |

Batches F1, F3, F4 and F7 are independent of each other and may run in any order.

### Parallel lanes — F4, F5, F6, F7 and the Hook-side P2s are taken by Lane B

Recorded 2026-08-21 by the second agent. The fix phase now runs as two lanes worked in
parallel by two different agents. This section is the ownership boundary: read it before
opening any file listed in it, and change it before crossing it.

| Lane | Owner | Batches | Reserved paths | Build lock |
| --- | --- | --- | --- | --- |
| A | the agent that shipped F1 and is mid-F2 | F2, F3, F8, F9 | Loom `crates/**`, `framework-packages/**` (**except** `framework-packages/runtime-host/src/mcp.rs`, lent to Lane B for F13 — see the note below), root `Cargo.toml` / `Cargo.lock`, Loom `.github/**`, `art-packages/samples/image-search/**`, `art-packages/shared/**`, `apps/desktop/**` | owns Loom's `cargo` and `target/` |
| B | second agent, joined 2026-08-21 | F4, F5, F6, F7, F13, plus the Hook-side P2s S1-2 (Hook half), S2-1, S2-2, S3-3, and the Hook-side P3 S3-4 (claimed 2026-08-22 — it belonged to no batch) | the whole `Hook/` repository, Loom `art-packages/samples/stock-monitor/**`, `mcp-server-packages/**`, `docs/progress/phase-78-lane-sync.md`, plus `framework-packages/runtime-host/src/mcp.rs` while F13 is open | Hook `npm` and Hook `src-tauri` cargo only — a separate tree with its own `target/`, plus the detached `framework-packages/runtime-host` manifest (its own `[workspace]` and `target/`, so it does not take the Loom workspace lock) |

Boundary loan, 2026-08-22 (F13): `framework-packages/runtime-host/src/mcp.rs` is held by Lane B
for the duration of F13, which claims the two findings in that file that belonged to no batch —
S7c1-1 and S7c2-1. Scope of the loan is that one file plus its in-file `#[cfg(test)]` module;
`framework-packages/runtime-host/Cargo.toml` and `Cargo.lock` stay with Lane A and were
deliberately left untouched (the lock currently carries Lane A's uncommitted `loom_security`
edit, and `crates/loom_security/` is still untracked, so committing it would break a clean
checkout). That constraint shaped the S7c1-1 fix: no new dependency, therefore no `semver` crate
in runtime-host. See H11 in the lane-sync document.

Why the split is drawn here and not by batch number: two agents in one Cargo workspace
serialize on the `target/` lock, and workspace verification is where most of the wall time
goes, so halving F8 would buy almost nothing. Hook is a separate repository with its own
`target/`, and the stock-monitor Art layer is PowerShell plus browser JavaScript with no
cargo step at all, so Lane B never contends for the lock. The dependency chain F4 → F5 → F6
also lands entirely inside Lane B, because F5's gate is Hook's `test:surface-browser`, which
F4 is the batch that wires up.

Two ownership corrections this forces:

- **S8c1-1 moves from F2 to F7, and therefore to Lane B.** It lives in
  `art-packages/samples/stock-monitor/runtime/main.ps1`, the file F7 rewrites, and its fix
  edits the same `Find-SurfaceAction` region as F7's S8c1-2 and S8c1-3 work. Lane A's F2
  keeps the other six call sites. Lane B implements the bounded-depth shape from
  `framework-packages/runtime-host/src/mcp.rs:703-716` locally in PowerShell; it cannot
  depend on `loom_security`, because PowerShell does not call the crate.
- **S8d2-9 stays with Lane B.** It is listed under F7 but lives in
  `art-packages/samples/stock-monitor/surface/main.js`, so it is fixed with F5 and F6.

Rules both lanes follow:

1. Do not edit a path reserved by the other lane. To cross the boundary, amend the table
   above first and say so in the sync document.
2. Commit with explicit paths (`git commit -- <path>`), never `git add -A`. The Loom working
   tree holds both lanes' work in progress at the same time, so a catch-all stage would
   commit the other lane's half-finished batch.
3. Lane B does not append to this document outside this section. Lane B's batch records go
   to `docs/progress/phase-78-lane-sync.md`, and whoever runs F10 merges them back here.
4. Lane B does not run `cargo` against the Loom workspace. If a stock-monitor change needs
   the sample-art contract test (`ci.yml:85`), Lane B requests a window in the sync document
   and runs that single test, not `--workspace --all-targets`.
5. F10 is single-owner and strictly last. `build-release.ps1 -RequireCleanSource` refuses on
   any dirty or untracked file, so both lanes must have committed first. Next version ids
   are `r76` for Loom and `r89` for Hook.

Cross-lane status, open questions and handoffs live in
`docs/progress/phase-78-lane-sync.md`. Both lanes update it at the start and end of every
batch.

### F1 — done

`crates/loom_security` now exists as a leaf crate with three modules and no `loom_*` dependency
of its own, so both sides of the `loom_tool_registry` → `loom_mcp` edge can enforce the same
rules:

- `archive` — the former `loom_tool_registry::secure_zip`, moved verbatim and made `pub`. Two
  tests were added while moving it, because the extractor's most important branches were the
  ones with no coverage (part of S9-5): `rejects_parent_directory_traversal` also asserts that
  nothing was written beside the destination directory, and
  `rejects_absolute_and_drive_qualified_entries` covers a drive-qualified entry name.
- `network` — the former `loom_tool_registry::network_policy`, moved verbatim, plus
  `validate_local_path`. That new function closes the gap S8b2-1 describes: a UNC path
  (`\\host\share\...`), its verbatim form (`\\?\UNC\...`), a forward-slash UNC and a Win32
  device path (`\\.\...`) all reach the network or a raw device without ever parsing as a URL,
  so `validate_outbound_url` never sees them. It is implemented over `std::path::Prefix` rather
  than string matching so that `\\?\C:\...` — a legitimate long local path — still passes.
- `json` — `value_is_within_depth` ported from `framework-packages/runtime-host/src/mcp.rs`,
  with the recursion depth of the check itself bounded by the budget, plus
  `ensure_within_limits` and `parse_within_limits` and the six named limit constants. The
  ported semantics are preserved exactly: depth is measured on the values inside containers, so
  an empty container is depth 0.

`loom_tool_registry` keeps its old public path through `pub use loom_security::network as
network_policy;`, so none of the 28 existing references had to change, and `secure_zip` is
re-exported at crate level as `pub(crate)`. `loom_mcp` gained the dependency so that F2 can
apply the primitives at its call sites.

Deferred within F1: `framework-packages/runtime-host` keeps its own copy of the depth guard.
It is a detached manifest that is deliberately built as a standalone artefact, and pointing it
at a workspace path dependency changes how it is packaged, which belongs with F3's work on
building and testing that manifest in CI rather than here.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --workspace --all-targets`
clean, `cargo test --locked --workspace` all green with the 16 new `loom_security` tests among
them.

### F2 — done

The six call sites this batch owns now use the F1 primitives. S8c1-1 is not part of this record:
it moved to F7 / Lane B when the lanes were drawn, and `stock-monitor/runtime/main.ps1` was left
untouched here.

- **S7a-1** — `loom_mcp::package::extract_package` no longer walks the archive itself. It calls
  `loom_security::archive::extract_zip_securely` and maps the error through
  `package_error_from_secure_zip`, so MCP packages get the traversal, symlink, absolute-name and
  produced-byte checks the Art extractor already had. The declared `MAX_PACKAGE_FILES` and
  `MAX_EXTRACTED_BYTES` checks were kept in front of it as an early reject, with a comment
  recording that the declared values are attacker-controlled and the real bound is the one the
  extractor enforces on bytes actually written.
- **S7b2-1** — remote MCP endpoints are validated before the client is built.
  `ensure_remote_scheme_allowed` allows `https`, refuses every other scheme, and gives a distinct
  message when credential headers are attached to a plain `http` URL, because that case leaks
  secrets rather than merely being unencrypted. `validate_outbound_url` then applies an
  `OutboundPolicy` whose loopback and private-network allowances are both off unless local servers
  are opted into, via `configure_local_servers` or `LOOM_MCP_ALLOW_LOCAL_SERVERS=1`, and
  `apply_runtime_proxy` is applied so the client cannot be sent through a proxy that would connect
  on its behalf to a host the policy never saw.
- **S8b2-1** — the host half is `loom_security::network::validate_local_path` (recorded under F1).
  The Art half is in `art-packages/shared/image-runtime-common.ps1`: `Get-RequestImageRoots`
  derives the roots a request actually grants — `artDir`, `context.cacheDir`, `context.tempDir`,
  `outputDir` and the runtime work root — and `Resolve-ImagePath` now takes them as
  `-AllowedRoots` and resolves through `Resolve-ConfinedImagePath`. That helper refuses anything
  starting `\\` or `//` via `Test-RemoteOrDevicePath` (the same rule as the host's), accepts only
  the host-less `file:///C:/...` spelling through `Convert-FileUrlToLocalPath` because .NET turns
  `file://localhost/...` and `file://host/share/...` into UNC paths, and requires the canonical
  result to sit under one of the granted roots by way of `Get-NormalizedPathRoots` and
  `Test-PathUnderRoots`. The four sample runtimes that read images — image-blend, image-compress,
  image-search and remove-bg — pass the roots through.
- **S6b2d3-2** — `collect_mcp_image_candidates_from_value` in `loom_tool_registry` now carries a
  `depth` and a shared `McpImageCandidateWalk` holding both the candidate list and the seen set,
  bounded by `MAX_MCP_IMAGE_CANDIDATE_DEPTH = 24` and `MAX_MCP_IMAGE_CANDIDATES = 64`. Every
  recursion site checks `walk.is_full()` afterwards and returns early, so a wide result stops
  costing work once the cap is reached rather than after the whole tree is walked. Embedded JSON
  strings are parsed through `parse_mcp_image_candidate_json`, which spends the remaining depth
  budget rather than restarting it — `loom_security::json::parse_within_limits` with
  `MAX_PROCESS_RESPONSE_BYTES` and `MAX_MCP_IMAGE_CANDIDATE_DEPTH - depth` — so nesting a document
  inside a string cannot buy back depth. A test,
  `mcp_image_candidates_stop_at_the_nesting_limit`, pins the boundary.
- **S8b1-2** — the same bound in the Art, as `$script:McpImageCandidateDepthLimit = 24` and
  `$script:McpImageCandidateLimit = 64` in `image-search/runtime/main.ps1`. It has to be a local
  implementation: PowerShell does not call the crate, and a PowerShell stack overflow cannot be
  caught, so the guard must stop the descent rather than recover from it.
- **S8b1-1** — the image-search Art no longer downloads whatever URL an MCP server names.
  `Resolve-ImageDownloadTarget` requires an absolute `http`/`https` URL, resolves the host, and
  refuses if *any* resolved address fails `Test-BlockedImageAddress`, all before the first socket
  is opened. `AllowAutoRedirect` is off and `UseProxy` is false; up to
  `$script:ImageDownloadRedirectLimit = 5` redirect hops are followed by hand, each one
  revalidated, and the MIME type is derived from the final URL. The extension check was never a
  control here — `http://127.0.0.1:8787/anything.png` satisfies it.

`Test-BlockedImageAddress` mirrors `loom_security::network::validate_ip` range for range, and
deliberately not one range further. An earlier draft also blocked CGNAT `100.64/10` and
benchmarking `198.18/15`; that broke every real download on the development machine, because a
fake-IP DNS resolver — how many machines reach the internet through a local proxy — answers
ordinary public hostnames out of that pool. `www.python.org` resolved to `198.18.0.79`. Widening the set past the host's therefore refuses ordinary public image hosts, and
the function's comment says so. IPv4-mapped IPv6 addresses (`::ffff:10.0.0.1`) are judged as the
IPv4 address they embed, since that is the address actually reached.

The known limitation is the host's as well: DNS is resolved once for validation and again by
`HttpClient`, leaving a rebinding window. Closing it would mean connecting to a pinned address and
carrying the `Host` header by hand, which the Rust side does not do either, so the Art matches the
host rather than diverging from it.

`scripts/tests/Test-LoomSampleArtRuntime.ps1` grew from 10 to 11 cases. The new one starts a real
`TcpListener` on a loopback port, hands the Art an MCP candidate naming
`http://127.0.0.1:<port>/candidate.png`, and asserts both that the runtime returns an error and
that `$loopbackListener.Pending()` is still false — that is, no connection was ever opened, not
merely that the response was discarded. `Pending()` was checked against a positive control first
to confirm it reports `True` when a connection does arrive, so the assertion is not vacuous.

Verified: the seven-case rejection probe returned `connected=False` for the loopback literal, the
loopback name, `169.254.169.254`, an RFC 1918 address, an IPv4-mapped loopback address and an
unresolvable host, while a `data:` URL still succeeded; two live cases,
`https://www.python.org/static/img/python-logo.png` and its `http://` form, both returned
`bytes=84503`, the second after a revalidated 301. `Build-LoomSampleArtPackages.ps1` built all
seven packages and `Test-LoomSampleArtRuntime.ps1` passed 11/11. On the Rust side
`cargo fmt --all -- --check`, `cargo check --locked --workspace --all-targets` and
`cargo test --locked --workspace` are clean, as are the two detached manifests
(`apps/desktop/src-tauri` check, `framework-packages/runtime-host` test, 11 passed) and the
desktop front end (`npm run typecheck` clean, `npm test` 142 passed).

### F3 — done

Loom's CI now compiles and runs the two things it was silently skipping, and the desktop test
command discovers tests instead of matching one directory.

- **S9-2** — `ci.yml` gained three steps for `framework-packages/runtime-host`:
  `cargo fmt -- --check`, `cargo check --locked --all-targets` and `cargo test --locked`, placed
  next to the wrapper's steps in the Windows job, plus `framework-packages/runtime-host -> target`
  in the `Swatinem/rust-cache@v2` workspace list so the extra manifest does not rebuild from
  scratch on every run. Its 11 tests — which cover the MCP bridge, the Surface binding root check
  and the depth guard — now gate merges. The steps were added to the Windows job only, matching how
  the Tauri wrapper is handled; the Linux job stays workspace-only so the second manifest is not
  paid for twice.
- **S9-3** — the wrapper's check gained `--all-targets`, so its test code is compiled, and a
  `cargo test --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml` step was added. This
  turned out to matter more than the finding assumed: `apps/desktop/src-tauri/src/lib.rs` has two
  `#[cfg(test)]` modules holding **37 tests**, none of which had ever run in CI. They pass.
- **S9-7** — `apps/desktop/package.json` `test` is now
  `node --test "src/**/*.test.ts" "src/**/*.test.tsx"`. Node 22 does not recurse into a directory
  argument (`node --test src` fails with `MODULE_NOT_FOUND`), but it does expand glob patterns
  itself, and the patterns are quoted so the shell cannot expand them first — the same string works
  through cmd on Windows and sh on Linux. Verified by dropping a throwaway test at
  `src/components/zz-probe.test.ts`: the count went from 142 to 143 and the nested test was named
  in the output, so a test outside `src/services` is no longer invisible. The probe was deleted. A
  `.tsx` pattern that currently matches nothing is included deliberately and does not error.

`scripts/tests/Test-GitHubActionsContract.ps1` asserted the old wrapper check string literally, so
its required-text list was updated to the six new commands. That contract is itself a CI step, so
leaving it would have failed the job it was meant to protect.

Verified locally, each command exactly as CI now runs it: `Test-GitHubActionsContract.ps1` passes;
runtime-host `fmt --check` clean, `check --locked --all-targets` clean, `test --locked` 11 passed;
wrapper `check --locked --all-targets` clean, `test --locked` 37 passed; `npm test` in
`apps/desktop` 142 passed through the new script.

### F8a — done

First F8 sub-batch: the four Loom daemon findings that live in the surface resource store and the
node patch pointer helpers. All in `apps/daemon/src`, which neither lane reserved and which travels
with this batch.

**S4b-4 — RFC 6901 array handling in `surface_store.rs`.** `set_json_pointer` used to coerce any
non-object cursor to `{}`, so `/props/items/0` on `items: ["a", "b", "c"]` replaced the whole array
with a single-key object and silently dropped the other two elements; `remove_json_pointer` only
ever looked at objects, so the same path was a silent no-op. Both helpers now walk arrays as arrays.
A new `pointer_array_index` accepts `0` or a digit string with no leading zero and nothing else, so
`01`, `+1` and `last` cannot be mistaken for an index. `Set` replaces an in-range element, appends on
the RFC 6901 `-` token, and also appends when the index equals the current length, which is what a
client that just read `len()` will send; anything past that is an `Invalid` error rather than a
silent extend. `Remove` deletes an in-range element and shifts the tail, stays a no-op when the
element or a parent is absent so a repeated patch is still harmless, and errors when the token is not
an index or when the path traverses a scalar. Intermediate objects are still created on demand, which
is what makes `/props/a/b` work on a node without an `a`, but an intermediate that exists with the
wrong shape is now an error instead of being overwritten. Four tests cover set-on-element, `-`
append, end-index append, traversal through an array element, every rejection case, array removal
with its no-ops, and one end-to-end `mutate_node_json` patch proving sibling elements survive.

**S4a-2 — descriptor validation no longer re-hashes the payload on every patch.** `validate_descriptor`
went through `get`, and `get` reads the file and takes a full SHA-256 over up to 16 MiB. Since
`validate_references` runs on every snapshot and every patch, once per resource and once per lease, a
surface carrying a few images re-hashed all of them on every revision. The store now keeps a
`verified` map of `(len, mtime_nanos)` per resource id, stamped when `register` writes bytes it has
already hashed and refreshed whenever `get` completes a successful verification. `validate_descriptor`
compares the descriptor against the in-memory metadata and returns when the file still matches its
stamp; a changed or unavailable stamp falls back to the full read-and-hash. `get` keeps hashing
unconditionally, so the HTTP fetch path and ingest are unchanged, and a failed verification clears
the stamp so the next descriptor check re-reads too. The stamp is taken *before* the read, so a file
replaced mid-read fails the digest check instead of recording a stamp for unverified content. This is
a deliberate narrowing: a payload swapped in place with its length and mtime preserved will pass a
descriptor check until the next fetch. Host-internal state is not the threat model that check was
paying for, and the fetch path still refuses to serve it.

**S4a-3 — the lease table is capped and its writes are debounced.** Pending events and confirmations
had caps; leases had none, while `MAX_RESOURCE_LEASE_MILLIS` lets an hour of grants accumulate and
`persist_leases` re-serializes the whole map with an `fsync` and an atomic replace on every single
`register`. `MAX_ACTIVE_RESOURCE_LEASES` (512) now bounds the live table, and both `register` and
`duplicate_loom_resource_lease` refuse with `LeaseRejected` when it is full rather than evicting a
grant another attachment may still be holding. The cap is global because the lease record carries no
instance id. Additions go through `queue_lease_persist`, which marks the table dirty and writes back
only once `LEASE_PERSIST_DEBOUNCE_MILLIS` (250 ms) has elapsed since the last write; a `Drop`
implementation flushes whatever is still pending. Removals keep writing immediately. Losing an
in-memory-only lease to a crash is the same outcome the caller already handles for an expired one —
the payload itself is durable before the lease is minted, so the client re-registers and gets a new
grant over the same object.

**S4a-4 — a duplicated lease gets a fresh TTL.** `duplicate_loom_resource_lease` cloned the lease and
replaced only `lease_id`, so a duplicate handed to a second attachment 14 minutes into a 15-minute
grant was valid for under a minute, after which `cleanup_expired` dropped it and the attachment's
next `validate_references` failed with `surface_resource_lease_rejected` — a failure that reads like a
protocol error rather than an expiry. The duplicate now expires no sooner than
`DEFAULT_RESOURCE_LEASE_MILLIS` from now, and a lease with longer left than that keeps its own later
expiry. This matches `renew_loom_resource_lease`, which already went through `register` and got a full
TTL.

Three new tests in `surface_resources.rs`: one tampers with a payload while preserving its length and
mtime to prove the stamp is trusted by `validate_descriptor` and still caught by `get`, and that the
failed fetch invalidates the stamp; one proves the second registration inside the debounce window
leaves the table dirty, that a full table refuses both `register` and `duplicate`, and that dropping
the store flushes the debounced writes so both leases survive a reload; one proves a 2-second lease
duplicates to a default-length grant while a maximum-length lease keeps its own expiry.

Verified: `cargo fmt -p loom-daemon` clean, `cargo test --locked -p loom-daemon` 214 + 8 passed, 0
failed. Two tests (`daemon_returns_shutting_down_for_request_accepted_before_shutdown`,
`daemon_runs_approved_capabilities_concurrently`) failed once on a run with the whole suite in
parallel and passed individually and on the next full run; both are timing-sensitive HTTP shutdown
tests unrelated to these files, so they are noted here rather than treated as regressions.

### F8b — done

Scope: S4b-1 and S4b-2, both in `apps/daemon/src/surface_actions.rs`. Both are about what
`execute_surface_action_job` leaves behind when it does not exit through its normal path.

S4b-1 — a panic inside the job body wedged the action forever with no failure ack. The reservation was
released at exactly one place, the end of the normal path, and `request_executor::worker_loop` catches
the unwind from a worker handler, so a panic in `parse_surface_action_response`,
`apply_action_response`, the base64 decode or the resource writes was invisible: for
`RejectWhileRunning` the key stayed in `reject_reservations` and every later invocation of that
`instance:action` pair answered "is already running" for the daemon's remaining lifetime, and the last
persisted ack was the `Running` one, so Hook waited on a request that could never resolve. The release
is now owned by a new `SurfaceActionJobGuard`, created immediately after the serial lock is taken and
therefore covering every path out of the body. Its `Drop` always releases the reservation; when the
body has not called `settle()` — which only happens on an unwind — it first synthesizes the terminal
report through the new shared `finish_failed`, with the error code `surface_action_panicked`, so a
panic produces the same three observable effects as a returned error: a recorded failure on the
instance, a `Failed` ack, and a failure broadcast. Every store access on that path already tolerates a
poisoned mutex, which matters because the panic may well have happened while this thread held one; a
destructor that panicked during an unwind would abort the process. `finish_cancelled` lost its
`coordinator` parameter, since the release is no longer its job, and the failure branch of the normal
path now calls `finish_failed` instead of carrying its own copy of that code. The guard is declared
after the serial lock so it drops first: the reservation is released before the lock that serializes
the action, never the other way around.

S4b-2 — `Serial` was not enforced once a job timed out or was cancelled, and the abandoned runner
thread was never reclaimed. The join handle was bound as `_runner_thread` inside the `match` arm, so
it was dropped when the polling loop broke and the thread was detached. On a timeout or a cancellation
the worker set the flag and returned, releasing both the serial lock and the reservation while the
runner was still executing, so the next `Serial` job for the same pair ran concurrently with the
abandoned one — the opposite of what `Serial` promises — and a runner that ignored cancellation
accumulated one live thread per invocation with nothing reporting it. The handle is now kept in a
`runner_thread` binding that outlives the match, and the new `reap_runner_thread` runs after the
terminal ack but before the guard releases the reservation and before the serial lock is dropped. It
uses the result channel as the "thread is about to finish" signal, since the runner sends immediately
before returning, so the `join` that follows does not block; a late result is discarded because the
terminal ack has already been decided. The wait is bounded by `SURFACE_ACTION_RUNNER_REAP_MILLIS`
(5 000 ms) because a thread cannot be stopped from the outside, and running out of that window logs
the request id and abandons the handle rather than hiding it. The grace is a parameter so tests can
use a short window. Placing the reap after the ack rather than before it keeps Hook's notification
prompt: the extra wait is paid only by the next invocation of the same action, which is exactly the
caller that must not overlap the abandoned runner.

Accepted narrowing: a runner that outlives the grace window is still abandoned. Nothing in std can
kill a thread, so the alternatives were an unbounded wait — which would wedge the worker itself, the
larger of the two failures — or the bounded wait plus a log, which is what this does. Also, the normal
success path now joins the runner thread before returning; the runner has already sent its result at
that point, so this is a handful of microseconds, not a wait.

Tests, all in `surface_actions.rs`: `a_panicking_job_body_releases_the_reservation_and_persists_a_failed_ack`
builds a real job through `reserve_action` and `accept_event`, panics inside a `catch_unwind` with the
guard live, and asserts that `reject_reservations` is empty, that the same pair can be reserved again,
that the persisted ack is `Failed` with code `surface_action_panicked`, and that the event is no longer
pending. `a_settled_job_guard_releases_without_synthesizing_a_failure` proves the guard does not
overwrite a terminal ack the body already wrote. `an_abandoned_runner_thread_is_joined_when_it_finishes_and_given_up_on_when_it_does_not`
covers the three reap cases: a runner that finishes 30 ms late is joined to completion, a runner that
never finishes costs exactly the grace window and no more, and a runner that already returned is
joined immediately.

Verified: `cargo fmt -p loom-daemon -- --check` clean, `cargo test --locked -p loom-daemon` 217 + 8
passed, 0 failed. The two timing-sensitive HTTP shutdown tests noted under F8a passed on this run.

### F8c — done

Scope: the persistence half of S4b-3 (P2) in `apps/daemon/src/surface_store.rs`. The lock-scope half of
the same finding — `submit_internal` holding the store mutex across the package resolve and the manifest
parse, and the missing manifest cache — is deliberately left to a following sub-batch, because it changes
who holds which lock and wants its own verification pass.

`transaction` called `persist` unconditionally, and `persist` serialized the whole persistent projection
and then ran `create_dir_all`, a temporary file, a `write_all`, an `fsync` and an atomic replace. All 17
mutating methods go through that path, including the per-event ones, so a single action produced several
full-store writes. Two changes remove the ones that could never have altered the file:

- `SurfaceInstanceStore` gained a `persisted: Option<Vec<u8>>` field holding the document bytes last known
  to be on disk. `persist` now takes `&mut self`, serializes first through a new shared `document_bytes`
  helper, and returns `Ok(())` before touching the filesystem when the new bytes equal `persisted`. On a
  successful write it records the bytes it wrote; on a failure it leaves `persisted` alone, so the next
  attempt still writes. `new` sets `persisted` from the reserialized projection when it loaded a file and
  leaves it `None` when there was none, so a store that found no file still writes on its first persist
  even if its projection is empty.
- `expire_confirmations` now scans `pending_confirmations` read-only and returns an empty vector without
  opening a transaction when nothing has expired. `recover_pending` ticks it on a timer, and the byte
  comparison alone would have skipped only the write, not the transaction's full clone of the instance
  map.

What this covers in practice: every mutation of a temporary instance (those are filtered out of the
document, so their bytes never change), every idle expiry tick, and any per-event mutation whose result
happens to serialize identically. What it deliberately does not do:

- No debounce and no deferred flush. Anything that does change the projection is still durable before the
  transaction that changed it returns. `recover_pending` re-queues pending events after a crash, so a
  window in which an accepted event is only in memory would lose work — the ordering guarantee is worth
  more than the write it would save.
- No per-instance files and no incremental serialization. That changes the on-disk format and the
  reload path, which is a schema migration rather than a fix.
- `transaction` still clones the whole instance map for rollback. The clone cannot be dropped without a
  journal or interior sharing of the heavy sub-records, and neither belongs in a P2 fix; the serialization
  it feeds is now the cheap half of what used to happen.

Tests, all in `surface_store.rs`: `a_mutation_that_leaves_the_persistent_projection_alone_writes_nothing`
plants bytes no store write could produce over the store file, then shows that creating and attaching to a
temporary instance leaves them there while attaching to a persistent one replaces them, and that the
result still reloads. `a_store_that_found_no_file_writes_on_its_first_persist` pins the `None` case.
`an_expiry_tick_writes_only_when_something_expired` runs an idle tick against the planted bytes, then
back-dates the pending confirmation and shows the tick expires it, reports the `surface_confirmation_expired`
ack and writes.

Verified: `cargo fmt -p loom-daemon -- --check` clean, `cargo test --locked -p loom-daemon` 220 + 8
passed, 0 failed.

### F8d — done

Scope: the remaining half of S4b-3, in `apps/daemon/src/surface_actions.rs` plus one accessor in
`apps/daemon/src/surface_store.rs`. Every Surface submit resolved the instance's installed Art package
and parsed its Surface manifest while holding the Surface store mutex, so one instance's package I/O
serialised every other Surface request in the daemon, for every instance, behind it. The manifest was
also re-parsed from the tool metadata on every single event.

`submit_internal` is now three phases with the lock held for only the first and third:

1. Under the lock, read the previous ack and the instance's descriptor. If the ack is already settled,
   return it. Nothing is reserved or accepted in this phase, so releasing the lock afterwards costs
   only the re-read in phase three.
2. With no lock held, resolve the locked package and pick the action out of its Surface manifest. This
   is the disk read and the JSON parse that used to run under the lock.
3. Re-acquire the lock, confirm the instance is still on the same package, re-read the ack, then do the
   work that has to be atomic: `await_confirmation`, `reserve_action`, `accept_event`, and building the
   invocation from the freshly-read authoritative state.

Supporting pieces:

- `resolve_action` and `surface_manifest` on the executor. `cancel` was doing the same resolve under
  the lock and now goes through `resolve_action` as well, which also unified its "not declared" error
  message with the one `submit_internal` produces; no test asserted either wording.
- A manifest cache, `Mutex<BTreeMap<String, Arc<SurfacePackageManifest>>>`, keyed by
  `art_id`/`art_version`/`package_digest`. The digest pins package content, so an entry cannot describe
  anything but the package the caller resolved and never needs invalidating. It is guarded by its own
  mutex rather than living in the store, because being reachable only under the store lock is precisely
  what the fix removes. Past `SURFACE_MANIFEST_CACHE_LIMIT` (64) it is cleared wholesale; entries only
  accumulate as instances migrate to new Art versions, and clearing keeps the bound without pretending
  to know which entry is coldest. A poisoned cache mutex is treated as a miss, since failing a Surface
  action because a cache lock was poisoned would be worse than parsing the manifest again.
- `settled_ack`, so the idempotency rule — return the existing ack unless recovery is re-driving a
  non-terminal one — is written once and applied identically in phases one and three.
- `same_locked_package`, which compares only `art_id`, `art_version` and `package_digest`. The rest of
  the descriptor carries counters (`generation`, `surface_revision`, `preview_revision`,
  `result_revision`) that move on ordinary traffic such as a snapshot, so comparing whole descriptors
  would report a package change on nearly every concurrent event and turn the retry into a spin.
- `SurfaceInstanceStore::descriptor`, a light accessor. `get` clones the whole record including pending
  events, acks and authoritative state; phase one only needs to know which package to resolve, and
  paying for a second full record clone would have traded one cost for another.

Deliberate narrowings. The resolved `ToolDefinition` is not cached alongside the manifest: the resolver
also re-checks that the package is still installed and still trusted, and a cached tool would keep
serving a package that has since been revoked or uninstalled. The prepare retry is bounded at
`SURFACE_ACTION_PREPARE_ATTEMPTS` (3) and then reports a conflict, rather than retrying forever against
a migration loop or failing on the first benign race.

Tests. `a_burst_of_events_reuses_one_parsed_surface_manifest` submits three events against one locked
package and asserts the resolver still ran three times — the installation and trust checks must not be
cached — while the manifest cache holds one entry. `a_package_migration_during_resolve_makes_the_submit_prepare_again`
injects a resolver that migrates the instance to a different Art version and digest on its first call
only; the submit resolves twice, and the second prepare rejects the now-stale event generation, which a
prepare that trusted its first reading of the store could not have done.

Verified: `cargo fmt -p loom-daemon -- --check` clean, `cargo test --locked -p loom-daemon` 222 + 8
passed, 0 failed.

### F8e — done

Scope: S5a-3, the HTTP request reader in `apps/daemon/src/lib.rs`. It read 512 bytes per `read` call —
about 196 000 syscalls for one `MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES` install — and then copied the
payload three times on the way to a handler: the accumulated `Vec`, a `String::from_utf8_lossy(..).to_string()`
of the whole request, and the `body: String` that `ParsedHttpRequest::from_raw` cut out of it. For a
96 MiB upload that is roughly 288 MiB resident at once.

What changed:

- `HTTP_READ_CHUNK_BYTES` is 64 KiB, so the same install takes about 1 500 reads instead of 196 000.
- The request head is parsed once, into a new `RequestHead { header_end, content_length, body_limit }`,
  the moment the `\r\n\r\n` terminator first appears. The size-limit and completeness checks became
  `RequestHead::exceeds_size_limit` and `RequestHead::has_full_body`, both arithmetic on the received
  byte count.
- `parse_request_head` takes a `scan_from` offset and the reader passes `len - 3`, so a terminator split
  across two reads is still found without re-searching the prefix.
- `HttpReadOutcome::Request` now carries `Vec<u8>` rather than `String`, and
  `ParsedHttpRequest::from_raw` takes those bytes by value: it `split_off`s the body out of the buffer it
  was handed and decodes only the head. A new `into_lossy_string` decodes the body without a copy when
  the bytes are valid UTF-8, which is the case that matters for a request body.

An unreported defect in the same loop is fixed by the head-parsing change. The old code called
`request_exceeds_size_limit` and `request_has_full_body` on every chunk, and each of those re-scanned
the entire accumulated buffer for the header terminator — O(n²) in the upload size, which is what made
the syscall count expensive rather than merely numerous. `request_has_full_body` had no tests of its own
and is deleted; `request_exceeds_size_limit` is kept as a `#[cfg(test)]` delegate over
`parse_request_head` so the tests that state the limit rules against a complete request still hold and
cannot drift from what the reader does.

Tests. A `ChunkedReader` yields at most `n` bytes per call and counts the calls.
`a_request_arriving_one_byte_at_a_time_is_still_split_at_its_first_header_terminator` feeds a request
whose body contains a blank line of its own, one byte per read, and asserts the reassembled bytes are
byte-identical and that method, path, header and body all survive the split — a reader that re-scanned
from each chunk boundary, or a parser that searched a decoded copy for any terminator, would cut in the
wrong place. `a_large_body_is_read_in_few_reads_and_handed_over_without_a_copy_per_layer` pins the chunk
size: a 200 KB body on `/v1/surfaces/resources` must complete within `ceil(200000 / HTTP_READ_CHUNK_BYTES) + 1`
reads. `a_declared_body_over_the_route_limit_is_rejected_before_the_body_is_read` sends only a head
declaring an over-limit `Content-Length` and asserts a 413 after exactly one read, so the limit is still
enforced from the declaration rather than by accumulating the body first.

Verified: `cargo check -p loom-daemon --all-targets` clean with no warnings, `cargo fmt -p loom-daemon -- --check`
clean, `cargo test --locked -p loom-daemon` 225 + 8 passed, 0 failed.

### F8f — done

Covers S6b2c1-2 (the framework trust policy ignored the persisted operator setting) and S6b2c1-3
(`resolve_framework_package_dir` reported a damaged neighbour as "framework not installed").

S6b2c1-2. `TrustStore::effective_policy()` (`loom_plugin_security/src/lib.rs:186`) is the environment
override when one is set and the persisted policy otherwise. Every Art path already called it; the three
framework paths called `TrustPolicy::from_env()`, which falls back to `AllowUnsigned` whenever the
environment variable is absent. An operator who wrote `require-trusted` into the trust store therefore
got that policy enforced on Art packages and silently not enforced on framework packages — the components
that execute Art code with the highest privilege. The three sites now read the effective policy:
`framework_ready_in` (`framework.rs:396`), the package install (`framework.rs:1029`) and rollback
(`framework.rs:1209`).

Consequence, deliberately accepted. Art installs enforce the policy only for external provenance
(`install.rs` checks `source == ArtInstallSource::ExternalPackage`, and skips the check when
`local_authoring` or `bundled_catalog` is set). Frameworks carry no provenance at all —
`FrameworkInstallationState` is `{version, enabled}` and `FrameworkActivationState` is `{active, previous}` —
and the four shipped host manifests under `framework-packages/` (publisher `neuro.official`) have no
`signature` member. So with this fix a persisted `RequireSigned` or `RequireTrusted` policy makes the
bundled frameworks unready, and every Art becomes unrunnable with
`ArtInstallError::FrameworkNotReady{reason: "ready"}`. That is the honest reading of "require signed":
the operator asked for signatures and the shipped frameworks do not have any. The fix stands as the
review prescribed it; signing the host framework packages at build time is the follow-up that makes a
strict policy usable, and is recorded here rather than smuggled into this sub-batch.

S6b2c1-3. `resolve_framework_package_dir` returned `Option<PathBuf>` and used `?`/`ok()?` throughout, so a
single unreadable directory entry or one sibling package with a damaged `framework.manifest.json` ended the
scan and the caller reported the *healthy* framework as not installed. It now returns
`Result<PathBuf, FrameworkError>`: it validates the reference first, resolves a publisher-qualified id
directly through `framework_storage_path`, maps a missing `runtime_root` to `FrameworkNotInstalled`, and
`continue`s past each unreadable entry and each damaged sibling manifest instead of aborting the scan. The
match on the collected candidates is explicit — zero is `FrameworkNotInstalled`, one is the directory, more
than one is `AmbiguousFramework`, which is what a local id shipped by two publishers really is and which the
old code reported as "not installed" as well. Callers updated: `framework_ready_in` (`framework.rs:333`),
`runtime_dir` (`framework.rs:577`), and `framework_process.rs:146`, which folds the resolver's error text
into the `FrameworkPackageNotFound.path` it already surfaces.

Tests. `a_damaged_framework_package_does_not_hide_a_healthy_one` installs one good package, writes a sibling
whose manifest is truncated JSON, and asserts the good one still resolves and is ready.
`a_local_id_shipped_by_two_publishers_resolves_as_ambiguous_not_missing` pins the error variant, so a future
change cannot quietly turn a name collision back into a missing framework.
`the_persisted_trust_policy_blocks_framework_readiness_and_installs` writes `RequireSigned` to the trust store
with no environment override present and asserts both that an unsigned framework package is refused at install
and that an already-installed unsigned framework stops being ready — the exact pair of behaviours that
`from_env()` skipped.

One existing test needed repair, and the repair is the point rather than an accommodation.
`install::tests::strict_trust_policy_allows_local_and_bundled_sources_but_rejects_external_unsigned_packages`
sets `RequireSigned` and then performs three Art installs; its fixture framework was unsigned, so after this
fix all three failed on framework readiness before reaching the Art trust check under test. The fixture now
installs a *signed* framework: `install_signed_test_framework` builds the same package as
`install_test_framework`, adds a `signature` member pointing at `signature.json`, signs the directory with a
key generated in the test, and zips the signature alongside the manifest and the entry file. The assertions
about Art behaviour are unchanged — external unsigned is still rejected with "trust policy rejected package
status Unsigned", authored and bundled installs still succeed with `PackageTrustStatus::Unsigned`. The
alternative, enforcing the effective policy only at install and rollback and leaving the readiness probe on
`from_env()`, was rejected: it would have left the persisted policy inert for every framework already on disk,
which is the whole of what S6b2c1-2 reports.

Verified: `cargo check --locked --all-targets` clean across the workspace, `cargo fmt -p loom_tool_registry -- --check`
clean, `cargo test --locked -p loom_tool_registry --lib` 127 passed, 0 failed.

Environment note, because it cost a bisect and will cost another one if it is not written down. During the first
verification pass 17 of those tests hung rather than failed, and a bare
`powershell.exe -NoProfile -NonInteractive -Command "Write-Output PS_OK"` produced no output in 90 s and had to be
killed. The tests that hang are the ones that spawn a copy of the shell — `framework_process.rs:1086` copies
`powershell_executable()` into the fixture package — plus the `normalize_mcp_image_search_*` and
`powershell_httpclient_fallback_*` pair. The cause is concurrent `cargo` load, not the code: with cargo idle the
same bare probe answers in well under a second and the full suite runs green in 14 s. Orphaned `powershell.exe`
processes accumulate while the shell is wedged and never exit, so they pile on. Do not run PowerShell tests or
`npm` while a Loom `cargo` build is in flight.

### F8g — done

Covers S6b2c1-4 (a corrupt `frameworks.json` silently reported zero installed frameworks, and the next
write made the loss permanent).

`installation_states` returned `BTreeMap` and swallowed both failure modes — the read error through a
`let Ok(..) else` and the parse error through `unwrap_or_default()`. It now returns
`Result<BTreeMap<String, FrameworkInstallationState>, FrameworkError>` and distinguishes the three cases
that were collapsed into one: a missing file is `Ok(empty)`, which is the genuine "nothing installed yet";
any other read error is `Io`; contents that are not the expected map are the new
`FrameworkError::CorruptState { path, reason }`, whose message names the file and says to repair or remove
it. Adding a variant is safe for consumers — `framework_error_response`
(`apps/daemon/src/lib.rs`) matches four variants and falls through to a 500, and it now has an explicit
`framework_state_corrupt` arm carrying the path so the UI can tell an operator which file to fix rather
than showing a bare 500.

The nine call sites split by what a wrong answer costs.

Mutating paths propagate, so corruption can never be written over. `recover_uninstall_tombstones` is the
worst of them: it decides restore-versus-delete from this map, so the old empty-map behaviour deleted every
pending tombstone and silently completed uninstalls the operator never asked for; it now returns the error
and leaves the tombstone in place. `set_enabled` and `uninstall` propagate, and both are additionally
protected by `resolve_state_key`, which reads the same file and now propagates too — so `uninstall` refuses
before it moves a package into a tombstone. The install path reads the state up front, right after
`packages_root` is created and before any package file moves into place, so a corrupt file aborts the install
instead of leaving an unpacked package that no state entry describes; the later insert reuses that map. The
two paths that had already mutated the activation before reading — rollback, and `uninstall` if the file is
damaged while the uninstall is in flight — run the same compensation their `write_installed` failure branch
runs: restore the previous activation, or rename the tombstone back.

Reporting paths degrade, with the reason written next to each. `installed_ids` and `is_enabled` are
infallible by signature and use `unwrap_or_default()`; `status_of` uses `.ok()?`. That is the old behaviour
for those three, and it is acceptable only because it can no longer become permanent: every mutating path
refuses. `readiness` is the one that surfaces it to a human — it already returns the `resolve_state_key`
error text, so a corrupt state file now shows the file path and the remedy instead of `未安装`.

Tests. `a_corrupt_state_file_is_reported_and_never_silently_rewritten` installs a healthy package, truncates
the state file mid-value, then asserts that readiness names the state file, that disable, uninstall and a
fresh install all fail with `CorruptState`, that the damaged bytes survive verbatim, and that the installed
package is still on disk. `a_corrupt_state_file_leaves_an_interrupted_uninstall_recoverable` stages exactly
what a crash mid-uninstall leaves — the package in a tombstone with the state file still listing it — damages
the state file, and asserts that constructing a registry leaves the tombstone alone; after the state file is
repaired, a second construction restores the package and consumes the tombstone. The first test is about not
losing state, the second about not losing a package.

Two `loom-daemon` tests needed the same repair F8f applied inside `loom_tool_registry`, and they are recorded
here because they were found while verifying this sub-batch.
`bundled_catalog_art_install_preserves_strict_external_trust_policy` and
`authored_art_handlers_cover_create_package_rollback_and_uninstall` both persist a strict policy and then
install an unsigned fixture framework, which the F8f change correctly refuses. A new
`signed_framework_package_zip(id, version, key)` fixture builds the same package signed; the `RequireTrusted`
test also registers a `PublisherTrustRecord` for `publisher.test`, because a signature alone yields `Verified`
and that policy accepts only `Trusted`. The assertions about Art behaviour are unchanged.

Verified: `cargo check --locked --all-targets` clean, `cargo fmt --all -- --check` clean,
`cargo test --locked -p loom_tool_registry --lib` 129 passed, `cargo test --locked -p loom-daemon` 225 + 8
passed, and `cargo test --locked --workspace` 575 passed across 57 suites, 0 failed.

### F8h — done

S6b2c2-1 (P2, `crates/loom_tool_registry/src/framework.rs`). Crash recovery treated the journalled `target`
as scratch space: whenever the on-disk activation did not match `next_activation`, recovery restored the old
activation and then deleted `package_root/journal.target`. Two writers name a directory that predates the
operation. The install path journals `target: active_relative` even in the branch that reuses an existing
version directory instead of renaming staging into place, and `rollback` journals `target: next.active`, which
is by definition a version that is already installed — usually the very one the restored activation points at.
A crash in either window therefore made the next startup delete a live version and leave the framework
pointing at a directory that no longer exists.

`FrameworkLifecycleJournal` now carries `created_target: bool` with `#[serde(default)]`, and
`recover_lifecycle_journals` removes the target only when that flag is set. The default is `false` rather than
`true` so a journal written by an older build is never used as authority to delete: recovery may then leave one
genuinely orphaned staging directory behind, which pruning later reclaims, and that is strictly better than
destroying an installed version. The install path passes its existing `target_created` local, which is already
`false` exactly in the reuse branch; `rollback` passes `false`.

`rollback` also wrote its journal and then called `write_activation` with a bare `?`. A failed activation write
left a journal describing an activation that never happened, so the next startup would "restore"
`old_activation` over an activation that already held that value and, before this fix, delete the rollback
target as well. That path now clears the journal before returning the error, matching every other failure path
in the file.

Two tests: `framework_recovery_keeps_a_version_the_interrupted_operation_did_not_create` installs 1.0.0 and
2.0.0, stages the journal an interrupted rollback would leave (`created_target: false`, target = the previous
version), and asserts recovery restores the 2.0.0 activation, consumes the journal, and leaves the 1.0.0
directory intact. `a_failed_rollback_activation_leaves_no_lifecycle_journal_behind` blocks the activation
staging path (`active.json.tmp` as a directory) so the write inside `rollback` fails, and asserts the journal
is gone, the activation is unchanged and the previous version still exists. That test first writes a sentinel
journal, so "no journal on disk" can only mean `rollback` reached its own journal write and cleaned up, not
that it failed earlier and never journalled at all. The existing
`framework_recovery_restores_previous_activation_and_removes_orphan_target` keeps asserting the delete, now
with `created_target: true`, so both directions are covered.

The Art-side twin of this finding (S6b2b1-2 in `install.rs`) is the next sub-batch; the review asks for both to
be fixed the same way.

Verified: `cargo check --locked --all-targets` clean, `cargo fmt --all -- --check` clean,
`cargo test --locked -p loom_tool_registry --lib -- framework` 51 passed, and
`cargo test --locked --workspace` 577 passed, 0 failed (`loom_tool_registry` 131, `loom-daemon` 225 + 8).

### F8i — done

S6b2a-1 and S6b2b1-2 (both P2, `crates/loom_tool_registry/src/install.rs`). The Art side of F8h, and the same
two defects: the review asks for both to be fixed together because they share one root cause.

`ArtLifecycleJournal` now carries `created_target: bool` with `#[serde(default)]`, and
`recover_art_lifecycle` removes `art_root/journal.target` only when that flag is set. As on the framework side
the default is `false`, so a journal written by an older build is never authority to delete. The install path
already computed `target_created` — `false` exactly in the branch that discards staging and reuses an existing
version directory — and honoured it on its synchronous failure paths; it now records it in the journal too.
`activate_art_pointer` records `false`, because both rollback and explicit version activation point at a
version that is already installed, which was the exact directory the old recovery code deleted.

`activate_art_pointer` also called `write_art_activation(active_path, &next)?` with a bare `?`, unlike the
install path, which restores the previous activation and clears the journal when that same write fails. It now
clears the journal before returning the error: the journal described an activation that never happened, so the
next startup would otherwise "restore" `old_activation` over an activation that already held that value.

Two tests: `art_recovery_keeps_a_version_the_interrupted_operation_did_not_create` installs two versions,
stages the journal an interrupted rollback would leave (`created_target: false`, target = the previous
version), and asserts recovery restores the newer activation, consumes the journal, and leaves the older
version directory intact. `a_failed_art_activation_write_leaves_no_lifecycle_journal_behind` blocks
`active.json.tmp` with a directory so the write inside `rollback_art_package` fails, and asserts the journal is
gone, the activation is unchanged and the older version still exists; it writes a sentinel journal first, so
"no journal on disk" cannot be satisfied by a rollback that failed before journalling at all. The existing
`art_recovery_restores_activation_and_rejects_unsafe_journal_paths` keeps asserting the delete, now with
`created_target: true`.

Verified: `cargo check --locked --all-targets` clean, `cargo fmt --all -- --check` clean, and
`cargo test --locked --workspace` 579 passed, 0 failed (`loom_tool_registry` 133).

### F8j — done

S6b2c3-1 (P2, `crates/loom_tool_registry/src/framework_process.rs`). Image candidates produced by a framework
Art never reached any consumer, because the producer and both consumers disagreed on the candidate key names.
`insert_image_candidate_metadata` copied each candidate object through verbatim, while the canonical shape
minted by the MCP tool path is `{index, title, imageUrl, thumbnailUrl, sourcePageUrl, width, height}`. Both
consumers key an item on `imageUrl` and drop items without it: the daemon's Hook canvas bridge
(`apps/daemon/src/hook_canvas.rs`) does so inside a `filter_map`, and Hook's `artDeliveryCandidates` does the
same. The shipped image-search Art emits `{id, title, thumbnail, data, sourceUrl, width, height, index}`, so
every one of its candidates was silently discarded.

The fix normalizes on the host side rather than making every Art learn the wire shape. Three source lists name
the producer keys that may stand in for a canonical key — `imageUrl`/`image_url`/`url`/`src`/`data`/`dataUrl`/
`data_url`/`thumbnailUrl`/`thumbnail_url`/`thumbnail` for `imageUrl`, `thumbnailUrl`/`thumbnail_url`/
`thumbnail`/`preview` for `thumbnailUrl`, and `sourcePageUrl`/`source_page_url`/`sourceUrl`/`source_url`/
`pageUrl`/`page_url` for `sourcePageUrl` — and the first non-empty string wins. `index` defaults to the item's
position when the Art omitted it or sent a non-integer. Producer keys are kept, not renamed, so an Art or a
consumer that reads its own names keeps working; an item with no usable image reference at all is left alone
rather than given a fabricated `imageUrl`. `selected_image_candidate_index` reads only `selectedIndex` and `id`,
so computing it before normalization is unaffected.

This also closes S6b2c3-1's Art half without touching the Art: the shipped image-search runtime needs no change,
because `data` is an accepted source for `imageUrl` and `sourceUrl` for `sourcePageUrl`.

One related inconsistency is deliberately left alone. When the Art declares no image output, candidates are
attached to `output.candidates` instead of `loomMetadata.candidates`, a key neither consumer reads. Moving them
would change what a non-image Art returns to its caller, which is a behaviour change rather than a fix, so it
stays out of this finding.

Tests: `framework_image_candidates_are_normalized_to_the_consumer_wire_shape` feeds three candidates through
`response_to_tool_value` — the shipped Art's shape, an Art that already speaks the wire shape, and one with no
image reference — and asserts the first gains `imageUrl`/`thumbnailUrl`/`sourcePageUrl` and a positional `index`
while keeping `thumbnail` and `id`, the second is untouched (including its explicit `index: 7` and its absent
`thumbnailUrl`), and the third gains only `index`. The existing
`framework_image_candidates_use_canonical_loom_metadata` still covers kind and `selectedIndex`.

Verified: `cargo check --locked --all-targets` clean, `cargo fmt --all -- --check` clean, and
`cargo test --locked --workspace` 580 passed, 0 failed (`loom_tool_registry` 134).

S6b2c3-2 (F8k) is the next sub-batch: bound the candidate array and total candidate bytes in the host, and stop
the shared image runtime from emitting the same full-size data URL as both thumbnail and payload.

### F8k — done (host half of S6b2c3-2)

S6b2c3-2 (P2) has a host half and an Art half; this is the host half, in
`crates/loom_tool_registry/src/framework_process.rs`. The Art half — the shared runtime and the image-search Art
each carrying the same full-resolution data URL twice — is F8l.

Two ceilings now apply to `response.candidates`, which the host previously inserted verbatim:
`MAX_FRAMEWORK_CANDIDATES` (64, matching `MAX_MCP_IMAGE_CANDIDATES` on the MCP tool path) and
`MAX_FRAMEWORK_CANDIDATE_BYTES` (32 MiB across the whole array, not per item). `bound_framework_candidates`
truncates to both and returns how many items it dropped; `candidate_value_bytes` measures an item by summing its
strings and keys instead of serializing it, so measuring copies nothing, and nesting needs no depth guard because
the value came off the framework's stdout through `serde_json`, which already refuses input past its recursion
limit. Truncation keeps the leading items — where the selected candidate sits unless the Art says otherwise — and
one item larger than the entire budget still survives as the only item, since a grid with no images at all is
worse than honouring the budget exactly. The drop count is reported as `loomMetadata.candidates.droppedItems`, so
a consumer can tell a truncated grid from a short one; the bound is applied in `response_to_tool_value` before
either branch, so the non-image `output.candidates` array is bounded too, even though it carries no place to
record the count.

`normalize_framework_image_output` also removes `output_base64` and `outputBase64` when it inserts the `content`
it built. `New-ImageOutput` in the shared Art runtime emits `output_base64`, `output_path` and its own `content`
together, and the host previously stripped only the path keys, leaving a second full copy of the same data URL
beside the one it had just decoded from the validated file. Removing it is safe because every reader in the
workspace falls back to `content[0].data`: `extract_image_output` and `extract_default_output` in
`crates/loom_workflow_runtime/src/lib.rs` check `output_base64` first and then `content`, `extract_named_output`
routes the port name `output_base64` through `extract_image_output`, and the daemon's
`extract_art_image_data_url` reads `content` before its key list. The removed value is also the unvalidated one:
the host checked the path, the output roots and the size limit for the copy it kept, and none of them for the
copy the framework declared.

Tests: `framework_image_candidates_are_capped_by_item_count` sends `MAX_FRAMEWORK_CANDIDATES * 3` candidates and
asserts 64 items, `droppedItems` 128 and the leading item preserved.
`framework_image_candidates_are_capped_by_total_bytes` sends three candidates of a third of the budget each and
asserts the third is dropped. `a_single_oversized_framework_candidate_is_still_delivered` sends one candidate
larger than the whole budget and asserts it survives with no drops.
`framework_image_output_drops_the_self_declared_base64_copy` writes a real 1×1 PNG inside an allowed output root,
hands the host an output carrying `output_base64`, `outputBase64` and `output_path`, and asserts all three are
gone while `content[0]` holds a freshly built PNG data URL and unrelated keys such as `width` survive.

Verified: `cargo check --locked --all-targets` clean, `cargo fmt --all -- --check` clean, and
`cargo test --locked --workspace` 584 passed, 0 failed (`loom_tool_registry` 138).

### F8l — done (Art half of S6b2c3-2, thumbnail downscale)

The shipped image-search Art put the same full-resolution data URL in both `data` and `thumbnail` of every
candidate, so a six-candidate response carried twelve copies of six images. The grid paints those thumbnails at
gallery size, so the extra pixels were never displayed — they only cost transport, the JSON parse, the clone
through the store and the browser's decode.

Fix: `New-ImageThumbnailDataUrl` in `art-packages/shared/image-runtime-common.ps1` (added after
`Convert-ImagePathToDataUrl`, so every sample Art gets it through the `common.ps1` copy that
`scripts/Build-LoomSampleArtPackages.ps1` makes). It decodes the data URL, and when the longest edge exceeds
`MaxEdge` (320 by default) it re-encodes a bicubic downscale through the existing `Resize-BitmapArgb`, preserving
aspect ratio and clamping each side to at least one pixel. The downscale is an optimization and never a hard
requirement, so every failure path returns the input unchanged: an empty or whitespace string, a non-positive
`MaxEdge`, a payload that is not valid base64, a payload `System.Drawing` cannot decode, and an image that is
already within `MaxEdge`. Both memory streams and every bitmap are disposed in `finally` blocks.

`art-packages/samples/image-search/runtime/main.ps1` now sets `thumbnail = New-ImageThumbnailDataUrl -DataUrl
$dataUrl` while `data` keeps the full-resolution URL. That split is deliberate: `data` is the payload a selection
turns into the node's output, and it is also the key the host's `CANDIDATE_IMAGE_URL_SOURCES` list maps to
`imageUrl`, so downscaling it would silently degrade the delivered image. `thumbnail` is preview-only, and the
host's `CANDIDATE_THUMBNAIL_URL_SOURCES` maps it to `thumbnailUrl`.

Verified by dot-sourcing the shared runtime and exercising the helper directly: a 900×600 PNG data URL of 4706
characters came back as 320×213 in 1194 characters; the same URL with `-MaxEdge 4096` came back identical; an
undecodable `data:image/svg+xml;base64,notbase64!!` came back identical; an empty string came back identical.

### F8m — done (Art half of S6b2c3-2, the duplicate `output_base64`)

The last copy of the same image inside a single response. Three sample runtimes emitted the finished image twice,
once under `output_base64` and once inside `content`: `New-ImageOutput` in
`art-packages/shared/image-runtime-common.ps1` (used by the image-blend and remove-bg Arts), the image-search Art's
own `$output`, and `art-packages/samples/color-transfer/runtime/main.py`. For image-search that made three copies,
because the selected candidate's `data` is already the same string.

F8k made the host strip `output_base64` from any framework output that also names a file, so the copy was already
being discarded before delivery for the two file-backed Arts and for color-transfer. What F8k cannot fix is the
stdout the host has to read and parse in the first place, and it never reaches the image-search Art at all, which
declares no `output_path` and so returns from `normalize_framework_image_output` before the strip. All three
runtimes now emit the image only inside `content`.

Safe because every reader falls back to `content[0].data`: the workflow runtime's `extract_image_output`, its
`extract_default_output`, its `extract_named_output` arm for a port literally named `output_base64`, and the
daemon's `extract_art_image_data_url` (which reads `content` first). Hook contains no `output_base64` reference at
all. Two declared ports were checked by name rather than assumed: the image-search manifest declares its output as
`output`, which routes through `extract_default_output`; the image-blend-compress workflow references
`${{ nodes.blend.outputs.output_base64 }}` and declares `primaryOutput.output` as `output_base64`, which routes
through the `extract_named_output` arm — and that fallback was already load-bearing before this batch, since the
Image Compress Art it names has never emitted the key.

`scripts/tests/Test-LoomSampleArtRuntime.ps1` asserted the key directly in three places, so it gained
`Get-ResponseImageDataUrl`, which reads the image the way the host does — `content` first, then a self-declared
`output_base64` for any Art that still carries one. The generic "output image is missing" assertion, the
image-search "selected output matched the second candidate" assertion and the reported output length all go
through it now, and image-search additionally asserts the key is absent. `Test-LoomSampleArtInstallExecution.ps1`
needed no change: it already tried `content` after both `output_base64` spellings.

`art-packages/samples/color-transfer/**` is reserved by neither lane, the same situation as `apps/daemon/**` in
F8f and F8g; it is noted on the sync board.

Verified: `scripts/Build-LoomSampleArtPackages.ps1` rebuilt all 7 sample packages and
`scripts/tests/Test-LoomSampleArtRuntime.ps1` passed all 11 execution and rejection cases, including both
color-transfer cases, image-search, image-blend and remove-bg.

### F8n — done (S6b2d1-1, a damaged Art settings file made the whole registry unreadable)

`ArtSettingsStore::read_file` propagated every `serde_json` parse error, and `apply_persisted_art_settings` —
which runs on the read path for each tool — turned that error into a failed registry read. One truncated
`art-user-settings.json` therefore emptied the entire Art list rather than losing the preferences it actually
held. `tools.json` has recovered from corruption for a long time by copying the damaged bytes aside and
rewriting the file, and a file that stores nothing but user preferences has no reason to be stricter than the
file that stores the tools themselves.

`read_file` now recovers in place: on a parse error it copies the raw bytes to a sibling
`art-user-settings.json.corrupt-<pid>-<nanos>` through the new `write_corruption_backup`, writes the default
document back, and returns the default. The naming matches the registry's own corruption backups so both land
in the control plane with the same recognisable shape, and the recovery happens once instead of on every read
because the file on disk is valid again afterwards. Both writes are deliberately best effort: a read-only
control-plane directory or a full disk must still let reads succeed, and the caller sees "no stored settings"
either way. The crate has no logging facility at all — no `tracing` dependency, no `eprintln!`, no `log::` —
so the recovery is observable through the backup file rather than through a log line.

`apply_persisted_art_settings` now calls `get_optional(...).unwrap_or_default()`. The store recovers from
corruption on its own, but two other failures remain reachable there: the id validation inside `get_optional`
can reject a qualified id that `ToolDefinition::validate` accepted, and the read itself can fail for reasons
unrelated to this tool, such as a permission error on the control-plane directory. In all of those cases the
honest answer is "this Art has no stored settings", not "the registry is unreadable and every Art disappears".

Three tests cover it. `a_damaged_settings_file_is_backed_up_and_reset_instead_of_failing_every_read` asserts
that `get_optional`, `get` and `list` all succeed over a truncated file, that exactly one backup exists and
holds the original bytes verbatim, and that the file left on disk parses.
`a_damaged_settings_file_still_accepts_the_next_save` asserts a save over a damaged file round-trips.
`a_damaged_art_settings_file_does_not_hide_every_art` covers the registry level: a saved package-backed tool
is still listed and still fetchable after the settings file is truncated behind the registry's back, and the
backup is on disk.

Verified: `cargo fmt --all`, `cargo check --locked --all-targets` and `cargo test --locked --workspace` all
clean, with `loom_tool_registry` at 141 tests (138 before) and the workspace at 587 passed, 0 failed. The
first workspace run flagged `daemon_records_failed_gateway_brain_plan_with_run_evidence` and
`daemon_returns_shutting_down_for_request_accepted_before_shutdown`; both pass in isolation and both pass on a
repeat full-suite run, so they are timing flakes under full-suite parallelism and unrelated to this change,
which touches `loom_tool_registry` only.

### F8o — done (S6b2d2-3 and S6b2d2-4, the cloud connector's defaults)

**S6b2d2-3.** `cloud_network_policy` read `permissionPolicy.network.allowLocalhost` with
`unwrap_or(true)` while `OutboundPolicy::default` sets `allow_http_loopback: false`, so a cloud Art
that declared no network policy at all was allowed to call `http://localhost:*` and
`http://127.0.0.1:*` in cleartext — the Loom daemon's own HTTP surface, Hook, a local model server —
while carrying the Art's credential headers. The default is now `false`, matching
`OutboundPolicy::default`, with a comment saying why. An Art that genuinely talks to a local service
knows that it does and can declare it; no shipped Art needed a change, because the only sample with a
`cloud_api` execution is `remove-bg`, whose endpoint is `package://remove-bg`.

Five fixture-backed tests did need the declaration, because they all point a cloud tool at a
loopback `TcpListener`: the four `execute_cloud_api_tool_*` tests in `crates/loom_tool_registry`
(through a new `loopback_cloud_metadata` test helper, and inline for the one that builds its tool from
JSON) and three daemon contract tests — `daemon_executes_cloud_api_backed_tool_contract`,
`daemon_hook_bridge_executes_cloud_api_art_node_image_output` and
`daemon_hook_bridge_executes_cloud_api_multipart_art_node_with_input_file`. `apps/daemon/**` is
reserved by neither lane, the same situation as F8f, F8g and F8m. A new negative test,
`a_cloud_art_without_a_declared_network_policy_cannot_call_loopback`, pins the new default: the same
fixture endpoint is refused when the tool declares nothing.

The daemon failure this produced before the fixtures were updated is worth recording, because the
symptom pointed nowhere near the cause: two cloud Art tests failed on the policy, and their panics
poisoned the daemon test module's `ENV_LOCK`, so more than twenty unrelated tests failed with
`env lock: PoisonError`. Only the first panic in the log named the real problem.

**S6b2d2-4.** `timeout.unwrap_or(CLOUD_API_TIMEOUT).min(CLOUD_API_TIMEOUT)` meant a declared deadline
could only ever be shortened: `execute_tool_with_timeout(tool, .., Duration::from_secs(120))` — how
the daemon passes the run budget down — silently became 30 s, and image generation and background
removal routinely need longer than that. A new `cloud_api_timeout(tool, requested)` resolves it in
one place: the caller's deadline when it states one, else a package's
`metadata.cloudApi.timeoutMs`, else the 30 s default, clamped to a new
`CLOUD_API_MAX_TIMEOUT` of 600 s at the top and to one millisecond at the bottom because `reqwest`
treats a zero timeout as "no timeout at all". `CLOUD_API_TIMEOUT` keeps its old value and is now
documented as the default rather than the ceiling.

The package-side knob is read out of the free-form `metadata` object, the same way
`permissionPolicy.network` already is, so it needs no protocol or schema change.
`a_cloud_art_deadline_can_be_raised_by_the_caller_and_by_the_package` covers all six cases: default,
caller-only, package-only, caller-wins-over-package, both clamped to the ceiling, and zero.
`docs/plugin-permissions.md` now documents the `allowLocalhost` default and the `cloudApi.timeoutMs`
vocabulary.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean, and
`cargo test --locked --workspace` at 589 passed, 0 failed, with `loom_tool_registry` at 143 tests.
One run flagged `hook_art_execution_creates_durable_run_evidence`, which passes in isolation and on
both repeat full-suite runs — the same full-suite parallelism flake family as the two daemon tests
noted under F8n.

### F8p — done (S6b2d2-2, the multipart file-field heuristic, plus the release smoke fallout from F8o)

`build_cloud_multipart_form` decided a field carried a file from the field's *name* — `file`, `image`,
`image_file`, or any `*_file` — and then did nothing more than `Path::new(&rendered_value).exists()`
before handing the path to `Form::file`. The rendered value comes from the execution arguments, so
any caller of `POST /v1/tools/{id}/execute` could put an absolute path in an ordinary text field of a
hosted Art and have the host read that file and upload it to the Art's third-party endpoint. An SSH
key, a credential file, a database dump: no containment check of any kind stood in the way, in
contrast to the framework arm, which canonicalizes every path it accepts and requires it to sit under
an execution root (`framework_process.rs:467-492`).

The heuristic is now the author's declaration and nothing else: `is_cloud_multipart_file_field` takes
only the template value and returns true for a `.path}}` binding or the legacy `{{inputs.image}}`
form. That is exactly what the Desktop cloud editor's multipart help text has always told authors to
write, and it is what the old ArtLoom contract the field-name rule came from actually meant. A field
bound to `{{inputs.x.value}}` travels as text no matter what it is called.

For a declared field the rendered value now takes one of three paths. A `data:` URL still decodes to
a binary part — that is the live path for image Arts, the one the Hook bridge feeds, since
`materialize_hook_art_inputs` hands inputs to the cloud arm as data URLs. An `http://` or `https://`
value travels as text, because some hosted APIs take the image as a URL in the same field an author
binds a path to and a remote URL is not a local file. Anything else goes through
`cloud_multipart_upload_path`, which canonicalizes it, requires a real file, and requires that file
to sit under a root Loom owns: the declared `metadata.artPackage.dir`, the control plane root
(reusing `art_settings::control_plane_root_for_tool`, now `pub(crate)`), or the host temp directory
the daemon stages call inputs in. A path that resolves nowhere allowed is a `CloudTemplate` error
naming the field, not a silent fallback to a text part.

The temp directory is treated more narrowly than the other roots because it is shared with every
other program on the machine: `cloud_upload_root_allows` only accepts a temp path whose first segment
starts with `loom-`, which is the prefix every temp directory in this workspace uses, including the
legacy `loom-cloud-input-<pid>-<stamp>.png` staging file the Phase 21 audit describes. Any other
allowed root vouches for its whole subtree, so a control plane root that happens to live under temp —
which is exactly how the daemon tests are set up — is unaffected.

Three tests: `a_multipart_field_named_file_no_longer_uploads_a_caller_named_path` drives a real
multipart request through the fixture server with a field literally named `file` bound to
`{{inputs.file.value}}` and asserts the request carries no `filename=` and none of the named file's
bytes; `a_declared_multipart_upload_path_has_to_sit_inside_a_loom_owned_root` covers accept-inside,
refuse-outside, refuse-directory, refuse-missing, and accept-inside-a-declared-package-directory;
`only_a_declared_path_binding_makes_a_multipart_field_a_file` pins the recognition rule itself.
`docs/plugin-permissions.md` gained an enforcement-matrix row, and the Phase 21 audit document now
says plainly that the file-field recognition it records is the old ArtLoom behaviour rather than
Loom's.

This batch also closed the release-smoke fallout from F8o. `scripts/smoke-release.ps1` registers three
cloud tools against a loopback fixture (`fixture-cloud-text`, `fixture-cloud-art`,
`fixture-cloud-multipart-art`) and `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1` registers a
fourth (`store-cloud-art`); none declared `permissionPolicy.network.allowLocalhost`, so after the F8o
default flip every one of them would have been refused at execution time. All four now declare it the
way a real local-service Art would. The smoke's multipart evidence field also still looked for
`filename="loom-cloud-input-`, the legacy staged-temp-file shape, which the data-URL path renders as
`loom-cloud-input.png`; it now matches the shared prefix. Those two scripts sit in paths reserved by
neither lane.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean, and
`cargo test --locked --workspace` at 592 passed, 0 failed, with `loom_tool_registry` at 146 tests. No
flakes in either full-suite run.

### F8q — done (S6b2d2-1, cloud request template injection)

`substitute_cloud_template` was a plain `str::replace` of `{{key}}`, `{{inputs.key}}`,
`{{inputs.key.value}}`, and `{{inputs.key.path}}` with the raw argument, applied identically to the
endpoint, the header block, and the request body. Arguments on that path come from the canvas and the
model, so they can carry content an attacker influenced, and text splicing let them change the shape
of the request rather than only its values. Two consequences mattered. An endpoint of
`https://api.example.com{{inputs.suffix}}` with a suffix of `@127.0.0.1:8787/` rendered a URL whose
host was the injected authority and whose userinfo was the author's intended host, which is a
credential-exfiltration primitive as soon as the injected authority is remote and the Art sends an API
key header. And a body of `{"prompt":"{{inputs.text}}"}` with a `text` of `x","stream":true` still
parsed as JSON, so the caller could add or override members the author never exposed; the header block
had the same shape and the same problem.

Rendering is now destination-aware. `substitute_cloud_template_with` takes the escaping rule as an
argument and `substitute_cloud_template` is the plain-text case that multipart field values and a
non-JSON body keep using, because neither position has structure an argument could break out of. The
endpoint renders through `percent_encode_cloud_template_value`, which keeps the unreserved set and
encodes everything else, so a substituted value can only ever be one path segment or one parameter
value — it cannot end the path, open a query, or introduce userinfo. The existing route and mode
bindings are unaffected: a grep of `art-packages`, `framework-packages`, `mcp-server-packages`,
`scripts`, `apps`, and `crates` found three templated endpoints in total, all single-segment values.
`validate_rendered_cloud_authority` then states the invariant outright: when the author wrote a fixed
authority, the rendered endpoint has to still carry exactly that authority. An author who templates
the host itself is trusted to have meant it, and the declared domain list is what constrains that
case — `validate_outbound_url` already runs on the rendered URL, which is the "re-validate after
substitution" half of the review's fix and needed no change.

The header block and a JSON body render through `render_cloud_json_template`, which parses the
template first and substitutes into the parsed document's strings — object keys included — so every
argument stays one string value whatever punctuation it carries. A template that is not valid JSON
before substitution, a placeholder standing in for an unquoted number being the real case, cannot be
parsed first and keeps the original splice-then-parse path; that is the one position where an argument
still reaches the serialized form, and it is the author's own choice to write it. Header names and
values are additionally refused when they carry a control character, so a value cannot split the
request on a lax client and the failure names the header instead of surfacing from inside the HTTP
client.

Five tests: `an_endpoint_argument_cannot_rewrite_the_request_authority` pins the encoding of the
`@127.0.0.1:8787/steal` suffix, that unreserved route and parameter bindings still render verbatim, and
the three authority-guard outcomes; `a_json_body_argument_cannot_add_sibling_fields` executes against
the loopback fixture with `x","stream":true,"model":"attacker` and asserts the captured body is a
one-member object whose `prompt` is that exact string; `a_header_argument_stays_one_header_value`
asserts the injected `X-Injected` never becomes a header and only one `X-Trace` line is sent;
`a_header_argument_carrying_a_line_break_is_refused` asserts the control-character error;
`a_json_body_template_that_is_not_json_yet_still_renders` covers the unquoted-number fallback and a
templated object key. `docs/plugin-permissions.md` gained a `Cloud request templating` enforcement row
and the Phase 21 audit's template-rendering section now records the destination-aware behaviour.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean, and
`cargo test --locked --workspace` at 597 passed, 0 failed, with `loom_tool_registry` at 151 tests. The
first full-suite run also hit `installs_independent_mcp_server_package` failing at
`crates/loom_mcp/src/package.rs:504`; it passes both alone and in the second full run, which puts it
with the known Windows temp-install flakes rather than with this change, which touches no MCP package
code.

### F8r — done (S6b2d3-1 and S6b2d3-3, MCP image download loopback and stall)

An MCP image-search tool returns candidate objects and Loom downloads the chosen candidate's image
itself. Both download paths — the reqwest attempt and the Windows PowerShell `HttpClient` fallback —
built their outbound policy inline with `allow_http_loopback: true` hardcoded. The candidate URL is
chosen entirely by the MCP server, so that handed any installed image-search server a request
primitive into the host's own local network: returning `http://127.0.0.1:<daemon-port>/v1/...`, a Hook
port, or a local model server as an image URL made Loom issue that request and hand the response body
back as an image. The declared `permissionPolicy.network` block existed and was ignored on this path.

`mcp_image_download_policy(tool)` now derives the policy from the tool's own declaration, reusing
`cloud_network_policy` exactly as a cloud Art does, and the policy is threaded as an
`&OutboundPolicy` through `normalize_mcp_result`, `normalize_mcp_image_result`,
`image_response_from_mcp_candidates`, the per-candidate and per-URL helpers, and both leaf
downloaders; the PowerShell fallback validates the URL against the same policy before it spawns
anything. Loopback and private networks are therefore off unless the package declares them. The
declared `domains` list is deliberately *not* applied to image downloads: an image-search tool's
declared domains name its API host, and the images the results point at live on whatever CDN the
upstream service uses, so an allowlist there would break every real search. That choice is stated in
the code comment and asserted by a test rather than left implicit.

The second finding was the stall. One candidate expands into the image URL and then the thumbnail,
each of those into the URL as given and then the modifier-stripped form, and each of those into a
reqwest attempt and then the PowerShell fallback, with every attempt bounded only by the 30 s
`CLOUD_API_TIMEOUT`. A result whose candidates all point at a host that accepts the connection and
never answers held one tool call for minutes per candidate, and a result carrying the full
`MAX_MCP_IMAGE_CANDIDATES` of 64 for roughly an hour. The loop now runs against one
`McpImageDownloadDeadline` created from a 90 s `MCP_IMAGE_DOWNLOAD_BUDGET`; `next_attempt_timeout()`
is re-read before every network attempt and returns the smaller of the remaining budget and
`CLOUD_API_TIMEOUT`, or `None` once less than `MIN_MCP_IMAGE_ATTEMPT_TIMEOUT` (2 s) is left, so the
fallback cannot spend a budget the first attempt already used. The loop also stops after
`MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS` (6) candidates, because reporting a failed search beats spending the
whole budget walking dozens of unfetchable candidates. On the PowerShell path the same value is passed
both into the script as `LOOM_FETCH_TIMEOUT_SECONDS`, which the script applies to `$client.Timeout`,
and into `process.limits.timeout`, so whichever fires first the attempt ends inside the caller's
remaining budget instead of at a fixed 30 s.

S6b2d3-2 needed no work here: it was already fixed by an earlier batch.

Five tests. `an_mcp_image_candidate_is_not_downloaded_from_loopback_by_default` runs the same loopback
fixture value through `normalize_mcp_image_result` twice and asserts the default policy refuses it while
a declaring tool's policy downloads it; `an_mcp_image_download_policy_comes_from_the_tool_declaration`
pins both flags against an undeclared and a declaring tool and asserts the declared domains do not
reach the download policy; `mcp_image_candidate_downloads_stop_at_the_attempt_cap` places a live
fixture at the last index inside the cap and then one index past it, asserting the first is selected and
the second is never requested; `an_mcp_image_download_deadline_bounds_each_attempt` covers the exhausted,
fresh, and nearly-spent budgets; `an_exhausted_mcp_image_budget_stops_before_the_next_request` asserts a
zero budget refuses a reachable fixture that the same call downloads with budget left. Existing direct
callers of the download helpers gained a `loopback_mcp_image_policy()` argument, the two `execute_tool`
MCP image tests now declare `allowLocalhost` in tool metadata, and the daemon's
`daemon_hook_bridge_executes_mcp_image_search_art_node_image_output` fixture tool declares it too — a
test-only touch of `apps/daemon/src/lib.rs`, which is reserved by neither lane. No `scripts/**` file
registers an MCP image tool, so nothing outside the two crates needed the declaration.
`docs/plugin-permissions.md` gained an `MCP image download` enforcement row, and its `network.domains`
and `network.allowLocalhost` entries now state where each does and does not apply.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean, and
`cargo test --locked --workspace` all green with `loom_tool_registry` at 156 tests, up from the 151 it
stood at after F8q.

### F8s — done (handoff H10(3), a loopback test seam for the image-search sample Art)

Lane B reported that `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` now fails on the
`custom-image-search` case: five Arts install and execute, then the image-search Art rejects the test's
own fixture image with `MCP image search returned candidates, but none could be downloaded`. That is the
F8l/F8m SSRF guard behaving correctly. The fixture serves its image from `http://127.0.0.1:<port>/`,
and an image URL naming a loopback address is exactly the shape the guard exists to refuse, because
every such URL is chosen by an MCP server rather than by the Art.

The Art cannot distinguish a test fixture from an attacker, so the seam is explicit and lives outside
the package. `art-packages/samples/image-search/runtime/main.ps1` reads
`LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` once per process and caches it, accepting `1`, `true`, `yes`,
or `on` case-insensitively; anything else, including an unset variable, leaves the guard fully closed.
With the seam on, `Resolve-ImageDownloadTarget` skips the loopback rule for a single case: an address
written literally in the URL. A hostname that resolves to a loopback address — the DNS-rebinding shape
— stays refused, and every other blocked range (private, link-local, unique-local, IPv4-mapped
equivalents) stays refused with the seam on. The exemption is re-applied per redirect hop, because
`Resolve-ImageDownloadTarget` is called for the requested URL and again for every hop.

The variable is an environment variable rather than a manifest field on purpose. No package can set it:
an Art runtime manifest declares a command and its arguments and nothing else, `manifest.json` has no
environment section, and an MCP server package's `env` applies to the server process rather than to the
Art runtime. Only whoever launches Loom can turn the seam on. The alternative — reading the Art's
`context.granted_permissions` — was rejected because that field carries the *framework* manifest's
policy, so honouring an `allowLocalhost` there would relax loopback downloads for every Art on the
`mcp` framework in the shipped product.

`crates/loom_process/src/lib.rs` had to carry the name as well. Every Loom-managed spawn calls
`env_clear()` and rebuilds the environment from a fixed allowlist, and an Art runs two spawns deep — the
daemon spawns the framework runtime host, which spawns the Art entry, and both hops go through
`loom_process`. `LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` is now in both the Windows and non-Windows
`ALLOWED` arrays with a comment stating that it is a test seam, what it relaxes, and why no package can
set it. `supervised_process_inherits_the_image_search_loopback_seam` asserts the name survives the
scrub and that an unrelated `LOOM_`-prefixed variable does not, so the addition stays one named
exception rather than a prefix passthrough.

Writing the seam exposed a latent bug in the same function. `$addresses` was assigned from an `if`
whose literal-address branch produced a one-element array, and PowerShell unrolls that back to the
element, so `$addresses.Count` was reading a scalar. It works today only because the runtime does not
set `Set-StrictMode`; under strict mode it throws `The property 'Count' cannot be found on this
object`, which would have turned every literal-IP image URL into a download failure the moment the
runtime gained a strict-mode line. The whole `if` is now wrapped in `@(...)`.

Verified: `cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_process` 8 passed, and `cargo test --locked --workspace` green apart from
one run of the known `loom-daemon` full-suite flake, which passes in isolation (`-p loom-daemon --lib`,
225 passed). On the PowerShell side the runtime parses cleanly, and an ad-hoc harness that loads the
three guard functions out of `main.ps1` by AST confirmed all eight cases: seam off refuses a literal
loopback URL; seam on allows literal `127.0.0.1` and `[::1]`; seam on still refuses `localhost`, a
private address, and the `169.254.169.254` metadata address; an unrecognised seam value refuses; and a
public address is unaffected. `scripts/Build-LoomSampleArtPackages.ps1` then repackaged the sample Arts
(the store zips were stale for every sample Art, not just this one), and
`scripts/tests/Test-LoomSampleArtRuntime.ps1` passed all 11 cases against the fresh packages, including
`custom-image-search: rejected an MCP candidate naming a loopback service` — the seam-off default still
blocks the SSRF case.

Seam-on coverage followed in the same handoff, in `scripts/tests/Test-LoomSampleArtRuntime.ps1` — a path
reserved by neither lane, so the touch was announced on the board first per handoff H4. The new case feeds
the image-search Art an MCP candidate whose image URL is `http://127.0.0.1:<port>/fixture.png`, sets
`LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES=1` for that one execution, and asserts the download actually
happened rather than merely that the Art reported success. Three pieces make that possible:

- `scripts/tests/fixtures/LoopbackImageFixture.ps1` (new) serves one PNG from an `HttpListener` bound to
  `127.0.0.1`, records every request line to a file, and 404s any path other than `/fixture.png`. It runs
  as its own process because the Art runtime blocks the calling script while it downloads, so the server
  cannot share the test's thread.
- `Invoke-Runtime` now copies an optional per-case `environment` hashtable into the child
  `ProcessStartInfo`, which is how the seam reaches the Art without leaking into the other cases.
- The case loop starts the fixture before the execution, reads the request log, stops the fixture, and
  requires at least one `GET /fixture.png`. That assertion is what pins the seam to a real network read:
  the seam-off case immediately above it names a loopback port and must still be refused, so the two cases
  bracket the behaviour from both sides.

The existing multi-candidate case grew an explicit `expectSelectedSecondCandidate` flag, because the
`result_index` assertions used to key off `$Case.id -eq "custom-image-search"` alone and would otherwise
have run against the seam case, which selects the only candidate it is given. Verified: both files parse,
and the smoke passes all 12 cases, with
`PASS custom-image-search (a literal loopback candidate with the download seam enabled)` beside the
unchanged `PASS custom-image-search: rejected an MCP candidate naming a loopback service`.

### F8t — done (S8b2-2, `Blend-Bitmaps` no longer walks the image a pixel at a time)

`art-packages/shared/image-runtime-common.ps1` blended two bitmaps with a nested PowerShell loop over
`Height × Width`, and every iteration paid two `GetPixel` calls, four roundings, a `Color::FromArgb`, and
a `SetPixel`. Each pixel accessor locks and unlocks the bitmap's bits by itself, so a 1920×1080 blend was
roughly 6.2 million GDI+ interop calls on top of 2.1 million interpreted loop iterations, and a 4000×3000
blend ran past `DEFAULT_FRAMEWORK_PROCESS_TIMEOUT` (120 s, `crates/loom_tool_registry/src/framework_process.rs`)
— the host killed the runtime and the user saw a timeout rather than a slow blend.

The loop is now two GDI+ draws, the spelling `Resize-BitmapArgb` next door already used: the source is
copied with `CompositingMode::SourceCopy`, then the reference is drawn over it with
`CompositingMode::SourceOver` and an `ImageAttributes` carrying a `ColorMatrix` whose `Matrix33` is the mix
ratio. Wherever both layers are opaque that is `source * (1 - ratio) + reference * ratio` per colour
channel, which is exactly what the loop computed. The two differ only where a layer is transparent, and
the composite is the better answer: the loop read a transparent pixel's colour as black and darkened the
other layer with it, and it turned an opaque source semi-transparent wherever the reference was absent.
Two details are load-bearing and were both found by test rather than by reading. `InterpolationMode` is
pinned to `NearestNeighbor` so a 1:1 draw stays a copy. The reference draw sets
`WrapMode::TileFlipXY` on the `ImageAttributes`, because GDI+ otherwise samples past the edge of the source
rectangle and leaves the outermost row and column of the composite carrying the source layer only; a first
attempt that instead set `PixelOffsetMode::Half` had that exact defect.

Verified with an ad-hoc harness under `Set-StrictMode -Version Latest`: the blend matches the old
per-channel arithmetic within one count at ratios 0, 0.25, 0.5, 0.75, and 1.0; every pixel of a 16×9
composite is blended at ratios 0, 0.5, and 1.0, which is the check that caught the edge defect; ratios
outside `[0, 1]` still clamp; a reference smaller than the source is still resized to fit, and its
semi-transparent bicubic edge ring composites as a compositing case should rather than as a plain copy. A
1920×1080 blend now takes about 50 ms. `scripts/Build-LoomSampleArtPackages.ps1` repackaged the seven
sample Arts so the store zips carry the new helper, after which
`scripts/tests/Test-LoomSampleArtRuntime.ps1` passed all 12 cases including
`custom-image-blend-script`, and `scripts/tests/Test-LoomSampleArtPackageContract.ps1` passed all seven
packages.

### F8u — done (S7a-3, and the reuse half of S7a-4: an installed MCP server package is now verified before it is spawned)

An MCP server package recorded one digest — the archive's — and nothing ever read it back. The installer
wrote `mcp/packages/<publisher>/<id>/active.json` and no code in the workspace opened that file except
`verify_active_mcp_package` in `crates/loom_tool_registry/src/install.rs`, which checked the identity,
version, and archive digest an Art's dependency claimed and hashed no bytes at all. So the digest was a
label rather than evidence: once a version directory existed, whatever it contained was what got launched.
That matters because a stdio transport's command is a file inside that directory, and the daemon spawns it
with the user's resolved credentials in its environment. Replacing just the entry script under an otherwise
untouched package was undetectable and gave the replacement those credentials.

Three changes, all in `crates/loom_mcp/src/package.rs` unless noted.

The installer now hashes every extracted file. `digest_tree` walks the staging root before it is renamed
into place and returns a `BTreeMap<String, String>` of package-relative path (`/` separators, via
`package_key`, which accepts `Component::Normal` only) to SHA-256. It uses `fs::symlink_metadata`, so a
symlink is seen as a link rather than followed, and anything that is not a regular file is an `UnsafePath`
error. The map is stored in both `McpServerPackageState.files` (new field on the type in
`crates/loom_mcp/src/lib.rs`, `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]` so existing
`servers.json` rows still deserialize) and in `active.json`.

`active.json` has a reader, and one reader. `McpPackageActiveState` is now a public type with the `files`
map on it, `read_active_state` is the canonical accessor, and `install.rs` no longer carries its own private
duplicate of the struct — `verify_active_mcp_package` calls the shared reader. The state file is therefore
the authoritative record of what was installed instead of a decoration.

`verify_installed_entry` runs before every spawn. `StdioMcpClient::spawn_with_timeout` calls it after the
transport check and before it builds the command, and a failure surfaces as the new
`McpError::PackageIntegrity`. It requires the config to carry package state and a `Stdio` transport, strips
`config.command` to a relative key under the recorded `package_dir`, reads `active.json` from that
directory's grandparent, checks the active state agrees with the config on qualified id, version, digest,
and package directory, and then compares the entry file's digest on disk against the recorded one. A
package whose `active.json` records no digest for its entry is refused with an error telling the user to
reinstall: the alternative is spawning an unverifiable executable with the user's credentials, so this fails
closed. The one fixture affected is the credential-resolution test in
`crates/loom_tool_registry/src/framework_process.rs`, which never spawns.

This is not a substitute for signing — a writer who can edit the version directory can edit `active.json`
beside it. It closes the entry-replacement case and is the check a publisher signature will hang off. The
signature and certification chain itself remains open as S7a-2.

The reuse half of S7a-4 came along with it. `install_server_package` used to see an existing
`versions/<version>-<digest[..12]>` directory, delete the freshly extracted staging tree, and return the old
one unexamined, so reinstalling over a tampered package was a no-op that kept the tampering. It now calls
`verify_tree_digests` against the digests just computed and fails with `Integrity` on a missing, modified,
or extra file, which makes a reinstall a repair. The naming half of S7a-4 — keying the directory on the full
digest rather than a 48-bit prefix — is deliberately still open: `&digest[..12]` is the Art installer's
convention in about eight places in `install.rs`, and lengthening the path costs Windows `MAX_PATH` headroom
for no attack this check does not already catch.

Four new tests in `package.rs` plus an extension of `installs_independent_mcp_server_package`, which now
asserts the recorded file set and that `active.json` matches the returned state:
`refuses_to_spawn_a_package_whose_entry_was_replaced`,
`refuses_to_spawn_a_package_with_no_recorded_digests`,
`refuses_to_reinstall_over_a_tampered_version_directory`, and
`refuses_to_spawn_a_package_backed_server_whose_entry_was_replaced`, which drives the real
`StdioMcpClient::spawn_with_timeout` so the gate is proven on the spawn path and not only in the checker.
`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_mcp` 28 passed, `cargo test --locked -p loom_tool_registry` 156 passed.

### F8v — done (S7b1-1, a packaged server's command is re-anchored inside its package at spawn)

`spawn_command_spec` took `config.command` exactly as `servers.json` spelled it, and
`McpServerConfig::validate` asked only that it be non-blank. A registry row could therefore keep an
installed package's block — publisher, version, archive digest, package directory — while pointing
`command` at any file on the machine, and the operator UI would still show it as that package. F8u's
digest check already derives the entry key by stripping `package_dir` off the command, so that half is
covered; this closes the two ways the check itself could be walked around.

The lexical strip is now backed by a resolved comparison. `verify_installed_entry` canonicalizes both
`package.package_dir` and the command and requires the resolved command to sit under the resolved
directory, so a link planted inside the package directory that resolves elsewhere is refused rather
than hashed-and-run. A package directory or entry that cannot be resolved at all is refused too.

An extensionless entry is refused on Windows. The platform resolves such a command itself: every
`PATHEXT` entry is appended in turn, so `runtime/server` can start `runtime/server.exe` — a file the
digest check never saw, because it hashed `runtime/server`. `resolve_windows_spawn_command`
(`crates/loom_mcp/src/lib.rs`) additionally returns early for `config.package.is_some()` before it
consults `PATHEXT` or `PATH`, so no packaged server goes through a search path even if it reaches that
code by another route. Unpackaged stdio servers are unchanged, including the extensionless `.cmd`
fixture that covers that path.

S7b1-2 (a relative command resolved against the daemon's CWD) and S7b1-4 (`.bat`/`.cmd` entry points)
are P3s and stay open; both are narrower now, since a packaged server can no longer reach a search
path and its command must resolve inside its own package directory.

Two new tests in `crates/loom_mcp/src/package.rs`:
`refuses_to_spawn_a_packaged_server_whose_command_points_outside_the_package` and, under
`cfg(windows)`, `refuses_to_spawn_a_packaged_entry_without_a_file_extension`, which installs a package
whose manifest entry is `runtime/server`. The test fixture builder grew
`package_bytes_with_entry` so an entry name other than `runtime/server.ps1` is expressible.
`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_mcp` 30 passed, `cargo test --locked -p loom_tool_registry` 156 passed,
`cargo test --locked -p loom-daemon` 225 + 8 passed.

### F8w — done (S7a-2 batch 1: MCP server packages go through the trust chain Art packages already use)

An MCP server package used to install on the strength of parsing: any zip whose `mcp.server.json`
validated was extracted and activated, whoever produced it. Arts have carried a publisher signature
and a trust policy for some time, and the same `plugin-trust.json` sits next to both, so an operator
who set `require-signed` or `require-trusted` reasonably believed it covered every package the
control plane installs. It covered Arts only.

`McpServerPackageManifest` now accepts an optional `packageSecurity` block holding the same
`PackageSignature` an Art's `metadata.packageSecurity` carries — one signing tool, one trust store,
one verifier for both package kinds. It deliberately holds no publisher identity of its own: the
manifest already names its publisher, and a second copy inside the security block would only give
two places to disagree.

`install_server_package` calls the new `verify_package_trust` immediately after `validate_manifest`,
while the package is still in staging and before anything is hashed or moved into place. The helper
loads `<control_plane_root>/plugin-trust.json` through `TrustStore::load`, builds a
`PublisherIdentity` from the manifest's publisher (with the signature's key id attached), calls
`verify_package_signature`, and then hands the resulting status to
`trust_store.effective_policy().enforce(...)`. Every failure becomes the new
`McpPackageError::Trust`. This mirrors `crates/loom_tool_registry/src/install.rs:531-547` step for
step, with one difference: the Art path only enforces for `ArtInstallSource::ExternalPackage`,
whereas every MCP server package is external by construction, so there is no equivalent exemption
here.

Passing the manifest's publisher to the verifier is what lets a signature made with a key this
machine already trusts for that publisher reach `Trusted` rather than stopping at `Verified`, which
is what makes `require-trusted` mean something for MCP servers. The stored policy still defaults to
`AllowUnsigned`, so unsigned packages — including every sample package in the repository — install
exactly as they did before, and enforcement stays opt-in.

Four tests in `crates/loom_mcp/src/package.rs`, each writing its policy into a per-test
`plugin-trust.json` rather than setting `LOOM_PLUGIN_TRUST_POLICY`, which is process-global and would
race under the parallel test harness: an unsigned package refused under `require-signed`; a signed
package accepted under `require-signed`, with the signature document itself hashed into the recorded
file list; a signed package repacked around a different runtime file refused on digest mismatch at
any policy; and, under `require-trusted`, a package signed by a trusted key accepted while one
signed by an unknown key is refused. The fixture builders grew `package_bytes_with_files`, and
`signed_package_bytes` builds a package the way a publisher would — lay the tree out, `sign_package`
it, then archive the tree together with the signature document.

What stays open for F8x: `verify_package_signature` returns `Verified`, not a failure, when a
package names a publisher that has no matching trust record, so under `require-signed` a package can
still claim `publisher.test` while being signed by an unrelated key. Binding the accepted publisher
id to the verified key, and persisting the resulting trust status into `McpServerPackageState` and
`active.json` so the UI can show it, is the second half of S7a-2.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_mcp` 34 passed, `cargo test --locked -p loom_tool_registry` 156 passed,
`cargo test --locked -p loom-daemon` 225 + 8 passed. `Cargo.lock` gained the `loom_mcp` →
`loom_plugin_security` edge. One note on the daemon suite: the first full run had
`daemon_returns_busy_when_request_queue_is_full` fail, and it passed both on its own and on an
immediate second full run — a load-sensitive flake in that queue-depth test, untouched by this
change.

### F8x — done (S7a-2 batch 2: a signature has to come from the key its publisher actually uses, and the verdict is recorded)

F8w left one hole open on purpose. `verify_package_signature` reports an unknown
`(publisher, key_id)` pair as `Verified` rather than as a failure, which is the right answer for a
publisher this machine has no opinion about and the wrong one for a publisher it has already pinned a
key for: anyone holding any valid Ed25519 key could sign a package, name `publisher.test` in the
manifest, and install it under `require-signed`, after which every surface would present it under
that borrowed name.

`enforce_publisher_key_binding` closes that. If the trust store holds any record for the publisher
the manifest names, the signature's key id has to be one of them; if it holds none, nothing happens,
because there is no pinned key to contradict and the policy alone decides whether `Verified` is
enough. Keeping the no-records case permissive is what stops the check from becoming a second,
stricter policy that ignores the operator's chosen one. A revoked record still matches by key id
here and is then rejected by `TrustPolicy::enforce`, which refuses `Revoked` at every policy level,
so revocation is unaffected by the ordering.

The trust verdict is now recorded rather than recomputed. `McpPackageActiveState` and
`McpServerPackageState` both carry `trust_status`, `write_active_state` and `config_from_manifest`
take it from the install-time check, and both fields default to `Unsigned` so a package installed
before signatures existed deserializes without a migration. A reader — the UI, `servers.json`, a
later audit — can now say whether an installed server was signed, and whether by a pinned key,
without re-verifying anything.

Two new assertions on the existing tests (the unsigned install records `Unsigned`; the signed install
under `require-signed` records `Verified` in both the returned config and `active.json`; the
trusted-key install records `Trusted`) and one new test,
`refuses_a_signature_from_a_key_the_publisher_does_not_use`: with `publisher.test` pinned to one key
under `require-signed`, a package signed by a second valid key is refused and nothing is installed,
while the pinned key's own package installs — which also covers the permissive no-records path
staying permissive.

The fixture in `crates/loom_tool_registry/src/framework_process.rs` that builds an
`McpServerPackageState` by hand gained `trust_status: loom_protocol::PackageTrustStatus::Unsigned`.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_mcp` 35 passed, `cargo test --locked -p loom_tool_registry` 156 passed,
`cargo test --locked -p loom-daemon` 225 + 8 passed. S7a-2 is now closed; what remains in F8 is P3s
only, plus S7c1-1 and S7c2-1, which are Lane B's F13.

### F14 — done (H2 Loom half: the surface stream envelope has a named version and a schema that requires it)

Scope: the Loom side of S1-2, handed to Lane A in board row H2. Hook already validates the poll
response against a stated envelope; Loom answered with an inline string literal and no schema, so the
contract lived in one repository's assertions and nowhere in the other's source.

`crates/loom_protocol/src/surface.rs` gained
`pub const SURFACE_STREAM_PROTOCOL_VERSION: &str = "loom.surface-stream.v1";` next to
`SURFACE_PROTOCOL_VERSION`, and `apps/daemon/src/lib.rs` now emits that constant instead of the
literal. The wire value did not change, so Hook's own copy of the string still matches and nothing in
`Hook/` had to move.

`protocol/schemas/surface-stream.v1.schema.json` is new and declares the envelope H2 posted:
`protocolVersion` as a `const` of `loom.surface-stream.v1`, `next` as a non-negative integer, `reset`
as a boolean, and `messages` as an array of bridge events with `method` and `params`. All four are
required. That is deliberate and is the point H2 was most explicit about: Hook treats a missing
`protocolVersion` as a mismatch rather than as an older daemon, so declaring the field optional would
have left open exactly the hole the finding is about. `messages` items are not
`additionalProperties: false`, because the per-method payloads already have their own schema in
`surface-message.v1.schema.json` and this envelope should not become a second place that has to grow
whenever a surface event does.

The schema is embedded as `schemas::SURFACE_STREAM_V1` in `crates/loom_protocol/src/lib.rs` and
reachable from the CLI as `loom-plugin schema surface-stream`, which meant three small follow-on
edits: the schema-name list in the CLI help text, the name-to-constant match arm, and the
`embedded_schemas_are_valid_json` list. `scripts/build-release.ps1` lists the shipped schemas one
`New-SupportSpec` per file, so it gained an entry too, announced on the board as H12 under the H4
rule. `scripts/tests/Test-ReleaseIntegrityTamper.ps1` also carries a schema list, but it fabricates a
synthetic zip rather than mirroring the real artifact, so it was left alone and nothing there asserts
parity. `.github/workflows/ci.yml` validates only the five framework and Art schemas through the CLI;
that list was already missing every surface schema, so it is a separate question and was not widened
here — the new schema's JSON validity is covered by the `loom_protocol` and CLI tests.

Two tests hold the contract from both ends. `surface_stream_schema_pins_the_protocol_version_constant`
in `loom_protocol` fails if the schema's `const` drifts from the Rust constant or if any of the four
required fields is loosened. The existing
`surface_stream_isolated_by_authenticated_attachment_device` daemon test now also asserts
`response["protocolVersion"] == "loom.surface-stream.v1"` with the value spelled out rather than read
from the constant, so changing the wire value breaks a test in this repository and not only in
Hook's — which is what makes the two-repository announcement H2 asks for unavoidable.

`protocol/README.md` gained one sentence naming the new schema in the paragraph that already
described the resumable stream, including why the field is required.

`reset` being required in the schema does not oblige Hook to read it. Hook still drops it; that is
S1-3, which remains open and is not Lane B's.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_protocol` 26 passed, `cargo test --locked -p loom-plugin-cli` 9 passed,
`cargo test --locked -p loom-daemon` 225 + 8 passed.

### F9a — done (S9-1 batch 1: a budget harness, and the first budget — wire bytes for one surface action)

Scope: the first third of F9. S9-1 is that Loom has no performance gate at all, so this batch builds
the smallest thing a gate needs and then uses it once. The other two budgets S9-1 names — peak
resident memory for one framework process, and end-to-end art execution wall time — are F9b and F9c.

`crates/loom_perf` is a new leaf crate with no dependencies. It holds a named budget, an environment
override (`LOOM_PERF_MAX_<METRIC>`), a report line stating what share of the budget a measurement
used, and `assert_within`, which panics with a message naming the override variable. Three decisions
in it are deliberate. A budget is a plain integer rather than a distribution, because all three of
Loom's first budgets measure one operation and a generous fixed ceiling is worth more than a precise
number somebody has to re-baseline every week. A malformed override is an error rather than a fall
back to the default, because a typo in a CI variable would otherwise be indistinguishable from a
passing gate. And the success path prints the measurement, so a green run still records the number
under `--nocapture`.

The first budget is `surface_action_response_bytes` in `apps/daemon/src/surface_actions.rs`: one
completed declarative action, measured across everything Loom pushes to a Hook client for it — the
queued and succeeded acks, the committed patch, and the formal result. Measured 1,665 bytes; the
default ceiling is 5,120, about three times that. It is loose on purpose. Tightening it onto the
measured number would turn one legitimate new field into a red build, while three times headroom
still catches the regressions this review is actually worried about: re-sending a snapshot where a
patch would do (S8c2-1's neighbourhood), or serialising the same payload several times into one
message.

No new workflow was needed for this batch. `ci.yml` already runs `cargo test --locked --workspace`,
so both the crate's own tests and the budget test run on every push. F9c is where a separate
performance workflow starts to be worth having, because wall time on a shared runner is the one
number that needs its own schedule and its own tolerance.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_perf` 6 passed plus 1 doc test,
`cargo test --locked -p loom-daemon` 226 + 8 passed.

### F9b — done (S9-1 batch 2: peak memory for one framework process)

The second of the three budgets S9-1 asked for. Loom already wrapped every supervised process in a
Windows job object so it could enforce a memory limit and kill the whole tree
(`crates/loom_process/src/lib.rs`, `attach_process_isolation`), and a job object also keeps the
counter this budget needs. `isolation_peak_memory_bytes` reads `PeakJobMemoryUsed` through
`QueryInformationJobObject` while the group is still open, and `SupervisedOutput` gained
`peak_memory_bytes: Option<u64>`.

Three decisions worth stating.

The number is on `SupervisedOutput` and not on `ExecutionDiagnostics`. `ExecutionDiagnostics` is a
wire type: it is what a framework sends back about its own execution, and
`protocol/schemas/framework-execute-response.v1.schema.json` closes `diagnostics` with
`additionalProperties: false`. A framework cannot report its own peak memory — only its supervisor
can — so adding the field to the wire type would have meant a schema change, a field no framework
can populate, and a cross-repo announcement, all to carry a value that never leaves the daemon.

`None` means "not measured", not "measured zero". Outside Windows a process group is not an
accounting boundary and there is no counter to read; sampling `/proc` would measure when Loom
happened to look rather than the peak. Windows also reports an unrecorded counter as zero, and a
process that used no memory does not exist, so zero is folded into `None`. The budget test says so
out loud when it skips, rather than passing silently as though it had measured something small.

The measured child is PowerShell, because that is the runtime Loom's own sample art frameworks
execute on, so the number is the floor under every art execution: what the interpreter costs before
any work starts. Measured 65,544,192 bytes (about 63 MiB); the default ceiling is 256 MiB. That is
deliberately loose, and deliberately *below* the 512 MiB the default `ProcessLimits` enforce — a job
that reaches the enforced limit is killed, so a budget above it could never fail.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_process` 9 passed (was 8), the new test printing
`perf budget framework_process_peak_memory_bytes: 65544192 bytes of 268435456 bytes (24% of budget)`.

No board entry: every path in this batch (`crates/loom_process/**`) is Lane A reserved, no wire
format changed, and Hook sees nothing of it.

Still open from S9-1: F9c, end-to-end art execution wall time, plus the question of whether wall
time deserves a workflow of its own rather than riding on `cargo test --locked --workspace`.

### F9c — done (S9-1 batch 3: wall time for one art execution; F9 complete)

The last of the three budgets, and the one a user actually feels. It wraps a whole art execution
through `execute_framework_art_in_root_with_timeout`: resolving the package, partitioning arguments,
spawning the interpreter, writing the request to stdin, reading the response and normalising it.
Measured 1,562 ms warm; the ceiling is 10,000 ms.

The art is the existing test fixture rather than one of the shipped sample packages. A sample package
has to be built by `scripts/Build-LoomSampleArtPackages.ps1` before anything can execute it, and
`ci.yml` runs only `Test-LoomSampleArtPackageContract.ps1` (`ci.yml:101`) — the sample *runtime*
script is in no workflow, so a budget placed there would not have gated a single push. The fixture
keeps everything that costs time: a real package on disk, a real PowerShell process, the real
supervisor. What it leaves out is the framework's own work, which no budget in this repository could
bound anyway.

The measured execution is the second one. A framework package is installed once and executed many
times, so warm is the representative case, and the first execution additionally pays for the
operating system caching the interpreter the fixture had just copied — an artefact of the fixture,
not a per-execution cost.

Answering F9b's open question: no separate performance workflow. Hook needs one because its shader
gate is slow and weekly; all three Loom budgets together add about two seconds to a test run that
already takes minutes, and `ci.yml` already runs `cargo test --locked --workspace` on
`windows-latest` for every push. Moving them to a weekly schedule would trade instant attribution —
the commit that blew the budget is the commit that went red — for a week-long window in which any of
a hundred commits could be the cause. The generous ceilings exist precisely so that riding the normal
test job is safe: 6× headroom on wall time, 4× on peak memory, 3× on wire bytes, which is noise
tolerance rather than measurement precision.

`cargo fmt --all -- --check` clean, `cargo check --locked --all-targets` clean,
`cargo test --locked -p loom_tool_registry` 157 passed (was 156), the new test printing
`perf budget art_execution_wall_time_ms: 1562 ms of 10000 ms (16% of budget)`.

No board entry: `crates/loom_tool_registry/**` is Lane A reserved and nothing on the wire changed.

F9 is complete. S9-1 asked for a performance gate where Loom had none; it now has three budgets, one
harness, and one override convention (`LOOM_PERF_MAX_<METRIC>`), all running on every push.

### F11a — done (S8b2 batch 1: shared image helper semantics)

Closes S8b2-3, S8b2-4 and S8b2-5, all three in `art-packages/shared/image-runtime-common.ps1`, the
file every framework image Art dot-sources as `runtime/common.ps1`.

S8b2-3, error preference. The file no longer assigns `$ErrorActionPreference`. A dot-sourced file runs
in its caller's scope, so that assignment was the library overwriting a decision that belongs to the
entry point. The four samples that dot-source it now set `$ErrorActionPreference = "Stop"` themselves,
on the line above the dot-source rather than below it, so the dot-source itself also runs with
terminating errors — `image-search` already set it, one line too late. `stock-monitor` does not
dot-source this helper and already sets both the preference and `Set-StrictMode`, so it is untouched.
`Set-StrictMode` for the shared helper stays out of scope here: turning it on inside a dot-sourced
file would impose it on the caller in exactly the way this finding objects to, and turning it on in
each sample is a behaviour change to five scripts that deserves its own batch.

S8b2-3, GDI+ load. `Add-Type -AssemblyName System.Drawing` moved out of file scope into a new
`Initialize-ImageRuntime`, which the five helpers that construct a bitmap from something other than a
bitmap now call first: `Load-BitmapArgb`, `New-ImageThumbnailDataUrl`, `New-ImageOutput`,
`New-ImagePathOutput`, `New-PlaceholderImage`. `Resize-BitmapArgb`, `Blend-Bitmaps` and `Save-Png`
take a `[System.Drawing.Bitmap]` parameter, so a caller cannot reach them without having loaded the
assembly through one of the five. The idempotence check is `'System.Drawing.Bitmap' -as [type]`
rather than a flag variable, because a flag at file scope would be library state written into the
caller's scope — the same objection as the error preference — and the type test is also correct when
something else in the process loaded GDI+ first.

S8b2-4. The unreachable `IDictionary` fallback is gone; `Get-RequestInputValue` is now the two lines
that read `inputs` and consult the declared names. The comment records why it was removed rather than
repaired, so the next reader does not restore it: `ConvertFrom-Json` yields `PSCustomObject`, so it
never ran, and the one shape where it would have run is the shape where picking by hash order feeds an
Art whichever input came first.

S8b2-5. `Get-JsonPropertyFromNames` now skips a value that is a string and whitespace-only, and
continues to the next candidate name instead of returning it. Only strings are filtered: `0` and
`$false` are values a caller meant to send, and `Get-RequestParamValue` must keep returning them
rather than falling through to its default.

Verified. A probe dot-sourced the helper in a fresh `powershell -NoProfile` and printed:
`drawing-after-dotsource: False`, `drawing-after-init: True` (so the assembly load really is deferred
and `Initialize-ImageRuntime` really performs it), `blank-then-url:
https://example.invalid/a.png` for `{"path":"","url":"..."}` (S8b2-5's exact reported shape),
`whitespace-skip: real`, `zero-kept: 0`, `false-kept: False`, `unmatched-names: []`, and
`caller-preference-after-dotsource: Continue` from a second probe that chose `Continue` before
dot-sourcing. Then `scripts\tests\Test-LoomSampleArtRuntime.ps1` passed all 12 execution and rejection
cases, including `custom-image-search` with a loopback candidate and the download seam enabled, and
`scripts\tests\Test-LoomSampleArtPackageContract.ps1` — the script `ci.yml:101` runs — passed for all
7 packages.

One pre-existing failure, not caused by this batch and not fixed by it.
`scripts\tests\Test-LoomSampleArtInstallExecution.ps1` installs and executes each sample package for
real; it passed `custom-1770146354922`, `custom-remove-bg-cloud`, `custom-1770131241684`,
`custom-image-blend-script` and `custom-image-blend-compress-workflow`, then failed at
`custom-image-search` with `image_search_failed: MCP image search returned candidates, but none could
be downloaded`. The cause is the loopback download guard, which this batch does not touch: the test
serves its fixture image from `http://127.0.0.1:<port>/fixture.png`, and reaching it requires
`LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES`, which that script never sets — only
`Test-LoomSampleArtRuntime.ps1:188` does, per-Art. Exporting the variable into the test process did
not help either, because the environment allowlist that carries it through both spawns
(`crates/loom_process/src/lib.rs:259`) is uncommitted, and the test runs a prebuilt
`target\debug\loom-daemon.exe` that predates it. Rebuilding the daemon to confirm is currently
impossible: `cargo build --locked -p loom-daemon` fails with 8 `E0425` errors in
`apps/daemon/src/surface_resources.rs` (missing `runtime_log_warn`, `load_stored_resource`,
`file_modified_millis`), a file last written at 13:45 today — mid-edit work in an unreserved path,
not this lane's and not this batch's. Two follow-ups belong to whoever owns those paths: the install
test should set the seam variable itself instead of inheriting it ambiently (an `scripts/**` edit, so
H4 announcement first), and `surface_resources.rs` has to compile again before F10 can verify
anything.

No board entry: `art-packages/shared/**` and `art-packages/samples/image-search/**` are Lane A
reserved, and `image-blend`, `image-compress` and `remove-bg` are unreserved by either lane — H4
covers `scripts/**` only. Nothing on the wire changed and no Rust source was touched, so
`cargo fmt`/`cargo check` are unaffected by this batch.

S8b2-6 is deliberately left for the next batch: a per-process work root, matching suffixes in the
generated filenames across the image samples, and validation of a requested root the way the host
validates output paths.

### F11b — done (S8b2 batch 2: fallback work root and generated filenames)

Closes S8b2-6. `art-packages/shared/image-runtime-common.ps1` gained two functions and
`Get-RequestWorkRoot` was rewritten; three image samples now build their output filename through the
new helper.

`Get-ArtRuntimeInstanceId:16` returns a value unique to the running process — the process id joined
to its start time (`HHmmssfff`). The finding asked for a per-process GUID; a GUID was rejected because
it would have to be generated once and then remembered, and the only scope a dot-sourced file can
remember anything in is its caller's — precisely the coupling S8b2-3 objected to. The pair needs no
memory: only one live process holds a given id at a time, and a reused id necessarily carries a later
start time, so the value is both unique among concurrent runs and stable within one run. It is derived
on each call and cached nowhere.

`New-WorkRootFileName:29` takes a stem and an optional extension (default `.png`) and returns
`<stem>-<instance-id><extension>`. Every filename an Art generates inside the work root now goes
through it: `Resolve-ImagePath:337` for the file a data-URL input is decoded into
(`"$Label-input"`), and the three sample outputs — `image-blend/runtime/main.ps1:28`,
`image-compress/runtime/main.ps1:18`, `remove-bg/runtime/main.ps1:18`. Those were the only fixed
filenames left: a repository-wide grep for `Join-Path $workRoot` and for `Save-Png` call sites found
nothing else, and `New-PlaceholderImage` has no live caller at all (only `target/` build artefacts and
this document mention it), so it takes its path from whoever eventually calls it.

`Get-RequestWorkRoot:84` now separates the two cases explicitly. A requested `tempDir`/`cacheDir` is
validated before `New-Item -Force` can bring it into existence: `Test-RemoteOrDevicePath` rejects the
UNC and device spellings, mirroring `crates/loom_security/src/network.rs:188
validate_local_path`, and a second check requires the path to be absolute, because a relative root
resolves against whichever directory the process happened to be spawned in. Both failures throw
rather than falling back silently — a caller that named a root and got a different one would write its
outputs somewhere it is not looking. With no requested root, the fallback is
`%TEMP%\loom-art-package-runtime\<instance-id>` instead of the single shared
`%TEMP%\loom-art-package-runtime`, so concurrent off-host runs no longer share a directory. A
whitespace-only `tempDir` reaches neither branch's validation: `Get-JsonPropertyFromNames` skips blank
strings as of F11a, so it is treated as absent and lands on the per-process fallback, which is the
safe outcome.

The generated files still are not deleted by this file. Production does not need them to be —
`crates/loom_tool_registry/src/framework_process.rs:258-263` supplies a per-request
`%TEMP%\loom-framework\<request_id>` under `TempDirectoryGuard` and removes it afterwards — and an Art
cannot delete its own output before its caller has read it, so cleanup stays with the caller that
chose to omit `context`.

Verified with a probe dot-sourcing the helper directly: `instance-id: 44344-135825125`,
`instance-id-stable: True`, `filename: image-blend-output-44344-135825125.png`, `filename-ext:
x-44344-135825125.jpg`; `unc: threw -> requested work root '\\server\share\loom' names a remote or
device path`, `unc-forward` and `device` the same, `relative: threw -> requested work root
'relative\loom' must be an absolute path`; `fallback:
C:\Users\vmjcv\AppData\Local\Temp\loom-art-package-runtime\44344-135825125` with `fallback-has-id:
True` and `fallback-exists: True`; an absolute requested root was accepted verbatim; a whitespace
`tempDir` produced the fallback root. A second run of the same probe printed a different instance id
(`48340-135752248`), confirming two processes do not collide.

`scripts\tests\Test-LoomSampleArtRuntime.ps1` — "Curated Art runtime smoke passed for 12 execution and
rejection cases." `scripts\tests\Test-LoomSampleArtPackageContract.ps1` — "Sample Art package contract
passed for 7 packages." `Test-LoomSampleArtInstallExecution.ps1` was not re-run: it still stops at the
pre-existing `custom-image-search` loopback failure recorded under F11a, whose cause (an uncommitted
environment allowlist plus a daemon that cannot currently be rebuilt) this batch does not touch.

No board entry, for the same reason as F11a: `art-packages/shared/**` is Lane A reserved and
`image-blend`, `image-compress` and `remove-bg` are unreserved by either lane. No Rust source was
touched and nothing on the wire changed.

S8b2-7 (unbounded data-URL decode, always written `.png`) and S8b2-8 (GDI+ objects leaked on partial
failure in `Load-BitmapArgb`, `Resize-BitmapArgb`, `Blend-Bitmaps`) remain open and belong to later
batches.

### F11c — done (S8b2 batch 3: data-URL size ceiling and subtype mapping)

Closes S8b2-7. All three edits are in `art-packages/shared/image-runtime-common.ps1`.

`Get-ImageDataUrlMaxEncodedLength:307` returns 44,739,244 — 32 MiB of decoded image expressed in
base64 characters, four characters per three bytes, rounded up. The number is not new: the host already
applies a 32 MiB ceiling on both paths that hand image bytes to a framework Art
(`MAX_FRAMEWORK_CANDIDATE_BYTES` in `crates/loom_tool_registry/src/framework_process.rs:972`,
`MAX_MCP_IMAGE_BYTES` in `crates/loom_tool_registry/src/lib.rs:53`) and `image-search` enforces the same
on a download (`image-search/runtime/main.ps1:417`), so a data-URL input now stops at the same place
the other two routes into this runtime stop.

The ceiling is checked in `Resolve-ImagePath` against `$text.Length`, gated on the value starting with
`data:`, and it is checked *before* the data-URL pattern runs. That ordering is the point: the capture
group copies the encoded payload, so a value that is already too long is rejected while only one copy
of it exists. Checking after the match would have made the guard pay the cost it exists to avoid.

`Resolve-ImageDataUrlExtension:316` maps the declared subtype to the extension the decoded bytes are
written under — `png`, `jpeg`/`jpg`/`pjpeg`, `bmp`/`x-ms-bmp`, `gif`, `tiff`, `x-icon`/
`vnd.microsoft.icon` — and throws on anything else, naming the subtype. The pattern in
`Resolve-ImagePath` now captures the subtype (`(?<subtype>[A-Za-z0-9.+-]+)`) and passes it through
this function, so `svg+xml` is refused where the message can explain itself instead of reaching
`Bitmap`'s constructor and failing with "the parameter is not valid". Comparison is
`ToLowerInvariant`, so `data:image/PNG` still resolves. `webp` and `avif` are refused for the same
reason as SVG — GDI+ cannot decode them; note that `image-search:83` still accepts `.webp`, `.svg` and
`.avif` in a *candidate URL* sniff, which is a different path (those bytes are forwarded as a data URL,
never decoded locally) and is unaffected.

A value that begins `data:` but does not match the base64 pattern still falls through to the confined
path resolver and comes back `$null`, exactly as before; no behaviour changed for it.

Verified with a probe dot-sourcing the helper: `max-encoded-length: 44739244`; `png: ok ->
probe-input-58844-141841774.png`, `jpeg: ok -> ...jpg`, `tiff: ok -> ...tif`, `icon: ok -> ...ico`,
`uppercase-PNG: ok -> ...png`; `svg: threw -> data URL image subtype 'svg+xml' is not a format this
runtime can decode`, `webp: threw -> data URL image subtype 'webp' is not a format this runtime can
decode`; `oversize: threw -> data URL image exceeds the 32 MiB this runtime accepts`; the
non-base64 `data:` value resolved to `$null`. The work root afterwards held exactly the four files the
four accepted subtypes wrote, each with its own extension.

`scripts\tests\Test-LoomSampleArtRuntime.ps1` — "Curated Art runtime smoke passed for 12 execution and
rejection cases." `scripts\tests\Test-LoomSampleArtPackageContract.ps1` — "Sample Art package contract
passed for 7 packages." A repository-wide grep over `scripts/`, `crates/`, `protocol/` and `apps/`
found only `data:image/png` and bare `data:image/` in fixtures and host code, so no existing caller
depended on a subtype this batch now refuses.

No board entry: the only file touched is Lane A reserved. No Rust source changed.

S8b2-8 (GDI+ objects leaked on partial failure in `Load-BitmapArgb`, `Resize-BitmapArgb`,
`Blend-Bitmaps`) and S8b2-9 (output builders decode the whole image just to read its dimensions)
remain open for later batches.

### F11d — done (S8b2 batch 4: bitmap helpers dispose on the failure path)

Closes S8b2-8. All three helpers in `art-packages/shared/image-runtime-common.ps1` now allocate their
result inside a `try` and dispose it from a `catch` that rethrows, so a failure between the allocation
and the `return` releases the surface instead of abandoning it.

`Load-BitmapArgb:415` initialises `$loaded` and `$bitmap` to `$null` and allocates both inside the
`try`; the `catch` disposes `$bitmap` and rethrows, the `finally` disposes `$loaded` if it exists. That
also closes a smaller hole the finding did not name: `[System.Drawing.Bitmap]::new($Path)` used to run
outside any `try`, so its own failure was fine, but nothing covered `Graphics::FromImage($bitmap)`
throwing after `$bitmap` existed. `Resize-BitmapArgb:456` does the same for `$resized`.
`Blend-Bitmaps:489` gained `$output = $null` before its `try` and a `catch` that disposes `$output`;
its existing `finally` still disposes `$referenceSized`, and `Resize-BitmapArgb` now cleans up after
itself if the reference resize is what fails.

The `catch` cannot run on the success path — the `return` is the last statement in the `try` — so the
caller still receives a bitmap nobody has disposed, and no double-dispose is possible. `$graphics` and
`$attributes` were already disposed in `finally` blocks and were not touched.

Verified with a probe dot-sourcing the helper. Success path unchanged: `load: 4x3
format=Format32bppArgb`, `resize: 8x6`, `blend: 4x3`, `saved: 119 bytes`. Failures still reach the
caller with the original exception rather than being swallowed by the new `catch`:
`resize-disposed: threw -> MethodInvocationException`, `blend-disposed: threw ->
MethodInvocationException`, `load-corrupt: threw -> MethodInvocationException`. Disposal is
deterministic rather than left to a finaliser: 200 consecutive failing `Resize-BitmapArgb -Width 1024
-Height 1024` calls, each of which allocates a 4 MiB surface before its `DrawImage` fails, moved private
bytes by 2.0 MiB in total (`repeat-failure-private-bytes-delta-mib: 2.0`) where 200 abandoned surfaces
would be roughly 800 MiB.

One limit on that evidence, stated plainly: the `catch` in `Blend-Bitmaps` could not be exercised from
outside. Reaching it needs a GDI+ failure *after* `$output` is allocated, and every input that fails at
all fails earlier — a disposed `Reference` fails inside the reference resize, a disposed `Source` fails
on `$Source.Width`, and an over-large `Source` fails in the `$output` allocation itself, where `$output`
is still `$null`. That branch rests on the same shape as the two that were exercised.

`scripts\tests\Test-LoomSampleArtRuntime.ps1` — "Curated Art runtime smoke passed for 12 execution and
rejection cases." `scripts\tests\Test-LoomSampleArtPackageContract.ps1` — "Sample Art package contract
passed for 7 packages." Those cover the success path through all three helpers, since `image-blend`
loads two bitmaps, resizes the reference and blends them.

No board entry: the only file touched is Lane A reserved. No Rust source changed.

S8b2-9 (the output builders decode the whole image just to read its dimensions, and `New-ImageOutput`
reads the file twice) is the last open finding in this slice and belongs to a later batch.

### F11e — done (F8 batch: what counts as an unlabelled base64 image)

Closes S6b2d3-4 and S6b2d2-9.

`looks_like_base64_payload` accepted any run of eight or more characters drawn from the base64
alphabets, so `"completed"`, `"12345678"`, a request id, a hex digest, and most slugs all qualified as
image payloads. Two consumers acted on that answer: `normalize_cloud_json_value` turned a `data`
string into `data:image/png;base64,<token>` — a broken image on the canvas with no diagnostic
anywhere — and `mcp_result_already_contains_image` short-circuited normalization for a text content
item that happened to be alphanumeric.

The predicate now requires a length of at least 1024 characters, a length that is a multiple of 4,
`=` padding only at the end and at most two of it, and a single alphabet (standard `+/` or URL-safe
`-_`, never a mixture). The kilobyte floor is the part that does most of the work: a base64 image
below it is smaller than anything these paths produce, while the values that used to be mistaken for
one all sit well under the bound.

Raising the floor would have broken a small labelled image, so `normalize_cloud_json_value` no longer
relies on the predicate alone. `cloud_json_image_response` accepts a `data` string as an image on any
of three signals — a `data:image/` prefix, an `image/*` `mimeType`/`mime_type`, or the strict
predicate — and returns `None` otherwise, which drops the value into the existing text handling. The
same helper now also gates the nested `output.data` branch, which previously returned an image
unconditionally and defaulted the MIME type to `image/png`; that was the same defect one field
deeper, in the same function, and no test exercised the old behaviour.

`crates/loom_workflow_runtime/src/lib.rs:1019` held a character-for-character copy of the old
predicate behind `is_image_like`, used by `extract_image_output` and `normalize_image_reference`. The
finding does not name that copy, but it is the same defect in a Lane A file, and leaving it would have
left the two crates disagreeing about the same values. Rather than fix it twice, the rule moved to
`loom_image_io::looks_like_base64_image_payload` (both crates already depend on that crate) and both
call sites now ask it. `normalize_image_reference` still falls through to its `path.is_file()` check,
so a short value that is a real file path keeps resolving as before.

Verified with `cargo test -p loom_image_io` (7 passed), `cargo test -p loom_tool_registry` (162
passed), and `cargo test -p loom_workflow_runtime` (16 passed), plus `cargo fmt` on the three crates.
New tests: three in `loom_image_io` covering ordinary text, a kilobyte-scale payload, and the
shape rules; four in `loom_tool_registry` covering a `data` string with no image signal, a nested
`output.data` with none, a short `data` string with an explicit `image/jpeg` label, and a data URL of
any length; and one asserting an alphanumeric text content item is no longer read as an image. Every
existing image fixture in both crates carries a `data:image/png;base64,` prefix, so none of them
depended on the loose predicate.

No board entry: `crates/loom_image_io`, `crates/loom_tool_registry`, and `crates/loom_workflow_runtime`
are all Lane A reserved.

Still open in these slices after this batch: S6b2d2-5, S6b2d2-6, S6b2d2-7, S6b2d2-8, S6b2d3-5,
S6b2d3-6, and S6b2d3-7. `cargo clippy` is not installed for the pinned 1.95.0 toolchain, so the
formatting check is the only lint evidence here.

### F11f — done (F8 batch: what counts as an image, and what a data URL has to prove)

Closes S6b2d3-5 and S6b2d3-7. Both findings are about the same moment — the point where a string
named by an untrusted MCP server becomes an image on the Hook canvas — so they were fixed together.

**S6b2d3-7, SVG.** Decided out of scope. An SVG is a document that can carry script and reference
remote content, and nothing between this crate and the canvas sandboxes it. The old behaviour was
also internally inconsistent: `infer_image_mime_type_from_bytes` has no SVG branch, so the same bytes
were accepted behind a `.svg` URL and rejected behind an extensionless one. The rule is now the same
everywhere: raster kinds only, named once in `IMAGE_URL_EXTENSIONS` and `SUPPORTED_IMAGE_MIME_TYPES`
in `crates/loom_tool_registry/src/lib.rs`. Four places changed to use them — candidate collection
(`looks_like_image_url`, which previously matched a second hand-written copy of the same extension
list), the reqwest download's `Content-Type` filter, the Windows PowerShell fallback's `contentType`
filter, and `infer_image_mime_type_from_url`, which no longer maps `.svg` to `image/svg+xml`. The two
declared-type filters went from `starts_with("image/")` to the `is_supported_image_mime_type`
allowlist, so `image/svg+xml` is now discarded like any other kind this crate will not deliver.

`MCP_IMAGE_FETCH_ACCEPT` still lists `image/svg+xml` and is deliberately unchanged. It is a
browser-shaped `Accept` header whose purpose is to look like a browser to hosts that vary their
response on it; what comes back is judged by the filters above and by the byte signature, not by what
was asked for.

**S6b2d3-5, data URLs.** `image_response_from_mcp_candidate_url` no longer forwards a data URL
verbatim. The new `image_response_from_image_data_url` keeps the cheap length bound first — so an
absurd string is refused while there is still one copy of it — then requires a `;base64` header,
decodes the payload, rejects an empty or over-ceiling result, and takes the MIME type from
`infer_image_mime_type_from_bytes`. The type the URL claimed is no longer trusted: a PNG labelled
`image/webp` now arrives as `image/png`, malformed base64 and an unrecognised payload are refused
outright, and an SVG data URL fails for the plain reason that SVG has no raster signature. The
re-encode from the decoded bytes also means the string handed onward is canonical base64 rather than
whatever whitespace or padding the server sent.

The Art that produces these candidates was brought in line in the same batch, because a rule enforced
only on the consuming side leaves the Art's own output — which goes to the canvas through
`content[0].data` — unchecked. In `art-packages/samples/image-search/runtime/main.ps1`:
`$script:ImageUrlExtensions` and `$script:SupportedImageMimeTypes` replace the two inline lists and
drop `.svg`; `Test-ImageLocation` now judges a `data:` URL by its media type instead of accepting any
`data:image/`; new `Get-DataUrlMediaType`, `Test-SupportedImageMimeType`, and
`Test-RefusedImageLocation` helpers carry the rule; an inline `image` block whose label is outside the
list is refused at conversion time, since inline data never reaches the download path; a structured
candidate naming a `.svg`/`.svgz` URL is dropped, and a refused *thumbnail* is dropped without
dropping the candidate; the download path refuses a response that declares an unsupported content
type, while a response that declares nothing still falls through to the URL as before; and
`Get-ImageMimeType` no longer maps `.svg` and no longer echoes an arbitrary `image/*`. A structured
URL is still not required to carry a known extension — search APIs routinely return extensionless
image URLs — it is refused only when it says outright that it is a kind the Art will not deliver.

Verified with `cargo test -p loom_tool_registry` (166 passed) and `cargo fmt` (clean). Four new tests:
a data URL identified from its bytes, a PNG mislabelled `image/webp` coming back as `image/png`, four
malformed or non-raster data URLs refused (invalid base64, empty payload, a non-base64 `data:` URL,
and a base64 SVG), and `.svg` URLs plus `image/svg+xml` types refused by `looks_like_image_url`,
`infer_image_mime_type_from_url`, and `is_supported_image_mime_type`.

For the Art, `scripts\tests\Test-LoomSampleArtRuntime.ps1` passed (12 execution and rejection cases,
including the two `custom-image-search` download cases) and
`scripts\tests\Test-LoomSampleArtPackageContract.ps1` passed (7 packages). The refusals themselves
were exercised directly by loading the runtime's declarations and calling them: a PNG data URL and a
`.png` URL still pass, while an SVG data URL, a plain (non-base64) SVG data URL, a `.svg` URL, a
`data:text/plain` URL, an inline `image/svg+xml` block, a structured `.svg` candidate, and a `.svg`
thumbnail are all refused; an extensionless URL is still accepted; and `Get-ImageMimeType` returns
`image/jpeg` for a declared JPEG, `image/webp` and `image/gif` for data URLs with and without
parameters, and never `image/svg+xml`.

No board entry: `crates/loom_tool_registry` and `art-packages/samples/image-search` are both Lane A
reserved.

Still open in these slices after this batch: S6b2d2-5, S6b2d2-6, S6b2d2-7, S6b2d2-8, S6b2d3-6, and
S6b2d3-8. S6b2d3-6 (URL-modifier stripping replacing a candidate's real URL) sits in
`strip_image_url_modifiers`, which this batch touched only to read the shared extension list, and it
needs the candidate to keep both forms rather than a tighter list, so it stays backlog. `cargo clippy`
remains unavailable for the pinned toolchain.

### F11g — done (F8 batch: how much borrowed text an error may carry, and the listing failure nobody saw)

Closes S6b2c3-10, S6b2d2-6, and the error-text half of S6b2d2-8.

Errors from this crate travel much further than the code that raises them: into the log, into the
Surface error payload, and into the canvas. Several of them embedded text whose size nothing in this
crate controls — a framework runtime's stdout or stderr, a cloud API's response body. A stray `print`
in a runtime, or an API that answers a failure with a megabyte of HTML, turned a diagnosable error
into a payload nobody can read or store. One crate-local helper now bounds all of it:
`bounded_error_text` in `crates/loom_tool_registry/src/lib.rs` keeps the head of the text up to
`MAX_BORROWED_ERROR_TEXT_BYTES` (2 KiB) and states the number of bytes it dropped, so a truncated
error is not mistaken for a short one. Only the head is kept because a diagnostic states its problem
first and pads afterwards, and the cut lands on a character boundary — this text is regularly not
ASCII and a blind slice would panic. A single helper was chosen over per-site truncation because drift
on a length bound is harmless while duplicated slicing logic is a panic waiting to happen.

Five call sites now pass through it: `CloudHttpStatus` and `CloudJson` in `lib.rs`; in
`framework_process.rs` the non-zero-exit `detail`, the `invalid JSON response: …; stdout: …` protocol
reason, and the resource-limit `detail`'s stderr echo; and in `framework.rs` the framework self-test's
stderr echo. `CloudJson` now borrows the body rather than moving it, since the sibling arm still
returns that same body as text content.

For S6b2d2-6, `let tool_list = client.list_tools().ok();` discarded the reason a listing failed, and
argument normalization then silently ran without a schema, so string arguments went to a server that
wanted an integer and the server's rejection was the only thing anyone ever saw. Failing fast on the
listing was rejected: a server that cannot list its tools may still be able to run them, and making
the listing mandatory would stop those servers working at all. Instead the failure is remembered and,
if the call then fails, folded into the call error by the new `mcp_call_error` — the two texts are
reported together, both bounded. A call that succeeds says nothing about the listing, and a call that
fails after a normal listing is reported exactly as before, so the daemon's existing mapping
(`ToolRegistryError::Mcp` → 500 `mcp_execution_error`) is unchanged. Nothing outside `loom_mcp`
matches on `McpError` variants, so folding the call error into `McpError::Protocol` in that one case
costs no downstream behaviour.

Verified with `cargo test -p loom_tool_registry` (172 passed, up from 166) and `cargo fmt --all --
--check` (clean). Six new tests: short borrowed text kept whole (including empty and exactly-at-bound
input), long text keeping its head and reporting the dropped byte count, a multi-byte body cut on a
character boundary, a failed call after a successful listing reporting only itself, a failed call
after a failed listing reporting both, and a folded pair of 64 KiB texts staying bounded.

No board entry: `crates/loom_tool_registry` is Lane A reserved.

Still open in these slices after this batch: S6b2d2-5, S6b2d2-7, S6b2d3-6, S6b2d3-8, and the memory
half of S6b2d2-8. That memory half — a 64 MiB image body becoming roughly 85 MiB of base64 string on
top of the original buffer, and the non-image path copying the whole body through
`String::from_utf8_lossy` — is deliberately not fixed here: it needs streaming or a chunked encoder
rather than a length bound, which changes the shape of the cloud response path and of what
`normalize_cloud_response` returns, and that is not local or cheap. `cargo clippy` remains unavailable
for the pinned toolchain.

### F11h — done (F8 batch: the choices that were made quietly)

Closes S6b2d3-8 and S6b2d2-7.

Three places decided something on the caller's behalf and said nothing about it.

An image-candidate index past the end of the list was clamped, so asking for the eighth of three
returned the third; and when the chosen candidate could not be downloaded, another one was delivered
under the requested index's name. `selected_mcp_image_candidate_index` now returns an
`McpImageCandidateSelection` that keeps the requested index exactly as asked alongside the index it can
start from, and `attach_mcp_image_candidate_metadata` takes both that selection and the index actually
delivered. `selectedIndex` still names the candidate on screen — the daemon's
`node_selected_result_index` reads it to know which item is displayed, so that meaning is unchanged —
and when the delivered candidate is not the one requested, `requestedIndex` and a `selectionNote`
appear beside it. The note names each cause it found: an index past the end of the list, a candidate
that would not download, or both. Nothing fails over either — an image still arrived — and the
addition is purely additive, so a consumer that ignores the two new keys behaves exactly as before.

In the cloud multipart form, a field whose rendered value still contained `{{` was dropped from the
request, so an unresolved binding turned into the API complaining about a parameter Loom never sent.
The unresolved check now reads the *template's* own `{{…}}` tokens through the new
`unresolved_cloud_template_placeholder` and errors with the field name and the placeholder, rather than
searching the rendered text for braces — braces are legitimate content in an argument value (a caption,
a code snippet) and such a value used to make its field vanish. `__DISABLED__` is still honoured in
silence, because that is the author's own way of saying "leave this field out", and an empty rendered
value is still skipped, because that is an optional argument left blank.

Finally, only POST, PUT, and PATCH ever sent the declared body, so a `body` on a GET or DELETE Art was
dropped on the way out. It is now refused with an error that names the method and points at the query
string. Rejecting at execution rather than in `validate` was deliberate: validation also runs over
already-stored tools, and no Art in the tree declares a body on a method that cannot carry one, so
nothing that works today starts failing.

Verified with `cargo test -p loom_tool_registry` (178 passed, up from 175 after F11g's 172 plus this
batch's own additions) and `cargo fmt --all -- --check` (clean). Six new tests: an in-range request
reported with no note, an out-of-range request reporting the clamp it used, the note for a download
fallback and for both causes at once and for neither, only a template's own placeholders counting as
unresolved (including an unterminated `{{` and braces arriving in a value), a multipart field with an
unfilled binding reported instead of dropped, and a body on GET rejected. `McpImageCandidate` gained
`Default` so a test can name only the field it cares about.

No board entry: `crates/loom_tool_registry` is Lane A reserved.

Still open in these slices after this batch: S6b2d2-5, S6b2d3-6, and the memory half of S6b2d2-8.
`cargo clippy` remains unavailable for the pinned toolchain.

### F11i — done (F8 batch: cancelling an MCP run)

S6b2d2-5, MCP half. `execute_tool_with_timeout_and_cancellation` accepted a cancellation flag and then
threw it away for two of the three execution kinds. On the MCP path a cancelled canvas run kept talking
to the server until the request timeout expired, and the result of a run the caller had already given up
on was still handed back as a success.

A step already in flight cannot be interrupted from inside this arm. Both transports are synchronous:
the stdio client blocks in `recv_timeout` on its stdout channel and the streamable-HTTP client blocks in
a `reqwest` blocking call, and neither hands out a handle another thread could use to break the wait.
`McpClient::cancel()` exists but reaching it would need the client to be shared with a watcher thread
while the blocking call holds it exclusively. So cancellation is observed at the seams between steps
instead, which is where this arm can act on it:

- before the server is spawned, so a cancelled run starts nothing at all;
- after connecting, since a spawn is often the slowest part of the conversation;
- before the call, covering both setup round trips;
- after the call returns, because a cancellation that arrives mid-call is not visible any earlier.

The last one changes what a cancelled-but-completed run reports: the result is dropped and the
cancellation is raised instead. `loom_workflow_runtime` already does exactly this after each of its own
steps (`crates/loom_workflow_runtime/src/lib.rs:187`), but the surface action runner does not look at
the flag itself, so on that path the result of an abandoned run used to be persisted as an ack.

Each early return drops the client, and `loom_process::ManagedChild` terminates its child on the way
out, so a stdio server is not left running behind a cancelled run. `McpClient::cancel()` is therefore
still not called from here — calling it would repeat what `Drop` already does — and it stays unused.

The signal itself needed a name. `ToolRegistryError::ExecutionCancelled { id }` is new, sitting next to
`ExecutionRejected` since both describe a run that did not happen. The daemon maps it to HTTP 409 with
code `cancelled`: a cancelled run is not a server fault, and `cancelled` is the code the Hook bridge
already reports for a cancelled Art run, so a client recognises the two alike. That mapping is the only
exhaustive match on `ToolRegistryError` in the tree (`loom_workflow_runtime` wraps it with `#[from]`),
so nothing else needed a new arm.

Files: `crates/loom_tool_registry/src/lib.rs`, `apps/daemon/src/lib.rs`.
Tests: `a_cancelled_mcp_run_stops_before_the_server_is_started` (a server configured with a command
that cannot be spawned still reports the cancellation, which is only true if the run stopped before the
connect step), `an_uncancelled_mcp_run_still_reaches_the_server`.
Verified: `cargo test -p loom_tool_registry` — 180 passed; `cargo check -p loom-daemon` — clean;
`cargo fmt --all -- --check` — clean.

Board entry: none for `crates/loom_tool_registry` (Lane A reserved). `apps/daemon/src/lib.rs` is
unreserved and not covered by H4, which applies to `scripts/**`.

Still open in these slices after this batch: the cloud half of S6b2d2-5 (queued for F11j), S6b2d3-6,
and the memory half of S6b2d2-8. Interrupting an MCP step already in flight is recorded as backlog with
the reason above; it needs an abortable transport, which is the same dependency S6b2c3-6's HTTP half
already carries (cross-referenced at line 3107).

### F11j — done (F8 batch: cancelling a cloud run, and handing the flag over at all)

S6b2d2-5, cloud half, plus the caller-side gap that made the MCP half unreachable on one path.

`execute_cloud_api_tool` now takes the cancellation flag and reads it in three places: before anything is
rendered, so a cancelled run sends no request at all; once more immediately before the request goes out,
since template rendering sits between; and between chunks while the response body is read. That last one
is why the body is no longer read with a single `read_to_end`: the cap allows 64 MiB, and a download that
large is worth being able to stop part way rather than finishing it for a caller that has already given
up. The chunk loop keeps `read_to_end`'s treatment of `ErrorKind::Interrupted` — a read cut short by a
signal is retried, not reported.

The request itself still cannot be aborted. The blocking `reqwest` client hands out no handle another
thread could use to break off a call in flight, so a hung connection remains bounded only by the timeout.
That is stated in the comment at the send site rather than left implicit, and it is the same missing
dependency S7b2-8 records for the streamable-HTTP MCP transport.

Threading the flag into the registry was not enough on its own: the surface action runner passed it only
for framework Arts and called `execute_tool_with_workflows_timeout` for everything else, so an MCP or
cloud action never received a flag to observe. `loom_workflow_runtime` only offered cancellation together
with a preview stream, so the runner would have had to pass a callback that throws away everything it is
handed. `execute_tool_with_workflows_timeout_and_cancellation` is the missing plain variant, and the
runner now uses it. A cancelled non-framework action therefore stops instead of running to its timeout
and having its result recorded as an ack.

Left alone deliberately: `ToolRegistryError::ExecutionCancelled` is not remapped to
`WorkflowRuntimeError::Cancelled` on the way out. `WorkflowRuntimeError` takes registry errors through
`#[from]`, so intercepting would mean touching every `?` site that converts one, and the daemon already
labels the failure `cancelled` from the flag itself (`apps/daemon/src/lib.rs:16202-16206`), which is set
in every real cancellation. Not local, and nothing observable would change.

Files: `crates/loom_tool_registry/src/lib.rs`, `crates/loom_workflow_runtime/src/lib.rs`,
`apps/daemon/src/surface_actions.rs`.
Tests: `a_cancelled_cloud_run_sends_no_request` (asserts the fixture captured no request, so the run
stopped before the send and not after it). The chunked body read is covered by the existing cloud tests,
which all pass through it.
Verified: `cargo test -p loom_tool_registry` — 181 passed; `cargo test -p loom_workflow_runtime` — 16
passed; `cargo test -p loom-daemon surface_action` — 13 passed; `cargo check -p loom-daemon -p
loom_workflow_runtime` — clean; `cargo fmt --all -- --check` — clean.

Board entry: none for the two `crates/**` files (Lane A reserved). `apps/daemon/src/surface_actions.rs`
is unreserved and not covered by H4, which applies to `scripts/**`.

Still open in these slices after this batch: S6b2d3-6 and the memory half of S6b2d2-8. S6b2d2-5 is closed
apart from interrupting a request already in flight, which stays recorded backlog for both transports
with the reason above.

### F11k — done (F8 batch: keeping the URL a rewrite was derived from)

S6b2d3-6 is closed. An image candidate now carries both forms of its URL and the download tries both.

`normalize_image_candidate_url` used to return a bare `String`, so when it rewrote a URL — dropping a
CDN modifier off the end — the rewrite was all the candidate kept. That was fine for the convention the
rewrite exists for, `.../a.jpg!600x400`, and wrong whenever the cut landed inside a real path: the
heuristic cuts at the last image extension followed by `!`, `/`, or end of path, so
`https://host/logo.png/v2/actual` became `https://host/logo.png`, an address for a different image, and
the one the server actually sent was gone. The download path already retried the rewritten form of
whatever URL it was handed (`image_response_from_mcp_candidate_url`), but by then there was nothing left
to derive the original from.

The function now returns a small `CandidateUrl` holding the URL to request plus, when the URL is a
rewrite, the string it came from. `McpImageCandidate` gained an `alternate_image_url` field for that
string, and `image_response_from_mcp_candidate` walks the rewritten URL, then the original, then the
thumbnail, skipping duplicates so a candidate that repeats itself does not spend the download budget
twice on one address. The rewritten form stays first because a CDN modifier is the usual reason a URL
needs rewriting at all, and it also keeps the reported `imageUrl` in the candidate metadata unchanged
for the cases the existing tests pin.

One related detail changed with it. `source_page_url` falls back to the object's `url` field, excluding
the field when it is the image URL itself. It compared against the rewritten form only, so the original
decorated string — the very string `url` usually holds — was accepted as the page the image sits on and
became the `Referer` for its own download. Both forms are excluded now.

Thumbnails take the rewritten form and drop the original: `first_imageish_string` still returns a
`String`. A thumbnail is already the fallback after the image URL, and giving it a second form would
put a fourth attempt on the download budget for a case nothing has been observed to need.

Tests: `a_rewritten_candidate_url_keeps_the_string_it_came_from` pins both forms for the CDN convention
and for the nested path, and asserts the original is not mistaken for a source page.
`normalize_mcp_image_search_falls_back_to_the_unstripped_candidate_url` serves the image at
`/logo.png/v2/actual` and nothing at `/logo.png`, so only a run that retries the server's own string
returns the image. That test needed a fixture that answers more than once, so
`RetryingExactPathHttpImageFixture` was added beside the single-connection one: it serves until it is
dropped, which means a regression fails the assertion instead of hanging the suite waiting for a second
connection that never comes.

Verification: `cargo test -p loom_tool_registry` — 183 passed, 0 failed; `cargo check -p loom-daemon` —
clean; `cargo fmt --all -- --check` — clean.

Board entry: none. `crates/loom_tool_registry/src/lib.rs` is Lane A reserved.

Still open in these slices after this batch: the memory half of S6b2d2-8, and interrupting a request
already in flight (S6b2d2-5), both recorded backlog with their reasons above.

### F11l — done (F8 batch: the naming half of S7a-4)

S7a-4 is now closed. The version directory an MCP server package is installed into was named
`versions/<version>-<digest[..12]>`: 48 bits, which about 2^24 hashes is enough to collide once the
attacker also controls the version string. Two archives landing on one directory means one of them runs
the other's files, so the name is a security boundary and 48 bits is not a boundary. It is now
`digest[..32]` — 128 bits — behind a named constant, `PACKAGE_DIRECTORY_DIGEST_CHARS`, so the reason the
width is what it is sits with the number.

Not the full digest, deliberately, and the reason is in the constant's doc comment: an MCP server that
vendors its dependencies nests deeply inside this directory, every character in the directory name comes
out of the `MAX_PATH` budget those files need, and 128 bits already puts a collision out of reach. The
whole digest is recorded in `active.json` and in the server config regardless, so nothing that needs the
full value reads it off the path.

F8u closed the other half of this finding by verifying an existing directory against the digests just
computed, which already turned the collision from "the benign package silently executes the malicious
files" into an `Integrity` failure. This half removes the collision itself rather than relying on that
check to catch it.

Packages installed under the old 12-character name keep working and need no migration: nothing parses
the digest back out of the directory name, and `verify_installed_entry`'s only structural check on the
path is that its parent is the package's `versions` directory. A reinstall lands in a directory under
the new name, and the old one is left where it is — the installer has never pruned superseded versions.

Test: `the_version_directory_is_named_after_enough_of_the_digest_to_be_unique` installs a package and
pins the directory name against the recorded digest, the width against the 32-character floor, and the
recorded digest itself at its full 64 characters, so a later shortening of the name cannot quietly
shorten what is stored.

Verification: `cargo test -p loom_mcp` — 36 passed, 0 failed; `cargo check -p loom-daemon -p
loom_tool_registry` — clean; `cargo fmt --all -- --check` — clean.

Board entry: none. `crates/loom_mcp/src/package.rs` is Lane A reserved.

Still open in these slices after this batch: S7a-5 and S7a-6, both P3 and both in the file this batch
opened, so they are queued for F11m rather than left as backlog; within F8's remaining queue, S7b1-2 and
S7b1-4 plus the two backlog items named above.

### F11m — done (F8 batch: the two remaining P3s in the package installer)

Both slices live in `crates/loom_mcp/src/package.rs`, the file F11l had just opened, which is why they
were taken as a batch rather than filed as backlog.

S7a-5, the durability half. `write_active_state` wrote its payload with `fs::write` to a constant
`active.json.tmp` and then renamed it. Two problems in one line. The constant name is shared by every
concurrent install of the same package — a retry racing its own first attempt is enough to have two —
and then both writers own the same temp path and whichever rename lands second publishes whatever mix
of the two payloads happens to be on disk. And `MOVEFILE_WRITE_THROUGH` on the rename flushes the
rename, not the bytes of the file being renamed, so a crash between write and rename could leave an
`active.json` that exists and is empty. Empty reads back as a package with no recorded digests, which
is the one state a spawn refuses outright, so the failure mode was a package that installs and then
never starts. The rewrite follows the pattern `write_tools` and the zip extractor already use: a
per-install temporary name built from `staging_name()`, `write_all` followed by `sync_all`, then
`replace_file`, with the temporary removed on either failure path so a failed install does not leave
one behind.

S7a-6, the entry cap. `MAX_PACKAGE_FILES` was 128 against the shared extractor's 4096. No real MCP
server fits 128: an npm or Python server vendors its dependencies, and a dependency tree is thousands
of files before it is anything else. The cap that actually guards extraction was never this one —
`extract_zip_securely` enforces its own entry count, per-entry and total size limits, and a
compression-ratio check — so 128 only turned normal packages away, and did it at install time with a
message about entry counts rather than anything a publisher could act on. Raised to 4096 to match the
extractor, with the reasoning recorded at the constant.

Tests. `writing_active_state_leaves_no_temporary_file_behind` installs and then reinstalls the stdio
fixture and asserts nothing matching `*.tmp` survives in the package root, and that `active.json` still
parses with a 64-character digest — the per-install name means the rename is now the only thing that
removes the temporary, so a regression that skips the cleanup path shows up as a leftover file.
`installs_a_package_that_vendors_its_dependencies` builds a 302-entry archive shaped like a vendored
`node_modules` tree and asserts the install records every file, which fails against the old 128 cap.

Verification: `cargo test -p loom_mcp` — 38 passed, 0 failed; `cargo fmt --all -- --check` — clean;
`cargo check -p loom-daemon` — clean.

Board entry: none. `crates/loom_mcp/src/package.rs` is Lane A reserved.

Still open in this slice group: S7a-7 (P3, unbounded free-text and list fields in a manifest) is in the
file this batch touched but is not local or cheap — it needs a bound chosen for each of `name`,
`description`, credential `label`, `tools`, and `entry.args`, plus identifier validation on `tools`,
which is a validation-surface change rather than a fix to one line, so it stays recorded backlog.
Within F8's remaining queue: S7b1-2 and S7b1-4, plus the previously recorded backlog items (the memory
half of S6b2d2-8 and interrupting an in-flight request, S6b2d2-5).

### F11n — done (F8 batch: the last two P3s in F8's queue, both about which file a stdio server runs)

S7b1-2, a relative command. `McpServerConfig::validate` required only that a stdio `command` be
non-blank, so `runtime/server.exe` or `./server` was accepted and then completed by the daemon's current
directory — on Windows through `resolve_windows_command_path`'s empty search-path branch, on other
platforms by `Command::new` itself. The same configuration therefore starts different files depending on
where the daemon was launched from. `validate_stdio_command` now refuses a path that has a parent but no
root, and says why in the message. Two forms stay valid: an absolute path, which says exactly what it
means and is what the installer records for a packaged server; and a bare program name, which is a
`PATH` lookup and is how servers are normally launched (`npx`, `node`, `python`) — refusing that would
rule out most of the ecosystem for no gain, since a bare name was never ambiguous in the way a relative
path is.

S7b1-4, batch entry points. `std::process::Command` does not run a `.bat` or `.cmd` file directly; it
hands it to `cmd.exe`. The only thing between manifest-supplied `args` and a shell command line is the
standard library's batch-argument escaping — one mitigation, and one that has had CVEs of its own. The
slice suggested refusing batch files outright, and that is what this batch did *not* do, deliberately:
on Windows `npx`, `npm`, and most npm-installed CLI shims **are** `.cmd` files, so an outright refusal
would break the majority of real MCP server configurations while protecting nothing the operator did not
type themselves. The refusal is scoped to packaged servers, which is where the slice's own reasoning
applies — a package names its own entry point, so it can ship an executable or a `.ps1` instead. Both
ends are covered: `validate_manifest` rejects a batch `entry.command` at install, where the publisher can
still act on the message, and `verify_installed_entry` rejects one at spawn, because `servers.json`
supplies the command and the row can be edited after install (the S6b2c3-9 chain) to point at a batch
file that does sit inside the package directory.

Tests. `a_relative_stdio_command_is_refused` (`crates/loom_mcp/src/lib.rs`) pins both halves of the
rule: `runtime/server.exe` and `./server` are refused, a bare `npx` and an absolute path validate.
`refuses_a_package_whose_entry_is_a_batch_file` installs a package whose entry is `runtime/server.cmd`
and asserts the install fails with an `InvalidManifest` naming the batch file, and that no package
directory was left behind. `refuses_to_spawn_a_packaged_server_pointed_at_a_batch_file` installs the
normal fixture, writes a `.cmd` *inside* the package directory, repoints `config.command` at it, and
asserts `verify_installed_entry` refuses it — which is the case the install-time check cannot see.

Verification: `cargo test -p loom_mcp` — 41 passed, 0 failed; `cargo test -p loom-daemon` — 238 + 8
passed; `cargo test -p loom_tool_registry` — 183 passed; `cargo fmt --all -- --check` — clean.

Board entry: none. `crates/loom_mcp/src/lib.rs` and `crates/loom_mcp/src/package.rs` are Lane A
reserved.

Still open in this slice group, all P3 and all recorded backlog rather than fixed: S7b1-3 (protocol
revisions hardcoded per transport and never negotiated — a handshake change, not a local fix, and it
touches the same negotiation surface as S6b2d1-7), S7b1-5 (`.ps1` force-added to `PATHEXT` and script
args appended where the parameter binder sees them), S7b1-6 (user-added servers validated more weakly
than packaged ones), S7b1-7 (`env` has no denylist for process-influencing variables), S7b1-8 (a dead
error variant and an infallible function typed as fallible). S7b1-1 was closed earlier by F8v.

With this batch F8's queue is empty apart from the two items already recorded as not local or cheap: the
memory half of S6b2d2-8 (streaming base64 for a 64 MiB body) and interrupting a request already in
flight (S6b2d2-5).

### F11o — done (S9 batch: S6b2c3-3, an Art's image output stops being decoded and re-encoded)

Reading the slice against the code found its second half stale, and the first half worse than recorded.

The MIME half. The slice says the hardcoded `"mimeType": "image/png"` disagrees with the data URL, which
carries the real type. It does not: `read_image_path_as_data_url` decodes the file and re-encodes it as
PNG, so the data URL was always `data:image/png` and the label always matched. What the pairing actually
meant is that no other format could survive the trip — and `loom_image_io` builds `image` with
`default-features = false, features = ["png"]`, so a JPEG or WebP output was not mislabelled, it was
refused outright with `cannot decode image output`. Nothing recorded that an Art may only emit PNG.

The memory half. A file-size check with a decode behind it does not bound memory at all, and the reason
is stronger than the slice's arithmetic: base64 at 1.37× is linear in the number that was checked, but
the decoded surface in between is width × height × 4 and has no relation to the compressed size. A
modestly sized, highly compressible PNG can decode to gigabytes. That intermediate was the unbounded
part, and it existed only to produce a PNG the caller already had.

Both follow from the re-encode, so the fix removes it. `read_image_path_as_web_data_url` in
`crates/loom_image_io/src/lib.rs` reads the file, identifies the container from its magic bytes with
`image::guess_format`, and for a format a browser renders natively — PNG, JPEG, GIF, WebP, BMP, ICO,
AVIF — emits those bytes with that format's own MIME type. No decoder is involved, so peak memory is the
file plus its base64, both bounded by the caller's size limit, and a JPEG output now works instead of
failing. TIFF and the other containers `image` understands but browsers do not still go through the old
decode-and-re-encode path, because handing those bytes to a viewer would show nothing; the returned
`ImageDataUrl` carries `re_encoded` so a caller can tell which happened. The pass-through validates the
container and not the pixels, which is the trade: a full decode is the unbounded allocation being
avoided, and every consumer decodes the image itself anyway.

`normalize_framework_image_output` now labels the content block from `image.mime_type` instead of a
constant, and `MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES` gained a doc comment recording that it only became a
memory bound once the decode was gone. Its value is unchanged: 256 MiB of file is still close to 600 MiB
of peak once the base64 and `ConvertTo-Json`'s copy are counted, and lowering it would reject outputs
that work today, so it is left as a limit to revisit deliberately rather than tightened in passing.

The two other callers of `read_image_path_as_data_url` — the daemon's canvas reader and the workflow
runtime — are unchanged. They belong to their own slices, and the PNG re-encode is not wrong there, only
more expensive than it needs to be.

Test: `a_web_renderable_image_is_passed_through_with_its_own_mime_type` asserts a PNG comes back as
`image/png` with `re_encoded` false and a payload equal to the file's bytes byte for byte — which is what
proves no re-encode happened — and that a JPEG stub is identified from its magic bytes as `image/jpeg`, a
case the PNG-only build could not have read at all.

Verification: `cargo test -p loom_image_io` — 8 passed, 0 failed; `cargo test -p loom_tool_registry` —
183 passed; `cargo test -p loom-daemon` — 238 + 8 passed; `cargo test -p loom_workflow_runtime` — 16
passed; `cargo fmt --all -- --check` — clean.

Board entry: none. `crates/loom_image_io/src/lib.rs` and `crates/loom_tool_registry/src/framework_process.rs`
are Lane A reserved.

Remaining in the S9 queue: S8b1-7, S8a2b-12, S8b2-9.

### F11p — done (S9 batch: S8b1-7, an image-search candidate carries its payload once)

Two earlier batches had already taken the largest pieces of this slice: `output_base64` is gone from the
response, and `thumbnail` holds a downscaled preview instead of a second copy of the full image. What was
left is the duplication the slice named first, and it turned out to cost more than the runtime alone.

Each candidate stored its data URL under `data`. The host's `normalize_image_candidate_item` then copies
whichever key it recognizes — `imageUrl`, `image_url`, `url`, `src`, `data`, … — into `imageUrl`, because
that is the key both candidate consumers read: Hook's canvas result strip and its `artDeliveryCandidates`
both key on it and drop items without it. So every candidate's base64 was held twice in the host's value
and serialized twice on the way out, and no reader ever looked at `data`. At six candidates and the 32 MiB
per-image cap that is roughly a quarter of a gigabyte of duplicate string in one response.

The runtime now writes the payload under `imageUrl` and nowhere else. This is not a wire change: the value
the host would have inserted is byte for byte the value now supplied, so the normalizer finds the key
already present and copies nothing.

The thumbnail had a second, quieter path to the same problem. `New-ImageThumbnailDataUrl` returns its
input unchanged when the image is already under the 320 px edge or cannot be decoded — sensible for a
helper whose downscale is an optimization, but emitting that result as `thumbnail` makes a third copy of
bytes already present. The runtime now sets `thumbnail` only when the helper produced something different.
`HookCanvasView.tsx:138` renders `candidate.thumbnailUrl || candidate.imageUrl`, so a candidate without a
thumbnail still paints, and the small-image case it drops is exactly the case where a separate thumbnail
would have shown nothing extra.

The selected image still appears twice, once as its candidate's `imageUrl` and once in `content[0].data`,
and that pair is not removable: the grid needs the candidate and the Art's declared output port needs the
content block. The existing comment at that site records it.

Verification: `scripts/tests/Test-LoomSampleArtRuntime.ps1` — 12 execution and rejection cases passed,
including `custom-image-search` with two candidates, the `result_index=1` selection case, the loopback
rejection case, and the loopback-fixture download case; `Parser::ParseFile` on the runtime script — clean.

Board entry: none. `art-packages/samples/image-search/**` is Lane A reserved, and no `scripts/**` file was
edited (H4 does not apply to running them).

Not fixed in this file, still recorded: S8b1-8 (`result_index` is silently coerced, so an out-of-range
index quietly returns the last candidate — reachable but neither local nor cheap, since it needs a
diagnostic channel the Art response shape does not have) and S8b1-9 (`url` is overloaded three ways in
`Convert-ToMcpImageCandidate`, correct only because of the order the rules run in — a rewrite of that
function rather than an edit inside one).

S8a2b-12 leaves this queue: its scope is `mcp-server-packages/stock-api/**`, which is Lane B's, so the
whole-body-buffering half is that lane's to fix. Remaining in the S9 queue for Lane A: S8b2-9.

### F11q — done (S9 batch: S8b2-9, the output builders stop decoding an image to read its size)

`New-ImageOutput` and `New-ImagePathOutput` each constructed a `System.Drawing.Bitmap` from a path in
order to read two integers off it. That constructor decodes the file: the surface it allocates is
width × height × 4 bytes and bears no relation to the compressed size, so a 4000×3000 PNG cost 48 MiB to
answer `width` and `height`. It also holds an exclusive lock on the file for the object's lifetime, which
is why `New-ImageOutput` was reading its own output twice — the `Bitmap` kept the file open while
`Convert-ImagePathToDataUrl` opened it again to base64-encode it.

`Get-ImageDimensions` replaces both uses. It opens the file with `FileShare.Read`, calls
`[System.Drawing.Image]::FromStream($stream, $false, $false)` — no embedded colour management, no image
validation — reads `Width`/`Height` from the header, and disposes both the image and the stream before
returning. No pixel data is touched and no lock outlives the call, so the remaining read is the one the
data URL needs.

Skipping validation is a real behaviour change: a truncated or corrupt file is no longer rejected at this
point. It is the right trade for these two callers, because every one of them has just written the file
itself with GDI+, so decoding it here re-validates the runtime's own output; a caller that passes a file it
did not produce has already been through `Resolve-ImagePath`.

The slice's second recommendation — stream the base64 instead of materializing the byte array — was
measured and declined. `ToBase64Transform` has `InputBlockSize` 3 and does not transform multiple blocks,
so a `CryptoStream` over it runs one managed transform call per three bytes, which is the same
per-element-interop shape `Blend-Bitmaps` was rewritten to escape. Chunking `[Convert]::ToBase64String`
into a `StringBuilder` avoids that, but `ToString` then copies the whole result: peak becomes two copies of
the base64 (about 2.74× the file) where `ReadAllBytes` plus `ToBase64String` is the file plus one base64
(about 2.37×). The streaming version is slower and no smaller at peak, so `Convert-ImagePathToDataUrl` is
unchanged and this stays recorded rather than "fixed" in the wrong direction. A genuine fix needs the
base64 written straight to stdout instead of into a JSON string, which is a response-writer change
(S8b2-10) and not a change to this helper.

Verification: `scripts/tests/Test-LoomSampleArtRuntime.ps1` — 12 execution and rejection cases passed;
`scripts/tests/Test-LoomSampleArtPackageContract.ps1` — 7 packages passed; a direct probe of the new helper
on a 37×19 PNG returned `37x19` through `Get-ImageDimensions`, `New-ImageOutput`, and `New-ImagePathOutput`,
and the file deleted immediately afterwards, which is what shows the lock is gone; `Parser::ParseFile` on
the shared helper — clean.

Board entry: none. `art-packages/shared/**` is Lane A reserved, and no `scripts/**` file was edited.

The S9 queue is now empty for Lane A.

### F11r — done (Lane B's two hand-backs: the poison cascade and the shutdown-race sleep)

Both items came from Lane B's entries on `docs/progress/phase-78-lane-sync.md`, and both sit in
`apps/daemon/src/lib.rs`. They are recorded here because they are Lane A's lines and because they decide
whether F10's full-suite run is usable as evidence.

**The poison cascade.** The test module serializes tests that mutate process-wide state behind two
`Mutex<()>` statics, `ENV_LOCK` and `HOOK_ART_REQUEST_TEST_LOCK`, and every one of the 47 call sites took
them with `.expect(...)`. Because a `std::sync::Mutex` is poisoned when a thread panics while holding it,
one panicking test made every later test that wanted the same lock fail on the poison rather than on
anything of its own: Lane B observed a single flake reporting as 38 or 39 simultaneous failures, which
buries the real fault and makes the run worthless as a signal.

Poisoning is the wrong contract for these two locks in particular. They guard `()`. There is no shared
invariant a panicking test could have left half-written, because the state each test depends on —
environment variables, the Hook canvas runtime — is state that test sets up for itself. A new
`lock_ignoring_poison` helper takes either lock with
`unwrap_or_else(|poisoned| poisoned.into_inner())`, and all 37 `ENV_LOCK` sites plus all 10
`HOOK_ART_REQUEST_TEST_LOCK` sites now go through it. The failure count is once again equal to the fault
count.

**The shutdown-race sleep.** `daemon_returns_shutting_down_for_request_accepted_before_shutdown` writes
request headers without a body, starts the serve loop, and then wanted shutdown to arrive while that
connection was accepted but unanswered. It expressed the wait as `thread::sleep(100ms)`. That is a guess
in both directions: when the accept is faster the test wastes the remainder, and when the machine is
loaded and the accept is slower the shutdown lands first, so the run exercises the backlog drain instead
of the path the test is named for and the mismatch surfaces as a timeout inside
`read_to_string(...).expect("read shutdown response")` — a panic, which is also how the poison above got
started.

The daemon already had the shape for an answer: three `#[cfg(test)]` observer slots on `DaemonRuntime`,
installed through `Arc::get_mut(&mut daemon.runtime)`, with `record_shutdown_observed` and
`record_request_submission` as the compile-out wrappers. A fourth, `ConnectionAcceptObserver`, counts
connections the serve loop has accepted and successfully handed to a read worker, recorded from the
`Ok(())` arm of `read_stage.try_submit`, which is the point at which the connection is genuinely in the
pipeline rather than merely off the backlog. The test now waits on
`accepted.wait_for_count(1, Duration::from_secs(3))` and asserts the wait succeeded, so it either
establishes the ordering it needs or fails saying that is what it could not establish.

Verification: `cargo fmt -p loom-daemon -- --check` — clean; `cargo check -p loom-daemon --locked
--all-targets` — clean; `cargo test -p loom-daemon --locked` — 238 passed, 0 failed, plus the 8-test
binary; the shutdown-race test run five more times on its own, 1 passed each time. `cargo clippy` remains
unavailable for the pinned 1.95.0 toolchain, so `cargo fmt` is the lint evidence.

Board entry: none — the board is Lane B's file. Lane B's two action items are answered by this record.

Lane A's queue is empty apart from the H11 backlog, which stays blocked until `loom_security` is
committed, and F10.

### F11s — done (H14(7): the workspace change set is committed as one buildable unit)

H14(7) required `apps/daemon/src/lib.rs` to be committed together with `crates/loom_protocol/**` and the
lockfile, because the daemon references `loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION`, which was not in
`HEAD`. Tracing the rest of the dependency made the required set larger than those three paths and fixed
its boundary exactly:

- `crates/loom_protocol/src/lib.rs` pulls `protocol/schemas/surface-stream.v1.schema.json` in with
  `include_str!`, so that file is a compile dependency and not documentation.
- The workspace manifest lists `crates/loom_security` and `crates/loom_perf` as members, and both were
  untracked. Committing the manifest without them leaves no buildable workspace at all.
- `apps/daemon/Cargo.toml` depends on `loom_perf`, and `apps/daemon/src/surface_actions.rs` calls
  `loom_perf::assert_within`.
- `crates/loom_tool_registry` deletes its own `network_policy.rs` and `secure_zip.rs` in favour of the
  shared crate; git recorded both as renames into `loom_security`, so the move has to travel with it.

Commit `41123d2` therefore covers the Rust workspace and nothing else: the root manifest and lockfile, the
daemon and its surface modules, `apps/plugin-cli/src/lib.rs`, the five changed crates, the two new crates,
and the schema. It was made path-scoped, on `main`, with no `git add -A`; the two new crate directories and
the schema were staged by explicit path first because a pathspec commit does not pick up untracked files.
Staying on `main` is deliberate: Lane B commits there too, and a branch switch in a shared worktree would
move the other lane's files.

Verification of the commit rather than the working tree, which is the point of the obligation: a detached
worktree was created at `41123d2` outside both repositories, and in that worktree —
`cargo check --workspace --all-targets --locked` finished with no errors, `cargo fmt --all -- --check`
exited 0, and `cargo test -p loom-daemon --locked` reported 238 passed, 0 failed, plus the 8-test binary.
The worktree was then removed. So `HEAD` builds and tests on its own, with none of the still-uncommitted
files present.

What remains uncommitted, and why it is separate: the Art sample runtimes and the shared PowerShell
helper, `scripts/**` (unreserved, and the board rule keeps Lane A out of editing it), `.github/workflows/ci.yml`
with its contract test, the docs, `apps/desktop/package.json`, and
`framework-packages/runtime-host/Cargo.lock`. None of them affect whether the workspace compiles. Two
untracked items are not project files at all — `.memsearch/` is a tool cache and `bash.exe.stackdump` is a
crash dump — and both have to be gone or ignored before `build-release.ps1 -RequireCleanSource` will run,
which is F10's problem to settle.

### F11t — done (the non-Rust change set lands: the sample Art runtimes, then CI)

`dd8b040` covers the Art work from F11o/F11p/F11q: `art-packages/shared/image-runtime-common.ps1` and the
five sample runtimes that consume it — `color-transfer`, `image-blend`, `image-compress`, `image-search`,
`remove-bg` — for six files, +911/-160. The `stock-monitor` sample is dirty too and was deliberately left
out: it is Lane B's path. The two `scripts/tests` files that exercise it were left uncommitted for the same
reason.

Verified before committing: `Test-LoomSampleArtRuntime.ps1` reported "Curated Art runtime smoke passed for
12 execution and rejection cases", and `Test-LoomSampleArtPackageContract.ps1` reported "Sample Art package
contract passed for 7 packages".

`f149f96` covers CI. `.github/workflows/ci.yml` gains the runtime-host workspace in the rust-cache key,
`--all-targets` on the Tauri check, a `cargo test` for the Tauri wrapper, and three steps for the detached
runtime-host manifest, which every `--workspace` step above it skips.

It had to carry a second file. The first attempt, `c177263`, committed the workflow alone; checking that
commit in a detached worktree failed, because `HEAD`'s own `scripts/tests/Test-GitHubActionsContract.ps1`
requires the literal string `cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml`,
which the new workflow no longer contains. A commit that fails its own repository's contract test is not a
usable `HEAD`, so the two files were amended together into `f149f96` (+22/-2) and re-verified at that
commit: "Loom GitHub Actions contract passed."

That file is unreserved, and H4 asks Lane A to announce any touch of `scripts/**` on the board before
making it. The board is Lane B's file, so the announcement could not be written; it is recorded here and
was reported to the user instead. The diff is +5/-1 and only restates the workflow's step strings, so it
cannot collide with a Lane B change elsewhere in the file, but ownership of `scripts/**` still has to be
settled with Lane B before F10.

One verification note worth keeping: reading `$?` after piping a PowerShell test script through `tail`
reports `tail`'s status, not the script's, so the failing contract run first printed a zero exit. The
message text is the evidence, not the code.

### F11u — done (the detached runtime-host lockfile, and a test glob that promised more than it ran)

The three runtime-host steps `f149f96` added to CI had never been run locally. Running them at `HEAD`,
`cargo check --locked` failed outright: "cannot update the lock file ... because `--locked` was passed". The
workflow as committed could not have passed.

The cause is that the detached lockfile was stale relative to `crates/loom_mcp`, which the runtime-host
depends on by path. `loom_mcp` at `HEAD` depends on both `loom_plugin_security` and `loom_security`, and the
committed lock had neither; `loom_plugin_security` in turn pulls in the ed25519-dalek signature stack
(`der`, `pkcs8`, `spki`, `signature`, `curve25519-dalek` and its derive, `base64ct`, `const-oid`,
`fiat-crypto`, `rand_core`, `rustc_version`). The first attempt, `db7b474`, committed the working tree's
+11 lines, which added only `loom_security` — the same failure remained, because a partial closure is not a
closure. Regenerating the lock with a plain `cargo check` produced +159/-4 with fourteen new package
entries, and that was amended into `5db0198`.

Verified at that state, in the manifest's own directory: `cargo fmt -- --check` exited 0,
`cargo check --locked --all-targets` finished with no errors, and `cargo test --locked` reported 21 passed,
0 failed. The three new CI steps now pass against the manifest and lock `HEAD` carries.

`82fcad0` fixes `apps/desktop/package.json`. The test script globbed `src/services/*.test.ts`, so a test
file placed in any other directory, or written as `.test.tsx` for a component, would be skipped in silence —
the suite stays green by not running it. The script now globs `src/**/*.test.ts` and `src/**/*.test.tsx`.
Both patterns resolve to the same thirteen files today, so this is about what the script promises rather
than about a test currently being missed; `npm test` after the change reported 142 tests, 0 failures.

The detached worktree under `Temp\loom-ci-check`, used to check `41123d2` and `f149f96`, was removed;
`git worktree list` shows only the primary worktree.

### F11v — done (the docs catch up with code that already shipped)

Three documents described behaviour Loom no longer has.

`docs/analysis/phase-21-cloud-multipart-template-audit.md` still described ArtLoom's cloud templating as
plain text splicing, and still described file fields as recognized from a field name such as `file`,
`image`, or `*_file` — which let a caller pass any absolute path as an ordinary text value and have the host
read and upload that file. Two paragraphs replace both: rendering is destination-aware (endpoint
substitutions are percent-encoded and re-checked against the declared authority and domain policy, header
and JSON-body substitutions go into the parsed document as values, control characters in header names and
values are refused, and a body template that is not valid JSON before substitution keeps the old
splice-then-parse path), and a file field is now recognized only from the author's own path binding with
canonical containment in the Art package, the control plane root, or a Loom-owned staged input directory.

`docs/plugin-permissions.md` gains the reasons behind three defaults: `network.domains` names the hosts the
package itself calls and deliberately does not constrain the image URLs an MCP search result points at,
because those are CDN hosts the package cannot declare; `network.allowLocalhost` defaults to `false` for
cloud Arts, for an MCP tool's image downloads, and for the host's own HTTP client, so anything talking to a
local service has to say so; and `cloudApi.timeoutMs` defaults to 30 s under a 600 s host ceiling. Three
rows join the capability table, including the bounded MCP image-download loop.

`protocol/README.md` names `schemas/surface-stream.v1.schema.json` as the poll response envelope and states
why `protocolVersion` is required rather than optional there: a reader that finds it absent is talking to a
broken producer, not to an older Loom.

After this batch the only dirty paths left are `scripts/**` (six modified files and one untracked fixture,
whose ownership F10 has to settle with Lane B), Lane B's `stock-monitor` sample, and the two non-project
items `.memsearch/` and `bash.exe.stackdump`.

## Verification commands

Hook:

```
cd Hook
npm run typecheck
npm test
cargo fmt --check --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml
npm run test:surface-browser
```

Loom:

```
cd Loom
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo fmt --manifest-path .\apps\desktop\src-tauri\Cargo.toml -- --check
cargo check --locked --all-targets --manifest-path .\apps\desktop\src-tauri\Cargo.toml
cargo test --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml
cargo fmt --manifest-path .\framework-packages\runtime-host\Cargo.toml -- --check
cargo check --locked --all-targets --manifest-path .\framework-packages\runtime-host\Cargo.toml
cargo test --locked --manifest-path .\framework-packages\runtime-host\Cargo.toml
powershell -File .\scripts\tests\Test-GitHubActionsContract.ps1
cd apps\desktop && npm run typecheck && npm test
```

## Release

After confirmed findings are fixed and verification passes:

- Hook EXE: `powershell -File scripts\build-local-hook-exe.ps1 -OutputDir <release/Hook/tag>`;
  portable ZIP: `powershell -File scripts\package-release-zip.ps1 -ExePath <tag/hook.exe>
  -OutputDir <release/Hook/tag> -Tag <tag>`. The previously named
  `scripts\package-hook-release.ps1` does not exist.
- Loom: `powershell -File scripts\build-release.ps1 -VersionId <YYYYMMDD>-<slug>-r<N> -OutputRoot ..\release\Loom`.

`build-release.ps1 -RequireCleanSource` refuses on any dirty or untracked file; clear
leftover scratch paths at the Loom root first.

## F18/F10 independent closeout audit — 2026-08-23

This is a repository-and-artifact audit. It supersedes earlier status prose where that prose
conflicts with the current Git tree or generated packages.

### What the earlier records got wrong

- The lane board still called F8 in progress even though its P2 sweep was closed; its remaining
  enumerated P3s are accepted backlog, not missing F8 delivery.
- It called F9 not started and F14 in progress. F9a/F9b/F9c and F14 are complete and included in
  `41123d2`.
- F15/F16 historical records said the daemon files were intentionally uncommitted. Those files are
  also in `41123d2`, so that statement became false after the commit.
- F18's plan text was not implementation evidence. At the start of this audit, the default loopback
  daemon really did skip authentication and had no request-origin gateway.
- F8s claimed a working image-search seam, but the checked-in `.loom-art-store-data` framework
  archive was stale and did not contain the seam. The real package/install test was also missing
  bearer authentication, a localhost permission declaration, and correct Stock Monitor result-shape
  assertions.
- The Hook release command at the end of this document named a script that does not exist.

### Work completed

- F18 now always resolves an administrator token in `LoomDaemon::bind`: explicit configuration,
  non-empty `LOOM_DAEMON_TOKEN`, the restricted control-plane token file, or a new random token
  persisted atomically with restricted permissions. Any read/write/corruption failure fails daemon
  startup closed. Production capability manifests always advertise bearer auth.
- Only health and the three device-bootstrap routes are public. All other routes require a valid
  administrator bearer, administrator cookie, or applicable device session. Loopback `Host` and
  same-origin `Origin` are validated, `Sec-Fetch-Site` is restricted to `same-origin`/`none`, and
  state-changing requests require JSON content type. `/settings?token=...` exchanges the token once
  for an HttpOnly, SameSite=Strict cookie and sends a 303 to a tokenless URL.
- CLI, desktop Tauri HTTP/binary clients, and release smoke scripts discover the persisted token and
  send it without logging the secret.
- The MCP installer/Art dependency mismatch was fixed by sharing
  `loom_mcp::package::PACKAGE_DIRECTORY_DIGEST_CHARS` instead of validating a hard-coded 12-character
  immutable directory. A regression test installs/resolves the real 32-character shape.
- Windows framework staging renames now retry short `PermissionDenied` races within a bounded window.
- Four framework packages and seven sample Art packages were rebuilt. Real sample install/execution
  now covers authenticated daemon calls, an actual loopback `GET /fixture.png`, permission-policy
  inheritance through the framework process, and the non-duplicated Stock Monitor authoritative
  state contract.
- Hook's otherwise-complete shared HTTP-client change left
  `HookGeneralSettingsContract.test.ts` asserting the removed direct-builder call shape. The test now
  requires `shared_client`/`shared_client_with` at migrated call sites and verifies that the central
  shared-client builder still invokes `apply_to_url`, preserving proxy behavior.

### Verification receipts

- Loom workspace: fmt/check passed; `cargo test --locked --workspace` passed **677 tests in 59
  suites**. An independent daemon rerun passed **241 tests**.
- Loom release `20260822-phase78-closeout-r76`: build completed with a 57-entry checksum manifest,
  main portable ZIP under `packages/`, CLI package, plugin SDK with 11 schemas, four framework packages, seven sample Art
  packages, and two MCP server packages. `verify-release.ps1 -RunSmoke` passed file integrity plus
  standalone, Hook canvas, Hook error preview, framework Art store, plugin boundary, Surface
  prototype, and authored-Art smokes.
- Hook current tree: lint, application/test typechecks, full Vitest, browser Surface smoke, production
  web build, Rust fmt/check, and Rust tests passed. Rust reported **273 passed** across the executed
  suites; the single real-Tea-daemon smoke remains explicitly ignored without a live Tea daemon.
- Hook release `20260823-phase78-closeout-r91`: release build produced `hook.exe` and a six-entry
  portable ZIP. The built EXE reported self-check `status=ok`, version `0.1.7`, and CLI version text
  `hook 0.1.7`.

### F10 status and remaining provenance gap

The executable/artifact half of F10 is complete, but the originally specified clean-source joint
gate is not. Loom and Hook both contain substantial pre-existing modifications and untracked tool or
scratch files from concurrent work. This audit neither deleted them nor silently committed another
agent's work. Loom's manifest therefore truthfully records `gitDirty=true`; Hook's current packaging
scripts record no Git manifest at all. The two candidates are verified dirty-worktree builds, not
signed releases and not evidence that `-RequireCleanSource` passed. A strict F10 pass still requires
owners to make coherent repository commits, remove or ignore only confirmed non-project scratch
paths, then rebuild from clean checkouts.
