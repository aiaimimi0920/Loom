# Loom image-compress compat sync guard plan

**Goal:** Stop legacy `sync_user_arts` payloads from clobbering the
Loom-managed `图片压缩` Pingo art when they reuse the same art id, while keeping
the compat surfaces able to see the installed art.

**Architecture:** Distinguish between compat-visible arts and sync-owned arts.
Loom-local installed compat arts remain visible to ArtLoom compat routes, but
`sync_user_arts` only owns legacy `artloom-compat` entries and must not replace
an existing `loom-local` tool on id collision.

**Tech Stack:** Rust 2021, daemon ArtLoom compat layer, Hook Bridge WebSocket
tests, PowerShell recovery installer.

---

## File Map

Create:

```text
docs/progress/phase-56-image-compress-compat-sync-guard.md
docs/superpowers/plans/2026-07-29-loom-image-compress-compat-sync-guard.md
```

Modify:

```text
apps/daemon/src/lib.rs
docs/progress/MASTER.md
```

---

## Tasks

- [x] **Task 1: Reproduce the overwrite path with a failing daemon test**
  - Add a regression proving that a Loom-local installed compat art is
    overwritten by a colliding legacy `sync_user_arts` payload before the fix.

- [x] **Task 2: Preserve Loom-local compat arts during compat sync**
  - Keep Loom-local arts visible to compat listing/get routes.
  - Restrict `sync_user_arts` ownership to sync-managed compat entries only.
  - Skip overwrite when a colliding payload targets an existing Loom-local art.

- [x] **Task 3: Repair live state and rebuild Loom**
  - Reinstall the `图片压缩` Pingo art into the current control-plane registry.
  - Verify live HTTP and Hook execution paths.
  - Build a new parent-scoped Loom release.
