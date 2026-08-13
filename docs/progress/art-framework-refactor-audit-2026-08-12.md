# Art framework refactor completion audit (2026-08-12)

> Historical Phase 70 closure audit. The current canonical-only result and
> release/acceptance evidence are recorded in
> `phase-71-art-canonical-layout-legacy-zero.md`. R12/R21 below are immutable
> historical evidence, not the current acceptance defaults.

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

## Protocol and phase status (closed 2026-08-13)

- **Phases 67-69: complete.** Phase 67 established independent pluginized Art
  frameworks; Phase 68 hardened package lifecycle, trust, permissions, and
  release boundaries; Phase 69 closed phase-one Distributed Art Surface
  delivery and its clean Loom<->Hook runtime gate.
- **`loom.hook.v1`: standardized contract.** The public protocol now names
  `loom.hook.v1` as the only Loom<->Hook Art bridge. Distributed Surface payloads
  use `loom.surface.v1`; every production Surface push is carried on an exact
  `loom.surface.*` event name. Loom remains the package/execution/resource
  authority and Hook remains the capability-driven renderer and interaction
  client.
- **Phase 70: complete.** The old Loom<->Hook Art protocol, production routes,
  action names, response aliases, ArtLoom workflow converter, and Hook-local Art
  execution fallbacks have been removed. The closure record is
  `docs/progress/phase-70-loom-hook-v1-legacy-retirement.md`.
- **Superseded boundary-only result.** Phase 70 reduced the Loom<->Hook Art wire
  compatibility layer to zero. Phase 71 subsequently removed the persisted
  data, package-layout, provider/process, and Hook app-data compatibility paths
  that were still outside this audit's original scope. The
  bridge accepts only `loom.hook.v1` methods/events and the formal
  `loom.surface.*` Surface event set. Old `surface`, `surface/snapshot`,
  ArtLoom/AHRP, and direct per-Art names are not accepted as production wire
  aliases. Hook's `surface/...` Tauri events are a private native-to-frontend
  translation after a formal wire event is received. WGC-to-GDI capture remains
  only as a current runtime reliability path.

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
| 10. Formal releases | Complete | Loom R21 and Hook R12 were built independently. R21 passes the complete 49-entry packaged verifier and all seven smokes, and its Plugin SDK contains the final protocol README byte-for-byte. The exact R12/R21 pair passes the 600-second native dual-end acceptance, including pairing, Surface attach/event/resource/formal result, clean restart recovery, and final cleanup. |

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

## Verification matrix

| Gate | Result |
| --- | --- |
| Rust formatting (`cargo fmt`, Loom and Hook) | Passed |
| `loom_protocol` | 21 passed |
| `loom_shared_image` | 4 passed |
| `loom_tool_registry` | 120 passed |
| `loom_workflow` | 6 passed |
| `loom-daemon --lib` | 198 passed |
| Loom desktop | 141 passed; TypeScript typecheck and production build passed |
| Hook Rust library | 223 passed; `cargo check` and formatting passed |
| Hook frontend | Historical full gate: 252 files / 1046 tests and production TypeScript typecheck passed; fresh acceptance/default/SHA contract gate: 2 files / 18 tests passed |
| Art plugin boundary contract | Passed |
| Third-party plugin boundary runtime smoke | Passed |
| Four-framework / six-sample-Art / Hook formal execution smoke | Passed: 6 formal `loom.hook.v1` executions; MCP shared memory and generic candidate metadata verified |
| Rebuilt Loom desktop artifact legacy scan | Passed: no old ArtLoom/AHRP protocol routes or symbols |
| Production source legacy scan | Passed: no old Loom<->Hook Art protocol runtime entry remains |

## Explicitly retained non-goals

The following are not incomplete development targets because Phase 68 declares
them as limits or non-goals:

- automatic child/workflow orphan reference counting and garbage collection;
- AppContainer, restricted-token, namespace, seccomp, or VM isolation;
- OS-level denial of arbitrary direct access while permission mode is `audit`;
- an OS keyring for the Unix credential fallback;
- hosted marketplace operation, payment/licensing, and remote publisher
  governance.

## Historical Phase 70 releases

- Loom: `release/Loom/20260813-loom-hook-v1-surface-wire-r21`
- Hook: `release/Hook/20260813-loom-hook-v1-surface-wire-r12`

Artifact evidence:

- R21 `Loom.exe`: 10,069,504 bytes, SHA-256
  `b61245ec646fae56a9af745c6d6a2fa4e3ead5482e791ec7267bf9e2390029cf`.
- R21 `runtime/loom-daemon.exe`: 26,536,448 bytes, SHA-256
  `31746e1376415f2599c96f572e3984afeab33f7cbb8ccaa3a39e63bb0e53d82a`.
- R21 manifest: 49 checksum entries, including the required desktop, CLI, and
  Plugin SDK ZIPs, four framework ZIPs, and six sample Art ZIPs.
