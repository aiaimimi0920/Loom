# Hook canvas projection

The **Hook 同步** workbench shows Hook node positions, image previews and links
from the daemon's normalized canvas model. It does not establish another
persistent authority for Hook's session. YAML and technical diagnostics remain
under the advanced disclosure rather than the normal canvas workflow.

## Navigation and workflow actions

Selecting a live node selects its connected component for saving as a workflow.
Saving switches to the saved workflow. Saved workflows support rename/delete,
instantiation on the Hook desktop in `reference` mode and export to Art creation.
The workflow definition and exposed interface follow the
[workflow contract](WORKFLOW_CONTRACT.md).

The projected canvas supports viewport panning, wheel/button zoom, arrow-key
panning, Home-to-fit and minimap recentering. These gestures do not move or resize
Hook nodes or edit their connections. Node selection and the parameter/interface
controls are separate from viewport gestures. The previous proposal to navigate
to a second editor on every thumbnail/node click is not the current selection
contract. Current owners are `HookCanvasThumbnail.tsx`, `HookCanvasSurface.tsx`
and `useHookCanvasViewport.ts` under `apps/desktop/src/components/hook`.

## Synchronization and degradation

Two current producers have distinct canonical shapes: persisted Hook sessions
use `stickers`/`links` and `fromUnitId`/`toUnitId`/`fromPortId`/`toPortId`; the
`loom.hook.workflow.sync` wire uses `nodes`/`edges` and
`source`/`target`/`sourceHandle`/`targetHandle`. Persisted node types are `sticker`
or `art`; public workflow types are `sticker` or `artNode`. Packaged Art IDs must
be publisher-qualified; native `core.image.*` identities remain separate.
These are two active producers, not automatic migration of historical forms.
Do not infer an Art from an ID alone or accept missing/unknown/stale node types.
The live workflow retains the `hook-live` to `latest.yaml` storage contract.

The desktop reads `GET /v1/hook-bridge/canvas` and listens for
`loom.hook.workflow.updated` and `loom.hook.capabilities.updated`. A workflow
update naming a workflow other than `hook-live` is ignored by the live refresh
owner. Do not restore retired `art_loom` event aliases from historical designs.

Refresh is disabled while Loom is offline. If a refresh returns an unavailable
snapshot, the UI retains the previous available snapshot in memory. This is not
a persistent offline snapshot store. A preview-load failure shows a node-level
placeholder without removing its geometry and links or invalidating other nodes.
The desktop does not claim a separate last-image fallback cache.

## Preview security boundary

`GET /v1/hook-bridge/canvas/nodes/{nodeId}/preview` resolves a node in the current
daemon document; it does not accept an arbitrary filesystem path from the caller.
The desktop uses daemon-provided preview URLs rather than treating a local path
as read authority.

File previews are canonicalized and restricted to the session's `images/` or
Hook clipboard-cache roots. The response path rechecks canonicalization and the
allowed root before serving bytes. Asset and `file://` paths are decoded;
non-asset remote HTTP(S) image sources are rejected rather than fetched. Image
data URLs and file bodies are capped at 20 MiB, data URL headers at 128 bytes,
and preview-chain traversal at depth 64. Authentication remains the daemon's
normal route policy; the old design's loopback-only assumption is not permission
to bypass authorization on an explicitly configured remote deployment.

Source owners are `apps/daemon/src/hook_canvas/{document,preview_sources,
preview_candidates,model}.rs` and
`apps/daemon/src/runtime/hook_canvas_preview_session.rs`.

## Verification

`scripts/tests/Test-HookCanvasUiContract.ps1`, daemon canvas/preview tests and
desktop canvas service/component tests protect normalization, bounds, selection,
parameter exposure, workflow packaging and preview failure behavior. Real UI
smokes use isolated control-plane/session data, ports and WebView2 profiles;
they must not modify user sessions or stop unrelated Hook/Loom processes.

Historical screenshots, candidate versions and design alternatives remain in
Git tag `cleanup-base-20260928`. Their removal does not establish fresh native
UI or paired-device acceptance; verify the actual package for release claims.
