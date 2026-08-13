# Phase 71: Art canonical-only layout and legacy zero

## Status

Complete. The final R14/R23 artifacts, exact hashes, release verification, and
600-second native dual-end acceptance evidence are recorded below.

Phase 71 supersedes Phase 70 wherever Phase 70 treated persisted data, package
layout, provider/process input, Art Store storage, or Hook app-data migration as
out of scope. This is an intentional breaking cleanup. The project does not
discover, translate, or migrate obsolete development data.

## Canonical production boundary

The only cross-application Art protocol is `loom.hook.v1`, with
`loom.surface.v1` payloads and exact `loom.surface.*` event names. The public
Rust types are in `crates/loom_protocol/src/hook.rs`; the language-neutral
schema is `protocol/schemas/hook-message.v1.schema.json`; Hook mirrors that
contract in `src/services/protocol.ts` and its native bridge.

The production boundary is fail-closed:

- Framework and Art publishers are required.
- Canonical package identity is `publisher/id`.
- Framework packages live only under
  `frameworks/<publisher>/<id>/versions/<version>-<digest>`.
- Art packages live only under
  `arts/<publisher>/<id>/versions/<version>-<digest>`.
- A bare local ID is only a unique query convenience. It never creates or
  resolves a flat package directory.
- Framework process requests use the installed manifest's local framework ID;
  publisher qualification selects and locks the package at the host boundary.
- Art Store packages live only at `arts/<id>/<version>.zip` with matching
  digest sidecars and are fetched by exact version. There is no flat latest
  copy or `/latest` package route.
- Hook's only automatic application data identity is `com.yamiyu.hook`.
  `HOOK_APPDATA_DIR`, `LOOM_HOOK_APPDATA_DIR`, and
  `LOOM_HOOK_SESSION_PATH` remain explicit test/operator isolation controls;
  they do not scan old identifiers.
- Framework manifests require `publisher` and `entry.processModel`. MCP server
  configs require `transport`. Framework success is exactly `status=success`.
- Hook Art capability and execution payloads use the current camelCase wire
  fields only. Historical snake_case/public aliases are rejected.

## Current canvas shapes

Two shapes remain because they are both current producers, not migrations:

1. Hook's persisted `session.json` uses `stickers` and `links`, with link fields
   `fromUnitId`, `toUnitId`, `fromPortId`, and `toPortId`.
2. `loom.hook.workflow.sync` uses `nodes` and `edges`, with edge fields
   `source`, `target`, `sourceHandle`, and `targetHandle`.

The current node types are exact: persisted Hook sessions use internal
`type=sticker` or `type=art`, while the public workflow wire uses `type=sticker`
or `type=artNode`. An Art node must carry a canonical `data.artId`/`artId`:
built-in `core.image.*` identities remain native, and every packaged Art uses
`publisher/id`. Missing/unknown types, `artId`-only nodes, stale `sticker` plus
`artId`, and bare packaged Art IDs fail closed. Sticker workflow YAML uses only
the internal `__sticker__` sentinel.

Both enter `HookCanvasDocument::from_serialized_root`. The removed third shape
(`units`, `sourceNodeId`, `targetNodeId`, and old port aliases) is not accepted.
The live Hook workflow remains the current `hook-live -> latest.yaml` storage
contract.

## Nine canonical Art packages

The six sample packages are:

1. `neuro.official/custom-1770146354922` (`image-compress`)
2. `neuro.official/custom-remove-bg-cloud` (`remove-bg`)
3. `neuro.official/custom-image-search` (`image-search`)
4. `neuro.official/custom-1770131241684` (`color-transfer`)
5. `neuro.official/custom-image-blend-script` (`image-blend`)
6. `neuro.official/custom-image-blend-compress-workflow`
   (`image-blend-compress`)

The three Surface prototypes are:

7. `neuro.official/surface-device-dashboard`
8. `neuro.official/surface-project-form`
9. `neuro.official/surface-stock-card`

Every manifest declares publisher metadata, `metadata.art.qualifiedId`, a
global Art ID, a current framework dependency and version, canonical ports, and
package-local execution. The workflow Art's child dependencies and `uses`
references are publisher-qualified. The previous duplicate source under
`resources/workflow-arts/image-blend-compress` was deleted. Surface actions are
implemented by each prototype's package-local runtime; there is no host switch
that dispatches by prototype Art ID.

## Preview and formal output

- Preview is display-only.
- Downstream execution, persistence, save, and export consume formal outputs
  only.
- Preview and formal output use independent `previewRevision` and
  `resultRevision` counters and stale checks.
- A failed or cancelled generation cannot replace the last successful formal
  result.
- Formal outputs are typed `value`, `inline_resource`, `shared_memory`, or
  `resource` values.
- Local images prefer shared memory and retain typed `inline_resource` as the
  current reliability fallback when shared memory cannot be created.

## Reliability mechanisms retained

The following are current product behavior and are not compatibility layers:

- WGC-first Windows capture with runtime GDI fallback and the explicit
  `HOOK_CAPTURE_BACKEND=gdi` diagnostic override;
- shared-memory-to-inline-resource delivery fallback;
- browser and native Hook clients;
- Surface `fallbackScene` capability negotiation;
- Surface checkpointing, migration, rollback, remount, restart recovery,
  resumable streams, resource leases, journals, tombstones, and corruption
  detection;
