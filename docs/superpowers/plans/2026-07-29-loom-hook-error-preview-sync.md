# Loom Hook failed-art preview sync plan

## Goal

Keep Loom's Hook canvas preview behavior aligned with Hook when an Art node has
already produced its own local failed/error preview image.

## Tasks

- [x] Inspect the real Hook session shape and confirm how failed/saved Art-node
  previews are represented in current session JSON.
- [x] Add a failing daemon regression test proving Loom currently prefers the
  upstream input image over the Art node's own local failed preview.
- [x] Fix the Hook canvas preview resolver so non-screenshot nodes prefer their
  own local preview sources before falling back to connected upstream input.
- [x] Re-run Hook canvas daemon tests.
- [x] Rebuild a parent-scoped Loom release and verify the standard smoke chain
  still passes.
