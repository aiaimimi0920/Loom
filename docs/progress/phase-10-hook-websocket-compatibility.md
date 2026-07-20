# Phase 10: Hook WebSocket Compatibility

## Goal

Restore the missing ArtLoom/ArtHook WebSocket transport layer so external Hook
clients can connect to Loom's Hook bridge and receive legacy request/reply JSON
responses.

## Tasks

- [x] P10.1 Daemon WebSocket request/reply transport
  - Acceptance:
    `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` proves a real
    WebSocket client can connect to the started bridge, send legacy
    `handshake`, receive a `handshake` response, and observe connected client
    count.
  - Completed: 2026-06-12.
  - Evidence:
    - `cargo test --manifest-path Loom/Cargo.toml -p loom-daemon` -> 31 lib tests, 2 binary contract tests passed.
    - `cargo check --manifest-path Loom/Cargo.toml -p loom-daemon` -> passed.
    - `cargo fmt --manifest-path Loom/Cargo.toml --all -- --check` -> passed.
- [x] P10.2 Release WebSocket smoke
  - Acceptance: regenerated Loom release smoke proves packaged
    `loom-daemon.exe` accepts a WebSocket handshake on the started bridge port.
  - Completed: 2026-06-12.
  - Evidence:
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1` -> passed.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-hook-ws-6b8410b8 -Force` -> generated `release\Loom\loom-hook-ws-6b8410b8`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-hook-ws-6b8410b8 -Apps Loom` -> passed with `gitDirty = false`.
    - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-hook-ws-6b8410b8 -Apps Loom` -> passed.
    - Smoke summary: `output\smoke\runs\20260612-061836-Loom-50752-224911e7514b4cbeb24e71f6709019c5\release-local-apps-loom-hook-ws-6b8410b8-Loom-summary.json`.
    - WebSocket handshake evidence: `type = handshake`, `serverVersion = 0.1.0`, `hasSessionId = true`.

## Evidence

P10.1 completed with a real `tungstenite` WebSocket client test proving
legacy `handshake` request/reply transport and connected client accounting.
P10.2 completed with a packaged `loom-daemon.exe` smoke test that starts the
Hook bridge on a runtime port, connects with a real .NET `ClientWebSocket`,
sends the legacy `handshake` request, and records the response in the release
smoke summary.

## Notes

- This phase restores request/reply WebSocket compatibility.
- Broadcast fanout for subscribed long-lived clients is deferred to a later
  phase.
- Old source reference is read-only:
  `Z:\project\project\ArtNexus\ArtLoom`.
- Keep visible product naming as Loom.
