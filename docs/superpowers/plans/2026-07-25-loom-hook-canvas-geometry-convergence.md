# Loom Hook Canvas Geometry Convergence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Hook-canvas data-geometry that is independent of viewport state into the Loom daemon so Hook/Loom do not keep re-deriving the same graph geometry in multiple places.

**Architecture:** Keep A-class geometry in `apps/daemon/src/hook_canvas.rs` and expose it through the existing `GET /v1/hook-bridge/canvas` response. Keep B-class viewport/pan/zoom/minimap projection logic in `apps/desktop/src/services/hookCanvas.ts` and UI components. The frontend should consume daemon-provided world-space geometry first and only fall back to local derivation for backward compatibility.

**Tech Stack:** Rust daemon (`serde`, unit tests), React desktop (`TypeScript`, node:test), existing Hook Bridge canvas snapshot contract.

---

## Scope

This plan intentionally covers only the **safe, high-value A-class geometry**:

1. **Edge world endpoints** for Hook-style port-aligned links
2. **Connected-component ids** for pipeline highlighting / selection reuse
3. **Frontend consumption + compatibility fallback**

It intentionally does **not** migrate `buildSubWorkflowYaml(...)` into the daemon in this round, because that changes export responsibility rather than pure geometry ownership.

---

## File map

- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
  - Add world-space edge anchors and per-node component ids to the daemon snapshot
  - Add Rust tests for edge geometry and component-id derivation
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.ts`
  - Extend TS snapshot types
  - Prefer daemon geometry for edge rendering / connected-component lookup
  - Keep old frontend derivation as fallback
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\components\hook\HookCanvasThumbnail.tsx`
  - Reuse daemon edge world endpoints for minimap projection
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.test.ts`
  - Add tests for daemon-fed edge projection and componentId-first lookup

---

### Task 1: Daemon-owned edge world endpoints

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs` (existing test module)

- [x] Add `HookCanvasPoint` and extend `HookCanvasEdge` with `sourcePoint` / `targetPoint`
- [x] Compute edge anchors in **world coordinates** using Hook-aligned port gaps
- [x] Preserve minified-vs-normal gap behavior
- [x] Add daemon unit coverage for edge anchor derivation

### Task 2: Daemon-owned connected-component ids

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs` (existing test module)

- [x] Extend `HookCanvasNode` with `componentId`
- [x] Compute component membership from edges inside the daemon snapshot pass
- [x] Add daemon unit coverage for component-id grouping

### Task 3: Frontend consumption with compatibility fallback

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.ts`
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\components\hook\HookCanvasThumbnail.tsx`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.test.ts`

- [x] Extend TS snapshot types with `componentId`, `sourcePoint`, `targetPoint`
- [x] Make `edgeEndpoints(...)` project daemon world points through current layout when present
- [x] Make minimap reuse daemon world points instead of recomputing edge anchors locally
- [x] Make `connectedNodeIds(...)` prefer daemon `componentId`, fallback to BFS only for old snapshots
- [x] Add / update desktop tests

### Task 4: Verification

**Files:**
- None

- [x] Run `cargo test -p loom-daemon hook_canvas`
- [x] Run `npm run typecheck` in `apps/desktop`
- [x] Run `npm run test` in `apps/desktop`

---

## Exit criteria

- The daemon snapshot directly carries Hook-style edge anchor geometry in world coordinates.
- The daemon snapshot directly carries connected-component membership.
- Desktop edge rendering and minimap projection consume daemon geometry first.
- Desktop pipeline highlighting consumes daemon component ids first.
- Viewport/pan/zoom/minimap camera logic remains frontend-owned and unchanged in responsibility.
- Rust and frontend verification pass.

---

## Deferred follow-up (not in this plan)

- Evaluate whether `buildSubWorkflowYaml(...)` should move behind a daemon API, but only after confirming the current component-id convergence is stable in practice.
