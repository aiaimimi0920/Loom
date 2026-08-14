# Phase 70: `loom.hook.v1` closure and legacy retirement

## Status

Complete and superseded by Phase 71's broader canonical-only cleanup. This phase closes the
Loom<->Hook Art-node invocation framework as a single versioned protocol and
removes the previous production compatibility layer. Phase 71 additionally
removes obsolete persisted-data, package-layout, provider/process field, and
Hook app-data compatibility paths that were retained when this record closed.
The exact Hook R12/Loom R21 pair passed the full 600-second native
dual-end acceptance, including device pairing, Surface recovery after restart,
and isolated process/listener cleanup.

## Final ownership boundary

- Loom owns Art package discovery, installation, settings, framework execution,
  cancellation, resources, durable run evidence, preview revisions, and formal
  result revisions.
- Hook owns the canvas, capability-driven node controls, graph input resolution,
  preview display, formal-result display, and user interaction.
- Hook never loads Art package code and has no Art-ID-specific executor.
- Adding an Art or a third-party framework requires a package/manifest, not a
  Loom or Hook source edit.
- The four framework packages are `process`, `cloud_api`, `mcp`, and `workflow`.
  Command, PowerShell/script, and Python Arts all use `process`.

## Standard wire contract

The only Loom<->Hook Art bridge protocol is `loom.hook.v1`. The public typed
Rust contract lives in `crates/loom_protocol/src/hook.rs`; the language-neutral
schema is `protocol/schemas/hook-message.v1.schema.json`; Hook mirrors the
contract in `src/services/protocol.ts` and its native bridge.

Methods:

1. `loom.hook.handshake`
2. `loom.hook.capabilities.list`
3. `loom.hook.subscribe`
4. `loom.hook.workflow.sync`
5. `loom.hook.workflow.node.update`
6. `loom.hook.workflow.instantiate`
7. `loom.hook.art.execute`
8. `loom.hook.art.cancel`
9. `loom.hook.art.resources.release`
10. `loom.hook.settings.get`
11. `loom.hook.enhancements.get`
12. `loom.hook.ocr.execute`
13. `loom.hook.translation.execute`

Events:

1. `loom.hook.workflow.instantiated`
2. `loom.hook.workflow.updated`
3. `loom.hook.capabilities.updated`
4. `loom.hook.art.ack`
5. `loom.hook.art.progress`
6. `loom.hook.art.preview`
7. `loom.hook.art.result`
8. `loom.hook.art.failure`
9. `loom.hook.settings.updated`
10. `loom.hook.cache.control`

Surface wire events:

1. `loom.surface.snapshot`
2. `loom.surface.patch`
3. `loom.surface.generation`
4. `loom.surface.action.ack`
5. `loom.surface.confirmation.request`
6. `loom.surface.action.progress`
7. `loom.surface.preview`
8. `loom.surface.result`
9. `loom.surface.failure`
10. `loom.surface.lifecycle`
11. `loom.surface.dispose`

Surface payloads identify their payload protocol as `loom.surface.v1`; that
value does not replace the namespaced event name. Subscription matching is
exact. Old wire names such as `surface` and `surface/snapshot` are rejected.
Hook maps received `loom.surface.*` pushes to private `surface/...` Tauri events
for its frontend. Those process-internal UI events are not bridge aliases and
cannot be subscribed to over the Loom<->Hook wire.

Art execution carries a stable `requestId`, node ID, generation, optional device
scope, typed input port values, parameters, disabled parameters, and an optional
deadline. Superseding generations cancel older work in the same node/device
scope; explicit `art.cancel` is also device/node/generation bound. Preview and
formal revisions advance independently, and stale or cancelled work cannot
replace either current value.

Every execution declares `outputTransports`. Native Hook requests shared memory
plus websocket delivery; browser Hook requests websocket only. A shared-memory
release has its own fresh command `requestId` and explicitly names the original
`executionRequestId`. Loom binds every produced handle to the exact
device/execution/node/generation identity, rejects mixed or cross-owner handle
sets atomically, and automatically reclaims live handles on cancellation,
generation replacement, bridge reset, and terminal-cache eviction. Repeated
release of an already released owned handle remains idempotent.

