# Loom Hook Canvas Workflow Export Metadata Convergence Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Continue converging Hook-canvas A-class graph data into the Loom daemon by precomputing sub-workflow export metadata, while keeping B-class viewport/pan/zoom logic in the frontend.

**Architecture:** Extend the existing `GET /v1/hook-bridge/canvas` snapshot so each node carries daemon-owned workflow-export metadata: a stable YAML-safe `workflowNodeId` and direct `upstreamWorkflowNodeIds`. The desktop keeps the current `buildSubWorkflowYaml(...)` entry point, but it should become a thin serializer that prefers daemon metadata and only falls back to local derivation for backward compatibility.

**Tech Stack:** Rust daemon (`serde`, unit tests), React desktop (`TypeScript`, node:test), existing Hook canvas snapshot contract.

---

## Scope

This round does **not** introduce a new daemon export endpoint and does **not** change any refresh cadence. It only enriches the existing Hook canvas snapshot payload with more A-class data.

Included:

1. Stable per-node `workflowNodeId`
2. Stable per-node `upstreamWorkflowNodeIds`
3. Frontend `buildSubWorkflowYaml(...)` consumes daemon metadata first
4. Backward-compatible fallback stays in place for older snapshots

Not included:

- moving the whole "save workflow" command into daemon
- changing any viewport/minimap/zoom interaction logic

---

## File map

- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
  - add workflow export metadata to snapshot nodes
  - add Rust tests for uniqueness and upstream dependency derivation
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.ts`
  - extend TS types
  - make `buildSubWorkflowYaml(...)` prefer daemon metadata
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.test.ts`
  - add daemon-metadata-driven YAML export tests

---

### Task 1: Daemon-owned workflow export metadata

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\daemon\src\hook_canvas.rs` (existing test module)

- [x] Add `workflowNodeId` and `upstreamWorkflowNodeIds` to `HookCanvasNode`
- [x] Generate YAML-safe ids in the daemon with duplicate-safe suffixes
- [x] Derive direct upstream workflow ids from canvas edges
- [x] Add a Rust test covering duplicate `artId` / duplicate base ids and upstream derivation

### Task 2: Frontend serializer convergence

**Files:**
- Modify: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.ts`
- Test: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom\apps\desktop\src\services\hookCanvas.test.ts`

- [x] Extend TS types for daemon export metadata
- [x] Update `buildSubWorkflowYaml(...)` to prefer daemon metadata when all selected nodes provide it
- [x] Keep existing local derivation as fallback for compatibility
- [x] Add a desktop test that proves daemon metadata drives YAML output

### Task 3: Verification

**Files:**
- None

- [x] Run `cargo test -p loom-daemon hook_canvas`
- [x] Run `npm run typecheck` in `apps/desktop`
- [x] Run `npm run test` in `apps/desktop`

---

## Exit criteria

- The daemon snapshot directly carries stable workflow export ids and upstream dependency ids.
- `buildSubWorkflowYaml(...)` in the desktop becomes a serializer over daemon metadata whenever available.
- Old snapshots remain supported via frontend fallback logic.
- All Loom daemon and desktop verifications pass.
