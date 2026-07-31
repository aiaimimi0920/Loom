# Loom image-compress runtime preview overlay plan

**Goal:** Ensure a successful image-producing Art node can display its real
runtime output in Loom's Hook canvas even when Hook's live workflow snapshot
contains a blank `previewSrc`.

**Architecture:** Reuse the existing Hook-canvas runtime overlay mechanism.
Status/error overlays already live outside the persisted Hook snapshot; this
phase extends that overlay to carry a runtime preview image source and cache
token so the daemon preview endpoint can prefer the real runtime output over a
blank live preview payload.

**Tech Stack:** Rust 2021, daemon Hook canvas overlay logic, Hook Bridge runtime
tests, release PowerShell scripts.

---

## File Map

Create:

```text
docs/progress/phase-57-image-compress-runtime-preview-overlay.md
docs/superpowers/plans/2026-07-29-loom-image-compress-runtime-preview-overlay.md
```

Modify:

```text
apps/daemon/src/hook_canvas.rs
apps/daemon/src/lib.rs
docs/progress/MASTER.md
```

---

## Tasks

- [x] **Task 1: Reproduce the blank-preview case with a failing daemon test**
  - Build a live Hook workflow snapshot whose Art node carries a blank
    `previewSrc`.
  - Prove that a successful runtime image output still leaves the preview blank
    before the fix.

- [x] **Task 2: Overlay real runtime preview images**
  - Extend runtime node state with preview image data.
  - Override the Hook canvas preview source/cache token from successful runtime
    image outputs.
  - Clear stale runtime preview overlays on error or missing output.

- [x] **Task 3: Verify and package**
  - Run the targeted daemon regression.
  - Run the broader Hook canvas daemon test slice.
  - Build a new parent-scoped Loom release.
