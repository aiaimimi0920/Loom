# Loom all-framework fake art-store Hook smoke plan

> Scope: close the remaining end-to-end gap after Phase 45 by proving that all
> six Loom Art frameworks can be installed or exercised through a temporary
> local art-store contract, real Hook node instantiation, and valid
> execute-art-node calls.

## Goal

Add repo-owned coverage and smoke infrastructure for a temporary local
art-store setup that exercises:

- `cli_wrapper`
- `cloud_api`
- `script`
- `python_art`
- `mcp`
- `workflow`

through the real Loom daemon plus Hook Bridge compatibility routes.

## Tasks

- [x] Add failing regression coverage for the remaining runtime gaps:
  - daemon-side `python_art` readiness must reflect a framework-downloaded
    runtime under the control-plane root;
  - daemon Hook execution must prove `python_art` through the real bind/start
    path;
  - install-time path rewriting must not corrupt non-bundled executable names
    such as `powershell.exe`.
- [x] Fix the runtime/root resolution issues:
  - export the resolved daemon control-plane root to the process environment so
    framework-installed `python_art` runtimes are discoverable at execution
    time;
  - probe framework readiness against the real
    `<control-plane>\framework-runtimes` root instead of double-appending the
    framework id;
  - preserve non-bundled command names during Art install while still rewriting
    bundled files.
- [x] Add missing regression coverage for direct Hook execution:
  - `cli_wrapper` Art node image output;
  - `python_art` Art node text output.
- [x] Create a repo-owned PowerShell smoke:
  - start a temporary local fake cloud API server;
  - start a temporary local fake stdio MCP server;
  - build a temporary local fake art-store root with six Art zips plus the
    `python_art` framework runtime zip;
  - start `loom-art-store` and `loom-daemon` against isolated state;
  - install frameworks and one Art for each framework id;
  - instantiate Hook nodes for every installed Art;
  - execute each node once and persist evidence.
- [x] Record the closure in README/progress docs and regenerate a parent-scoped
  Loom release package.