- R21 Plugin SDK ZIP: 715,318 bytes, SHA-256
  `9b1cb8529fb95c44db65307775a7b109516d0ab94b799e3826e63039699be221`.
  Its `protocol/README.md` is byte-identical to the final source README (15,032
  bytes), closing the documentation drift found in R20.
- The release verifier now independently compares the SDK ZIP's protocol README
  byte count and SHA-256 with the release source. Its integrity-tamper regression
  rebuilds internally consistent manifest/checksum metadata around a mismatched
  README and proves that such a candidate is rejected.
- In a direct A/B check, the hardened verifier rejects R20 because its embedded
  README is 14,031 bytes while the final source is 15,032 bytes. R21 passes the
  same hardened 49-file verifier and all seven packaged smokes.
- `verify-release.ps1 -RunSmoke` passed all seven packaged smokes: standalone,
  Hook canvas, Hook error preview, Framework Art Store Hook, third-party plugin
  boundary, Surface prototypes, and authored Art creation.
- R12 `hook.exe`: 7,021,568 bytes, SHA-256
  `9c0699eb73eecb86002e9a44ec2cb5cba8f9e123d6b7a215bcb763b1158fa7d8`.

Final native evidence is
`Hook/artifacts/r12-r21-full-600s-retry/summary.json`, run
`20260813-080542-hook-loom-surface-7f3696e112be`, with outer and inner
`status = passed`. It verified the exact R12/R21 paths and digests, native WebView2/CDP
startup, main/watchdog and single-instance behavior, a `loom.hook.v1`
subscriber, canonical workflow identity
`neuro.official/surface-device-dashboard`, pending-to-approved device pairing
with `sessionEpoch = 1`, packaged Surface attach and click event, resource
resolution, a ready formal result, clean exit, restart, persistent recovery of
the same Surface instance, and another successful interaction. The 600-second
soak sampled 365 times: private bytes grew from 153,739,264 to 154,144,768
(405,504 bytes, 0.264%), peaked at 158,801,920, and recorded no violation.
Restart advanced the Surface revision from 5 to 8; authoritative state was
`ready`, pending events were empty, and the event acknowledgement succeeded.
Both Hook exits returned code 0, and no forced cleanup was needed. Final cleanup
stopped the isolated daemon and Art Store and left the store, daemon, and bridge
listener sets empty.

The preserved first R12/R20 full-run evidence,
`Hook/artifacts/r12-r20-full-600s/summary.json`, is rejected rather than
overwritten. Its initial pairing, Surface operation, and soak passed, but host
input generated `rdev_delete_triggered`/`trigger-delete-listener`; Hook correctly
sent a `disposed` lifecycle and persisted an empty session, so the restart probe
could not find that intentionally deleted Surface. The clean retry above is the
final acceptance evidence.

R14 was rejected by the formal verifier because two release smoke fixtures still
wrote the Hook session to the retired `com.vmjcv.arthook-next` directory. The
candidate binaries correctly read the current Hook identifier
`com.yamiyu.hook`; the smoke fixtures and their edge port names were corrected
before producing R15. R15 was then rejected by the Surface prototype gate: its
process-backed actions retained 2-second deadlines even though the standardized
framework path starts both the process framework host and the packaged Art
runtime. The failure evidence reported `surface_action_timeout`; action budgets
and their release contract were corrected before producing R16. R16 was rejected
before smoke because it was mistakenly built with `-NoZip`, omitting the required
CLI and Plugin SDK artifacts. R17 was the first complete package after those
fixes, but predates the final exact `loom.surface.*` wire contract. R18 retained
an obsolete Surface smoke fixture and failed; R19 was mistakenly built with
`-NoZip`; R20 is the first complete final-wire Loom package. R21 rebuilds that
same accepted daemon with the final protocol documentation embedded in the
Plugin SDK and is the final release. Hook R10 was the
blocked preflight candidate. R11 reached native pairing but the acceptance
workflow used the short local store ID `surface-device-dashboard`, which could
not match Hook's exact publisher-qualified catalog identity; the fixture was
corrected without adding a short-ID alias, and R12 was rebuilt as the final Hook
package. The first R12/R21 preflight at
`Hook/artifacts/r12-r21-full-600s/summary.json` was rejected before launch
because the command omitted the wrapper's explicit expected-daemon SHA override;
cleanup passed. The retry supplied the independently computed exact R12/R21
hashes and is the final evidence above. R13 and Hook clean R9 remain immutable historical Phases 67-69 runtime
baselines and were restored after candidate acceptance; they are not the final
Phase 70 release pair.

For this historical Phase 70 run, both acceptance entry points were hardened to
default to R12/R21 and their exact hashes. Those defaults were superseded by the
Phase 71 R14/R23 canonical-only pair; see
`phase-71-art-canonical-layout-legacy-zero.md` for current hashes and native
evidence. SHA parameters still require 64 hexadecimal characters and are always
compared; the outer runner always forwards the Hook hash to the native runner.
