# Loom Hook failed-art browser-view failure sync plan

## Goal

Keep Loom's visual Hook browser view aligned with Hook when an Art node has
entered the failed/error state.

## Tasks

- [x] Confirm the remaining user-visible drift is in the desktop/browser
  presentation layer rather than the daemon preview resolver.
- [x] Add desktop regression coverage for failed Art node presentation.
- [x] Update the Hook canvas node renderer so failed Art nodes show an explicit
  execution-failure placeholder instead of an image preview.
- [x] Extend packaged Hook canvas UI smoke coverage to prove the thumbnail and
  full canvas both show the failed state for the same node.
- [x] Re-run desktop tests, Hook UI/release contracts, and the packaged release
  verification chain.
- [x] Generate a new parent-scoped Loom release.
