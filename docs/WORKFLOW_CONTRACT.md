# Loom Workflow Contract

Loom stores and executes Art and Otter workflows through `loom_workflow_store` and `loom_workflow_runtime`.

## Otter workflow synchronization

Otter sends its live graph through the typed `loom.hook.workflow.sync` contract. Loom validates and persists the resulting graph using the same native workflow store backing packaged workflow Arts. Parameter updates use `loom.hook.workflow.node.update`, and desktop instantiation uses `loom.hook.workflow.instantiate`. Historical ArtLoom YAML converters are retired and are not accepted as import or runtime paths.

## Dynamic execution placement and intra-workflow hybrid relay

Workflow nodes support dynamic execution placement across heterogeneous Loom and Otter environments:

- Nodes declare execution capabilities and placement requirements (`local`, `server`, or `auto`).
- When a workflow spans server-side compute nodes (such as AI Gateway, MCP, or Cloud API steps) and terminal-bound nodes (such as local screen capture, desktop UI automation, or local-only Otter Arts), Loom coordinates cross-end step execution through the authenticated terminal relay (`/v1/otter/relay/*` in `apps/daemon/src/terminal_relay.rs`).
- Relay tasks bind to an approved, active device session, enforce per-device queue and payload bounds, preserve topological data dependencies (`needs` and `${{ nodes.<id>.outputs.<port> }}`), and fail closed when the required target terminal goes offline or exceeds its execution deadline.

## Workflow Art package dependencies and SemVer compatibility

A packaged workflow Art may declare child Arts under `metadata.dependencies.arts`. This package-install contract is separate from the dynamic `WorkflowGraph` execution contract above:

1. Loom inspects the dependency graph before activating the parent, then installs missing children dependency-first through the verified ZIP installer.
2. Dependency cycles and fetched packages whose canonical publisher-qualified identity does not match the requested child reference are rejected.
3. Existing installed child packages are retained rather than copied into the parent package, and their integrity is verified before locking the parent.
4. Each parent lockfile records direct children as `kind: "art"` with their publisher-qualified ID, SemVer requirement (when declared), locked version, and canonical SHA-256 digest.
5. Every child remains an independent immutable package with its own activation pointer, canonical digest, framework lockfile, writable `state/`/`cache/`/`outputs/` directories, and pre-execution integrity checks.
6. Parent readiness, execution, and rollback recursively verify the child dependency graph. When a child Art is upgraded within a declared compatible SemVer range and passes publisher trust, signature, and canonical digest verification, Loom supports explicit compatible child rebinding; incompatible version changes, tampering, revocation, or child uninstallation fail closed until a valid compatible child version is restored or the parent lock is refreshed.

Uninstall never deletes independently installed child packages automatically; operators manage child package lifecycles explicitly.

## Image blend and compress example

The canonical workflow sample is `art-packages/samples/image-blend-compress` (`neuro.official/custom-image-blend-compress-workflow`, workflow ID `image-blend-compress-workflow`). It depends on the `workflow` framework and two separately installed `process` Arts (`neuro.official/custom-image-blend-script` and `neuro.official/custom-1770146354922`) without bundling their code.

- Two image inputs (`input` and `reference`) feed `blend`, parameterized by `mix_ratio` (`0..=100`, default `50`) and `quality_num` (`60..=100`, default `90`, `lossless: false`).
- `compress` declares `needs: [blend]`, consumes `${{ nodes.blend.outputs.output_base64 }}`, and supplies the workflow's `primaryOutput` via `node_result`.
- Missing required inputs or upstream `blend` failures abort the DAG immediately without running `compress` or emitting intermediate outputs as formal results.
- Build sample packages with `scripts/Build-LoomSampleArtPackages.ps1` and install them through the standard package lifecycle.
