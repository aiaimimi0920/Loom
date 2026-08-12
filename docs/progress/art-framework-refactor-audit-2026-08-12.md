# Art framework refactor completion audit (2026-08-12)

## Audit basis

This audit checks the current repository and release pipeline against:

1. `docs/superpowers/plans/2026-08-01-loom-pluginized-art-frameworks.md`;
2. `docs/progress/phase-67-pluginized-art-frameworks.md`;
3. `docs/progress/phase-68-art-plugin-platform-hardening.md`;
4. the current four-framework architecture in `docs/ARCHITECTURE.md` and
   `crates/loom_tool_registry/src/framework.rs`.

The Phase 67 record explicitly supersedes the original six framework IDs with
`process`, `cloud_api`, `mcp`, and `workflow`. Therefore this audit treats the
four-package framework catalog as the required end state. It does not restore
the obsolete `cli_wrapper`, `script`, or `python_art` IDs.

## Plan task matrix

| Plan task | Current result | Evidence and audit decision |
| --- | --- | --- |
| 1. Source boundary guards | Complete | `Test-ArtPluginBoundaryContract.ps1` passes and now checks all six official sample IDs against Hook production source. |
| 2. Package manifests and default-empty state | Complete | Four current framework manifests pass `Test-ArtFrameworkPackageContract.ps1`; fresh state remains package-backed and default-empty. |
| 3. Framework lifecycle | Complete | Install, enable/disable, upgrade, rollback, uninstall, retention, journal recovery, tombstone recovery, trust, and tamper tests pass. |
| 4. Generic framework process protocol | Complete after follow-up | The common `loom_process` launch path now covers deep Windows executable and working-directory paths. Framework request, error, timeout, image-path, and scoped credential tests pass. |
| 5. Independent framework packages | Complete under the superseding architecture | `process`, `cloud_api`, `mcp`, and `workflow` build as independent ZIPs. Restoring the three obsolete IDs would contradict the current architecture. |
| 6. Six independent sample Arts | Complete after follow-up | `image-compress`, `remove-bg`, `image-search`, `color-transfer`, `image-blend`, and `image-blend-compress` all have independent sources and ZIPs. The workflow package has no local blend/compress runtime. |
| 7. Hook capability-driven rendering | Complete | The boundary contract forbids all six sample IDs in Hook production source; execution and rendering remain manifest/capability driven. |
| 8. End-to-end plugin boundary | Complete | Existing third-party no-source-change smoke remains in the release verifier; current six-package install/execution smoke also passes. |
| 9. Documentation and release leakage guards | Complete after follow-up | README counts, Phase 67/68 follow-ups, release catalog, verifier, source contract, and package contract all agree on four frameworks and six sample Arts. |
| 10. Formal releases | Complete when R10 verification is green | Hook's combined clean R9 remains the current Hook artifact because this audit changes no Hook source. Loom's clean candidate is `release/Loom/20260812-art-framework-refactor-audit-clean-r10`. |

## Gaps found and closed

### 1. Official sample catalog had regressed from six packages to four

The authoritative plan and Phase 67 acceptance checklist require six sample Art
packages, but current source/build/release contracts contained only four. The
audit restored:

- `custom-image-blend-script` as a `process` Art;
- `custom-image-blend-compress-workflow` as a declarative `workflow` Art.

The workflow package declares and invokes the exact child Arts
`custom-image-blend-script` and `custom-1770146354922`. Its ZIP contains
`workflow.yaml` and intentionally contains no `art.runtime.json`,
`Blend-Bitmaps`, or compression implementation.

The source builder, source/ZIP contract, direct runtime smoke, installed
execution smoke, formal release catalog, formal verifier, and Hook sample-ID
boundary now cover all six packages.

### 2. Packaged workflow definitions were not installed

Desktop catalog bootstrap calls the generic `/v1/arts/install` endpoint. Before
this audit, installing a Tool with `execution.type=workflow` did not register its
packaged `workflow.yaml`, so the Art could be listed but not executed without a
separate manual workflow PUT.

The daemon now resolves the active immutable Art directory, requires a contained
root `workflow.yaml`, validates it through `WorkflowStore::save_workflow`, and
synchronizes it for direct/catalog install, store install, exact-version update,
rollback, and auto-update. Uninstall does not garbage-collect workflows because
automatic orphan GC remains an explicit Phase 68 non-goal.

### 3. Immutable package resolution dropped credential bindings

Immediately before execution, the daemon resolves the exact version/digest from
the immutable package. That operation previously preserved only the registered
`enabled` flag and discarded `artUserSettings`, so a correctly saved MCP secret
binding disappeared before the framework broker created credential grants.

The resolved immutable Tool now overlays only the mutable registered
`artUserSettings`. Package paths, digests, lockfiles, manifests, and runtime
payloads remain sourced from and verified against the immutable package.

### 4. Deep Windows framework paths failed at process spawn

A clean verifier worktree produced a framework executable/working-directory path
longer than 260 characters and failed with Windows error 267. Canonical `\\?\`
paths are not sufficient for the `CreateProcessW` current-directory parameter.

`loom_process` now builds both managed and one-shot commands through one helper.
For deep absolute Windows paths it canonicalizes the target, obtains the
existing DOS short path, removes any verbatim prefix, and supplies the resulting
ordinary path to `Command`. The regression copies `cmd.exe` into a directory
longer than 260 characters and proves both deep executable and deep working
directory launch successfully.

## Fresh verification matrix

| Gate | Result |
| --- | --- |
| Rust formatting (`cargo fmt`, root and runtime host) | Passed |
| `loom_process` | 5 passed |
| `loom_tool_registry` | 120 passed |
| `loom_workflow_runtime` | 17 passed |
| framework runtime host | 4 passed |
| `loom-daemon` | 220 passed, plus 8 daemon CLI contract tests |
| Art plugin boundary contract | Passed |
| Four-framework source/ZIP contract | Passed |
| Six-sample-Art source/ZIP contract | Passed |
| Six direct sample runtime execution cases | Passed |
| Six installed sample Art executions | Passed |
| Authored cloud/MCP/process/workflow Art creation and execution | Passed |
| Art Store global ID publish/install flow | Passed |
| Malicious archive/signature/dependency/network/process/lifecycle cases | Passed |
| Standalone release and release-tamper contracts | Passed |
| Formal clean R10 release verifier and release smokes | Required before closure claim |

## Explicitly retained non-goals

The following are not incomplete development targets because Phase 68 declares
them as limits or non-goals:

- automatic child/workflow orphan reference counting and garbage collection;
- AppContainer, restricted-token, namespace, seccomp, or VM isolation;
- OS-level denial of arbitrary direct access while permission mode is `audit`;
- an OS keyring for the Unix credential fallback;
- hosted marketplace operation, payment/licensing, and remote publisher
  governance.

## Release targets

- Loom: `release/Loom/20260812-art-framework-refactor-audit-clean-r10`
- Hook: `release/Hook/20260811-distributed-art-surface-clean-r9` (unchanged;
  this audit modifies no Hook source)

The R10 candidate must record `gitDirty=false` and `sourceGitDirty=false`,
contain four framework ZIPs and six sample Art ZIPs, and pass the full formal
verifier before this audit is reported as complete.
