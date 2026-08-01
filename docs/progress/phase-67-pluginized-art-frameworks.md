# Phase 67: Pluginized Art frameworks

## Status

Planned. No implementation changes have started after the baseline tag.

## Why this phase exists

Phases 45 through 66 proved that Loom can install and execute representative
Arts for the six current framework IDs:

- `cli_wrapper`
- `cloud_api`
- `script`
- `python_art`
- `mcp`
- `workflow`

That proof is not enough for the final product boundary. The current code still
documents and implements several frameworks as built-in/default-installed
capabilities, and parts of the execution behavior remain inside the Loom host.
The target is a true plugin model:

- frameworks are optional packages;
- Art nodes are optional packages;
- a third-party author can build, package, install, and run an Art without
  changing Loom source;
- the same author does not need Hook source access;
- Hook renders dynamic Art capabilities generically.

## Baseline restore point

Before this phase, local annotated tags were created:

| Repository | Tag | Commit |
| --- | --- | --- |
| Loom | `框架修改前的最后一个版本` | `a8e3df0712bcaa4ba640d8848cf82d2271582054` |
| Hook | `框架修改前的最后一个版本` | `a86272a5b06e3b3f5a92d01dda6be138ab6e087f` |

Use these tags as the rollback boundary if the pluginization work needs to be
abandoned or restarted.

## Plan

Detailed implementation plan:

```text
docs/superpowers/plans/2026-08-01-loom-pluginized-art-frameworks.md
```

## Scope

In scope:

- package-backed framework install state;
- framework package manifest and ZIP format;
- generic external framework process execution protocol;
- six repo-owned framework packages built outside the default release payload;
- six repo-owned sample Art packages built outside the default release payload;
- Hook generic capability rendering for plugin Arts;
- release and smoke guards proving a third party can install without host
  source changes.

Out of scope for this phase:

- hosted public marketplace operations;
- code signing and trust policy beyond local checksum/manifest validation;
- remote payment/licensing;
- cloud provider credential UI beyond existing framework/Art manifest
  declarations.

## Progress checklist

- [ ] Task 1: Add source-contract guards before runtime changes.
- [ ] Task 2: Define framework package manifests and explicit installed state.
- [ ] Task 3: Implement framework package install, disable, upgrade, and uninstall.
- [ ] Task 4: Add the generic external framework execution protocol.
- [ ] Task 5: Convert the six sample frameworks into independent packages.
- [ ] Task 6: Convert the six sample Arts into external Art packages.
- [ ] Task 7: Make Hook fully capability-driven for plugin Arts.
- [ ] Task 8: Add end-to-end plugin boundary smoke.
- [ ] Task 9: Update documentation and remove default-build/resource leakage.
- [ ] Task 10: Build final Loom and Hook releases.

## Acceptance checklist

- [ ] Default Loom release contains no optional Art framework runtime package.
- [ ] Fresh control-plane root starts with zero installed optional frameworks.
- [ ] Framework installation is package-backed rather than a built-in flag flip.
- [ ] Framework disable/enable/upgrade/uninstall are supported.
- [ ] Art disable/enable/upgrade/uninstall are supported.
- [ ] Six sample framework packages install and execute successfully.
- [ ] Six sample Art packages install and execute successfully.
- [ ] A temporary third-party framework package installs and executes.
- [ ] A temporary third-party Art package installs and executes.
- [ ] Hook has no production branch on sample Art IDs.
- [ ] Loom has no production branch on sample Art IDs outside fixtures/tests/docs.
- [ ] `verify-release.ps1 -RunSmoke` includes the plugin boundary smoke.
- [ ] Final Loom release exists under
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`.

## Notes

- Current formal framework list comes from
  `crates/loom_tool_registry/src/framework.rs`.
- Current README explicitly says `cli_wrapper`, `cloud_api`, `script`, and
  `workflow` are installed by default. This must be removed during this phase.
- Current Color Transfer is implemented as `python_art` with Hook-facing
  shader compatibility metadata. Treat "shader" as UI/capability behavior
  unless product requirements promote it into a seventh framework ID.