- device-bound confirmation and cancellation, generation replacement, and
  late-result rejection;
- tolerant reporting of unknown session fields for forward schema drift,
  without interpreting those fields as historical aliases.

## Compatibility code removed in Phase 71

In addition to Phase 70's deleted ArtLoom/AHRP routes and executors, Phase 71
removes:

- flat `arts/<id>` and `frameworks/<id>` install/runtime/recovery paths;
- package-layout migration and flat latest Art Store copies;
- publisher-optional Framework and Art validation;
- publisher-less Plugin SDK scaffolds;
- fallback Art execution directories outside `metadata.artPackage.dir`;
- old settings, MCP, manifest, response-status, scalar-transport, node geometry,
  and workflow-binding aliases;
- reversed workflow image-binding normalization for historical manifests;
- Art session inference from `artId` when `type=art` is absent;
- Hook Canvas aliases `capture`, `screenshot`, outer `node`/`unit`, nested
  `data.type`, and public workflow `type=art`;
- unvalidated Workflow Store writes/reads, bare workflow child IDs, and the
  historical `uses: sticker` sentinel;
- obsolete Hook app-data identifier discovery;
- per-Art installer wrapper scripts and unused manual ArtLoom sync scripts;
- release smoke probes for removed `/v1/python-arts` routes.

No migration utility or compatibility shim is provided.

## Verification matrix

Fresh source/package gates completed on 2026-08-13:

| Gate | Result |
| --- | --- |
| Rust formatting and combined compile check | Passed |
| `loom_protocol` | 21 passed |
| `loom_mcp` | 19 passed |
| framework runtime host | 4 passed |
| `loom_tool_registry --lib` | 117 passed |
| `loom_workflow_runtime` | 16 passed |
| `loom_workflow_store` | 7 passed |
| `loom-art-store --lib` | 17 passed |
| `loom-daemon --lib --test-threads=1` | 195 passed |
| `loom-plugin-cli --lib` | 9 passed |
| Loom desktop | 141 passed; typecheck and production build passed |
| Hook Rust library | 221 passed; formatting passed |
| Hook frontend | 252 files / 1049 tests passed; typecheck and production build passed |
| Framework package contract | 4 manifests and 4 rebuilt ZIPs passed |
| Sample Art package contract | 6 sources and 6 rebuilt ZIPs passed |
| Surface prototype packaging | 3 ZIPs plus digest sidecars passed |
| Surface prototype runtime smoke | install, actions, cancel, multi-attach, restart recovery passed |
| Third-party plugin boundary smoke | install/execute/upgrade/disable/restart/uninstall passed |
| Framework Art Store Hook smoke | 4 frameworks, 6 Arts, 6 formal `loom.hook.v1` executions passed |
| Loom R23 release verification | 49 files; all 7 release/runtime smoke groups passed |

The Hook frontend suite requires approximately ten minutes in this workspace;
an earlier two-minute harness timeout was not a product failure. The fresh
unbounded run completed all 1049 tests.

## Formal releases and native acceptance

- Hook: `release/Hook/20260813-loom-hook-v1-surface-wire-r14/hook.exe`
  - bytes: `7019520`
  - SHA-256: `341fb0c88a268bd0cece05eacb623e5a3fc02c6238c80c7fe7f66b1854e746d2`
- Loom: `release/Loom/20260813-loom-hook-v1-surface-wire-r23`
  - `Loom.exe`: `02b3cbe635a578c8d100ed330cb341680e843384781a0ba3f301a6c8ff463c9f`
  - `runtime/loom-daemon.exe`: `376f336dcfe97ad83d18d1d9e74397fc36b81f67ac5f6844594012adfd4b75b6`
  - Plugin SDK: `dcba7d0a0e075bb9703982012fe82223b861b54dd45b0061728d2327f70ba9eb`
- Native acceptance:
  `Hook/artifacts/runtime-performance/hook-loom-surface-candidate/20260813-205423-hook-loom-surface-b89e7c2bd751/summary.json`
  - outer run: `20260813-205423-hook-loom-surface-b89e7c2bd751`, `passed`
  - native run: `20260813-205427-a4407e3b25e0`, `passed`
  - 600-second soak: 402 process-tree samples; private bytes
    `154849280 -> 158683136`, growth `3833856` bytes / `2.476%`, peak
    `160337920`, zero violations
  - shared Surface instance:
    `instance:69847ef9-43b3-449b-bbad-7abefc9a049a`
  - revision progression: `1 -> 4 -> 8` across action and restart
  - native WebView2/Tauri startup, single-instance refusal, device approval,
    qualified dashboard attach, click, resource resolution, formal result,
    normal exit, same-instance restart recovery, and final teardown passed
  - forced Hook cleanup: none; daemon/Art Store stopped; candidate processes and
    selected listeners were empty after teardown

Fresh preflight found no pre-existing Hook process, so no user process needed to
be stopped or restored. The two earlier non-passing R13/R23 diagnostics are kept
as evidence: both completed the soak, but shared-desktop global Delete input
disposed the selected test Surface before restart. R14 exposes the existing
explicit `HOOK_NATIVE_ACCEPTANCE=1` state to the frontend and ignores only the
native global Delete event in that gated test mode; normal product input and
protocol lifecycle behavior are unchanged.