Formal values are one of `value`, `inline_resource`, `shared_memory`, or
`resource`. Inline payloads use bare Base64 in `dataBase64`; data-URL prefixes
are rejected on the wire. Image results prefer shared memory and fall back to a
typed inline resource when shared memory cannot be created. Candidate lists use
the generic `candidates` metadata contract with `kind = image.candidates`; the
old image-search-specific delivery alias is not accepted.

## Legacy compatibility layer removed

The following production paths were deleted or replaced rather than retained as
aliases:

- `/v1/artloom-compat/*` and `/v1/python-arts/*` routes;
- old `art_loom/*`, `art_hook/*`, `art/process`, AHRP, and unnamespaced bridge
  dispatch branches;
- Hook `update_node_param`, mock ArtLoom bridge/startup names, old environment
  variables, and Hook-local pixelate/blur/checkerboard execution;
- `artDefaults`, `loomMetadata.imageSearch`, `delivery.imageSearch`, legacy Art
  IDs/qualified-ID aliases, and the old transport spelling `socket`;
- old `surface`, `surface/snapshot`, and other unnamespaced Surface subscription
  names; production Surface pushes use only the exact `loom.surface.*` set;
- the ArtLoom YAML converter, conversion tests, and ArtLoom example fixtures;
- stale generated desktop assets containing removed routes.

`PUT /v1/tools/{toolId}/defaults` remains a Loom tool-management API. It is not
an old Loom<->Hook wire alias and is not used by the Hook Art bridge. The
WGC-to-GDI runtime capture fallback remains a current reliability mechanism; it
is not an old Art protocol. The filesystem, package-layout, app-data, and
provider/process field compatibility paths described by the original Phase 70
non-goals are removed by Phase 71.

## Capability and graph rules

- Art inputs come only from capability-declared ports or explicit node ports.
  Undeclared aliases such as a historical `input_image` or `reference` link are
  not inferred.
- An untouched sticker relays its explicit image input; a locally edited sticker
  is a new formal image boundary.
- Art preview pixels are display-only. Downstream execution, persistence, save,
  and export consume the Art's formal output only.
- Browser and native Hook clients both match protocol/request identity and
  consume terminal `art.result`/`art.failure` events or the equivalent terminal
  response without running a local fallback executor.
- Art capabilities and workflow nodes use publisher-qualified identity on the
  bridge, for example `neuro.official/surface-device-dashboard`. The short local
  ID `surface-device-dashboard` remains valid only for Loom's install/store API;
  it is not a Hook catalog alias.

## Verification

本节的 R12/R21 与后续 R14/R23 是历史 release 证据。2026-08-14 审查后的源码新增了
explicit `outputTransports`、strict formal output、multi-output、shared-memory
execution ownership/release、package version 和 canvas shape 收紧；旧 release 不包含
这些修改，不能作为当前源码的发布证明。当前源码的 fresh gate 见
`art-framework-refactor-independent-review-handoff-2026-08-13.md`，新的正式 release
与 600 秒 native acceptance 仍需单独生成。

- Loom: protocol 21, shared image 4, registry 120, workflow 6, daemon 198,
  desktop 141, desktop typecheck/build, Art plugin boundary, third-party plugin
  smoke, and four-framework/six-Art formal Hook smoke all pass.
- Hook: Rust 223, production TypeScript typecheck, and the historical 252 files /
  1046 frontend suite pass. The fresh canonical-identity/default/SHA acceptance
  contract gate passes 2 files / 18 tests.
- The framework smoke installs four packages, installs six independent sample
  Arts, instantiates six Hook nodes, and observes six successful
  `loom.hook.v1` formal executions. It verifies shared-memory image delivery,
  inline fallback, MCP candidate metadata, and workflow formal output.
- Loom release: `release/Loom/20260813-loom-hook-v1-surface-wire-r21` passed its
  49-entry packaged verifier and all seven release smokes. `Loom.exe` SHA-256 is
  `b61245ec646fae56a9af745c6d6a2fa4e3ead5482e791ec7267bf9e2390029cf`;
  `runtime/loom-daemon.exe` is 26,536,448 bytes with SHA-256
  `31746e1376415f2599c96f572e3984afeab33f7cbb8ccaa3a39e63bb0e53d82a`.
  The Plugin SDK ZIP is 715,318 bytes with SHA-256
  `9b1cb8529fb95c44db65307775a7b109516d0ab94b799e3826e63039699be221`;
  its 15,032-byte `protocol/README.md` is byte-identical to the final source.
  The verifier now compares this embedded README's byte count and SHA-256 with
  the release source; a tamper regression proves that an internally rehashed but
  source-divergent SDK ZIP fails.
  A direct A/B run rejects R20 at 14,031 embedded bytes versus 15,032 source
  bytes, while R21 passes the hardened 49-file verifier and all seven smokes.
