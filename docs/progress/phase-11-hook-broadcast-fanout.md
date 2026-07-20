# Phase 11: Hook Broadcast Fanout

## Goal

Restore ArtHook-style long-lived subscription and broadcast fanout on Loom's
Hook bridge. A WebSocket client that sends `subscribe` should receive later
legacy broadcast frames such as `art_hook/instantiate`; ordinary request/reply
sockets should remain unsubscribed.

## Tasks

- [x] P11.1 Daemon subscribed WebSocket fanout
  - Acceptance:
    `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` proves a
    subscribed WebSocket receives an `art_hook/instantiate` frame triggered by
    a separate `art_loom/instantiate_workflow` request.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon fans_out_broadcasts -- --nocapture` failed before implementation because `subscribe` returned `type = error`.
    - GREEN: `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 32 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P11.2 Release broadcast smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` can keep a subscribed WebSocket open, trigger an
    instantiate broadcast from a second WebSocket, and record the pushed
    broadcast in the smoke summary.
  - Completed: 2026-06-12.
  - Evidence:
    - RED: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` failed until `Test-LoomHookBridgeWebSocketBroadcast` existed in release smoke.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-hook-broadcast-db2be04f -Force` -> generated `release\Loom\loom-hook-broadcast-db2be04f`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-hook-broadcast-db2be04f -Apps Loom` -> passed with `gitDirty = false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-hook-broadcast-db2be04f -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-063724-Loom-61604-4aed79b919e948dfb0e6ce0a6eb697f3\release-local-apps-loom-hook-broadcast-db2be04f-Loom-summary.json`.
    - Broadcast evidence: `method = art_hook/instantiate`, `workflowId = wf-release-broadcast`, `nodeId = release-node`, `edgeTarget = release-output`.

## Evidence

Phase 11 completed request-triggered push fanout for subscribed WebSocket
clients. Release smoke proves packaged `loom-daemon.exe` can keep a subscriber
open, publish from a second WebSocket, and deliver the legacy
`art_hook/instantiate` JSON frame.

## Notes

- This phase restores WebSocket push/fanout behavior only.
- OCR, Python, shared-memory image exchange, and full Art execution engines
  remain out of scope.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
