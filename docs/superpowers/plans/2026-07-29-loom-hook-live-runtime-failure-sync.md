# Loom Hook live-runtime failure sync plan

## Goal

Keep Loom's Hook browser view aligned with Hook's real runtime failure state,
even when the live Hook workflow snapshot still contains a usable preview image.

## Tasks

- [x] Confirm the remaining drift comes from missing runtime failure state, not
  from the already-fixed desktop placeholder rule.
- [x] Add daemon regression coverage for a live `overwrite_workflow` snapshot
  and for `art/process` failure overlay behavior.
- [x] Cache the live Hook workflow snapshot inside the daemon and use it as the
  preferred browser-view source.
- [x] Overlay runtime node status for both `execute_art_node` and best-effort
  matched `art/process` calls.
- [x] Ensure runtime overlay changes produce a fresh Hook canvas revision.
- [x] Rebuild and verify a new parent-scoped Loom release.