- Hook release: `release/Hook/20260813-loom-hook-v1-surface-wire-r12/hook.exe`
  was built independently. It is 7,021,568 bytes with SHA-256
  `9c0699eb73eecb86002e9a44ec2cb5cba8f9e123d6b7a215bcb763b1158fa7d8`.
- Final native pairing evidence:
  `Hook/artifacts/r12-r21-full-600s-retry/summary.json`, run
  `20260813-080542-hook-loom-surface-7f3696e112be`, has outer and inner
  `status = passed`. It verified exact artifact digests, native startup and
  single-instance behavior, the `loom.hook.v1` subscriber, canonical workflow
  instantiation, pending-to-approved device pairing (`sessionEpoch = 1`),
  packaged Surface attach/event/resource/formal result, 600-second soak, first
  clean exit, restart recovery of the same Surface instance, another Surface
  interaction, and the final clean exit.
- The soak collected 365 samples. Private bytes grew 405,504 bytes (0.264%)
  from 153,739,264 to 154,144,768, with peak 158,801,920 and no violation.
  Restart advanced the Surface from revision 5 to 8; daemon state was `ready`,
  pending events were empty, and an accepted `succeeded` event ACK was present.
  Both Hook exits returned 0; no forced cleanup was needed. Cleanup stopped the
  isolated daemon and Art Store and left all store, daemon, and bridge listener
  lists empty.
- The earlier `Hook/artifacts/r12-r20-full-600s/summary.json` remains as rejected
  evidence. Pairing, Surface interaction, and soak passed, but real host Delete
  input triggered Hook's normal delete path and a `disposed` lifecycle before
  restart. The missing Surface after that intentional deletion is not a restart
  persistence defect; the clean retry above is the final gate.

R14, R15, and R16 are rejected, immutable candidates rather than release baselines.
R14 exposed retired Hook fixture paths and edge port names. R15 reached the
Surface prototype gate but correctly failed a process-backed action whose
2-second declared deadline did not cover framework-host plus Art-runtime startup;
R16 includes the corrected action budgets and a release contract that prevents
those budgets from regressing below 10 seconds, but was mistakenly built with
`-NoZip` and therefore failed the verifier's required CLI artifact check. R17
closed those packaging issues but predates the exact Surface event namespace.
R18 failed an obsolete Surface smoke fixture, and R19 repeated the `-NoZip`
omission; R20 is the first complete final-wire package. R21 retains the accepted
daemon and embeds the final protocol README in its Plugin SDK, making it the final
Loom release. Hook R10 was the earlier blocked
candidate. R11 exposed an acceptance fixture that sent the short local store ID
instead of the exact publisher-qualified catalog identity. The fixture was fixed
without adding a compatibility alias, and R12 is the rebuilt final Hook package.
The first R12/R21 attempt in `Hook/artifacts/r12-r21-full-600s/summary.json`
failed preflight before candidate launch because the command omitted the
wrapper's expected-daemon SHA override; isolated cleanup passed. The retry used
the independently computed exact hashes and is the final acceptance above.
The two acceptance scripts now default to R12/R21 and their exact SHA-256
values, require 64-hex SHA input, compare unconditionally, and forward the Hook
hash into the inner native runner. A default-path preflight safely resolved and
hashed both final candidates, observed the restored R9 main/watchdog pair,
returned `blocked_existing_hook`, and started no candidate service.

## Non-goals retained

Phase 68 security non-goals and Phase 69 distributed Surface limits remain
unchanged. Phase 71 supersedes this section for data/layout compatibility: Art
Store flat-latest copies, old settings/session shapes, old package layouts,
provider/process field aliases, and obsolete Hook app-data discovery are no
longer retained. WGC-to-GDI capture fallback, inline-resource fallback,
Surface migration/rollback, cancellation, and recovery remain current product
behavior rather than historical compatibility.
