# Phase 46: All-framework fake art-store Hook smoke

## Status

Complete.

## Why this phase exists

Phase 45 closed Loom's installable-framework and art-store feature line at the
API/documentation level, but it still lacked one repo-owned end-to-end proof:

- a temporary local server pretending to be the Art store;
- one Art per framework id;
- real Hook node instantiation; and
- one valid `execute_art_node` call per framework.

This phase closes that last gap and also fixes the product issues uncovered by
the first real all-framework smoke.

## Implemented

- Fixed daemon startup so the resolved control-plane root is exported back to
  the process environment:
  - `LOOM_CONTROL_PLANE_ROOT`
  - `LOOM_FRAMEWORK_RUNTIMES_DIR`
- Fixed `FrameworkRegistry::readiness()` so `python_art` probes the real
  `<control-plane>\framework-runtimes` root instead of double-appending the
  framework id.
- Fixed Art install path rewriting so non-bundled executable names such as
  `powershell.exe` are preserved, while bundled files are still rewritten into
  the installed Art directory.
- Added regression coverage for:
  - framework runtime detail resolution for installed `python_art`;
  - daemon `/v1/tools/{toolId}/readiness` with a downloaded `python_art`
    runtime;
  - direct Hook `cli_wrapper` Art node execution;
  - direct Hook `python_art` Art node execution;
  - preserving non-bundled CLI command names during install.
- Added `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1`, which:
  - builds or reuses local Loom binaries;
  - starts a temporary local fake cloud API server;
  - starts a temporary local stdio MCP server;
  - prepares a temporary local Art store root with:
    - six Art packages (`cli_wrapper`, `cloud_api`, `script`, `python_art`,
      `mcp`, `workflow`);
    - one downloadable `python_art` framework runtime zip;
  - starts `loom-art-store` and `loom-daemon` against isolated state;
  - installs the `mcp` and `python_art` frameworks;
  - installs one Art per framework through `/v1/arts/store/install`;
  - instantiates six Hook nodes through
    `/v1/artloom-compat/ipc/instantiate-workflow`; and
  - executes all six through
    `/v1/artloom-compat/ipc/execute-art-node`.

## Verification

Commands run:

```powershell
cargo fmt --all
cargo test -p loom_tool_registry -- --nocapture --test-threads=1
cargo test -p loom-daemon -- --nocapture --test-threads=1
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\Invoke-LoomFrameworkArtStoreHookSmoke.ps1 `
  -Configuration Debug `
  -EvidenceRoot .\target\framework-art-store-hook-smoke
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId 20260729-framework-art-store-hook-smoke `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-framework-art-store-hook-smoke
```

Results:

- `cargo test -p loom_tool_registry -- --nocapture --test-threads=1`: 44 passed, 0 failed.
- `cargo test -p loom-daemon -- --nocapture --test-threads=1`: 149 daemon
  library tests and 8 CLI contract tests passed.
- `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` passed and proved:
  - `store-cli-art` image output;
  - `store-script-art` image output;
  - `store-cloud-art` image output;
  - `store-python-art` text output;
  - `store-mcp-art` text output; and
  - `store-workflow-art` image output.
- Parent-scoped release build completed successfully.
- Release verification completed with:
  - `filesChecked = 32`
  - `smoke = not-run`
  - `hookCanvasSmoke = not-run`

## Evidence

- Fake-store Hook smoke summary:

```text
target\framework-art-store-hook-smoke\20260729-125906-framework-store-12316-25345787c7834c0dafaefa7658cd1f29\summary.json
```

- That run contains:
  - the isolated control-plane root;
  - `loom-art-store` / `loom-daemon` / fake cloud logs;
  - the fake cloud request capture;
  - the fake MCP tool-call capture; and
  - the per-framework Hook execution results.

## Release

Generated:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260729-framework-art-store-hook-smoke
```

Release manifest summary:

```text
gitDirty: true
checksumEntries: 32
desktop zip sha256: 5e5c1f7d9d69c9ba322d7d7bc4a91dc54e46e7eac56bc9619f38ccf87feceb7e
cli zip sha256: f5b8a5ad97ea35148bfe2ad3af18ac237225ec44ce67a0702fbfcc735a678922
```

## Boundaries

This phase proves the Loom-side end-to-end contract with a temporary local
fake store and fixtures. It does not add a permanent hosted Art marketplace,
does not bundle the fake store fixtures into desktop runtime payloads, and does
not change the broader release smoke matrix beyond adding this dedicated local
framework/store/Hook proof path.
