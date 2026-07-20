# Phase 37: Hook Live Workflow Release Smoke

## Goal

Turn the Phase 36 Hook live workflow fix into packaged-release evidence.

Phase 36 made `hook-live` visible/openable in the desktop and persisted
`art_loom/instantiate_workflow` snapshots to `latest.yaml`, but the packaged
release smoke still only proved the old WebSocket broadcast. It did not prove
that a Hook-generated workflow appears in `GET /v1/workflows`, can be loaded as
`/v1/workflows/hook-live`, and keeps enough graph data for the workflow
workbench to open it.

## Root cause

- Release smoke covered `art_loom/instantiate_workflow ->
  art_hook/instantiate` broadcast, but not the persistence side effect.
- Adding that smoke first exposed a real Windows client compatibility bug:
  daemon JSON responses used `Content-Type: application/json` without
  `charset=utf-8`. PowerShell decoded Chinese workflow names as ANSI mojibake,
  so `Hook 实时工作流` became `Hook å®æ¶å·¥ä½æµ`.
- The first smoke graph used a dangling edge target that was not present in the
  node list. Loom's YAML workflow format intentionally converts valid node-to-
  node edges into `needs` plus `with` bindings, so the release smoke now uses a
  valid two-node graph and verifies the generated edge binding.

## Changes

- Added release smoke function `Test-LoomHookLiveWorkflowPersistence`.
  - Sends `art_loom/instantiate_workflow` through the packaged Hook WebSocket.
  - Verifies `GET /v1/workflows` contains canonical id `hook-live`.
  - Verifies `/v1/workflows/hook-live` loads the saved YAML.
  - Verifies the Chinese label remains `Hook 实时工作流`.
  - Verifies source node, target node, and the generated edge binding
    `nodes.hook-live-release-node.outputs.screenshot` are persisted.
- Added parity contract checks requiring:
  - `Test-LoomHookLiveWorkflowPersistence`
  - `/v1/workflows/hook-live`
  - `hookLiveWorkflow` summary evidence
- Fixed daemon HTTP JSON responses to declare
  `Content-Type: application/json; charset=utf-8`.
- Added daemon regression test `json_http_responses_declare_utf8_charset`.

## Validation

RED checks observed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

Failed as expected before the smoke function existed:

```text
Loom release smoke must prove Hook-generated live workflow persistence. Missing=[Test-LoomHookLiveWorkflowPersistence]
```

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom-daemon json_http_responses_declare_utf8_charset -- --nocapture
```

Failed as expected before JSON responses declared UTF-8:

```text
assertion failed: response.contains("Content-Type: application/json; charset=utf-8")
```

GREEN/regression validation:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
cargo test --manifest-path Loom\Cargo.toml -p loom_hook_bridge handler_instantiates_workflow_and_writes_hook_live_yaml -- --nocapture
cargo test --manifest-path Loom\Cargo.toml -p loom-daemon json_http_responses_declare_utf8_charset -- --nocapture
cargo test --manifest-path Loom\Cargo.toml -p loom-daemon -- --test-threads=1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-hook-live-release-smoke-phase37 -Force
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-hook-live-release-smoke-phase37 -Apps Loom
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-hook-live-release-smoke-phase37 -Apps Loom
```

Results:

- Parity contract passed.
- `loom_hook_bridge` targeted persistence test passed.
- `loom-daemon` UTF-8 targeted test passed.
- Full `loom-daemon` validation passed: 60 lib tests and 2 CLI contract tests.
- Formal release verification passed:
  - `gitHead = 1ac1315b4e6a7738560ba7f57aaa9ea49f1f722f`
  - `gitDirty = false`
  - `checksumEntries = 31`
- Full packaged Loom release smoke passed with Hook live evidence:
  - `hookLiveWorkflow.workflowId = "hook-live"`
  - `hookLiveWorkflow.listName = "Hook 实时工作流"`
  - `hookLiveWorkflow.nodePersisted = true`
  - `hookLiveWorkflow.targetNodePersisted = true`
  - `hookLiveWorkflow.edgePersisted = true`

Smoke summary:

```text
output\smoke\runs\20260616-134034-Loom-60672-08955ae2c3e84212b0761de4198ff06f\release-local-apps-loom-hook-live-release-smoke-phase37-Loom-summary.json
output\smoke\latest\release-local-apps-loom-hook-live-release-smoke-phase37-Loom-summary.json
```

## Release

Generated release:

```text
release\Loom\loom-hook-live-release-smoke-phase37
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
release\Loom\loom-hook-live-release-smoke-phase37\packages\Loom-loom-hook-live-release-smoke-phase37-windows-x64.zip
```

Package sha256:

```text
dd9154f921f98c9b010b66c85991de37b8b3b2186bf2b04130df1b4b35046aa8
```

## User-facing impact

- `loom-desktop.exe` remains the normal UI entry.
- The Hook-generated workflow is now covered by release smoke as a real
  workbench workflow, not just a WebSocket broadcast.
- Chinese Loom UI/API labels survive Windows PowerShell and other HTTP clients
  that rely on response charset declarations.
