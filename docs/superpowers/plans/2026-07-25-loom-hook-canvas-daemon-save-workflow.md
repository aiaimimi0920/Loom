# Loom Hook Canvas Daemon Save Workflow Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the "save selected Hook pipeline as a workflow" responsibility from the Loom frontend into the Loom daemon so the frontend no longer has to assemble and persist Hook-derived YAML itself.

**Architecture:** Reuse the existing `GET /v1/hook-bridge/canvas` snapshot as the daemon's data source and add one low-frequency save endpoint that accepts only the selection identity (`selectedNodeId`) plus the target workflow id/name. Keep the current frontend YAML builder as a compatibility fallback for older daemons, but prefer the daemon-owned export path.

**Tech Stack:** Rust daemon (`hook_canvas.rs`, HTTP routing, workflow store), React desktop (`TypeScript`, node:test), existing workflow store endpoints.

---

## Scope

Included in this round:

1. Add a daemon Hook-canvas workflow export/save endpoint
2. Add a daemon-side YAML exporter based on already-downshifted A-class metadata
3. Add a desktop API helper for the new endpoint
4. Make Hook canvas save action prefer daemon save, fallback to legacy frontend YAML only on endpoint absence

Not included:

- any new high-frequency polling
- any viewport/pan/zoom/minimap interaction changes
- removal of the frontend YAML fallback

---

## File map

- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
  - add workflow YAML export helper from selected node/component
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\lib.rs`
  - add request type
  - add `PUT /v1/hook-bridge/canvas/workflows/{workflowId}` route
  - add daemon HTTP contract test
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\loomApi.ts`
  - add `saveHookCanvasWorkflow(...)`
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\loomApi.test.ts`
  - add API helper path/body contract test
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\components\hook\HookCanvasThumbnail.tsx`
  - switch save action to daemon-first with 404 fallback

---

### Task 1: Daemon export helper

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs` (existing test module)

- [x] Add a daemon method that exports the selected node's connected component as workflow YAML
- [x] Make it rely on daemon-owned `workflowNodeId` and `upstreamWorkflowNodeIds`
- [x] Add a Rust test covering exported YAML content and component scoping

### Task 2: Daemon HTTP save endpoint

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\lib.rs`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\lib.rs` (existing daemon integration tests)

- [x] Add `SaveHookCanvasWorkflowRequest`
- [x] Add `PUT /v1/hook-bridge/canvas/workflows/{workflowId}`
- [x] Save exported YAML through the existing workflow store
- [x] Add an integration test proving the endpoint writes the expected workflow

### Task 3: Desktop daemon-first save path

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\loomApi.ts`
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\loomApi.test.ts`
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\components\hook\HookCanvasThumbnail.tsx`

- [x] Add desktop helper for the new daemon endpoint
- [x] Add a test that locks request path/body
- [x] Make Hook canvas save action call daemon first
- [x] Keep legacy frontend YAML fallback only for missing endpoint (`HTTP 404`)

### Task 4: Verification

**Files:**
- None

- [x] Run `cargo test -p loom-daemon`
- [x] Run `npm run typecheck` in `apps/desktop`
- [x] Run `npm run test` in `apps/desktop`

---

## Exit criteria

- Saving a selected Hook canvas pipeline can be performed fully inside the daemon using only `selectedNodeId` + `workflowId`.
- The desktop no longer needs to build YAML on the happy path.
- Older daemons remain supported via 404 fallback.
- Rust and desktop verification pass.
