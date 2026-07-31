# Loom Hook failed-art real-shape fixture hardening plan

## Goal

Strengthen failed-Art preview regression coverage with a fixture that more
closely matches the Hook session shape currently observed in real user data.

## Tasks

- [x] Add a source-level regression test using a realistic Hook Art-node shape:
  - `previewSrc` absent
  - local absolute-path `src`
  - `minified`, `savedRect`, `cropOffset`
  - `params.reference`
  - upstream link using `output -> input`
- [x] Tighten the packaged failed-preview smoke fixture to use the same
  realistic shape.
- [x] Extend the standalone release contract so the new smoke cannot silently
  regress back to a toy fixture.
- [x] Re-run daemon Hook canvas tests, the standalone release contract, the
  dedicated failed-preview smoke, and the full release verification chain.
- [x] Generate a new parent-scoped Loom release.
