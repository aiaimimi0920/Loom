# Loom Workflow Contract

`loom_workflow` defines Loom's v1 workflow graph and deterministic executor.

## Graph

`WorkflowGraph` fields:

- `id: WorkflowId`
- `entry_node: String`
- `nodes: BTreeMap<String, WorkflowNode>`
- `edges: Vec<WorkflowEdge>`

`WorkflowNode` currently supports one action kind:

```yaml
action:
  type: agent
  actor_id: planner
```

The node map key and the node's `id` should match.

## Validation rules

`WorkflowGraph::validate()` rejects:

- a missing `entry_node`;
- an edge whose `from` endpoint does not exist;
- an edge whose `to` endpoint does not exist;
- cycles;
- nodes unreachable from `entry_node`.

## Execution

`WorkflowExecutor` runs the graph in deterministic breadth-first order from the
entry node. For each agent node it looks up a configured `StepOutcome` by
`ActorId`.

The executor records durable events through `loom_durable::EventStore`:

1. `RunStarted`
2. one `ActorMessage` per executed node
3. `RunFinished`

Execution stops at the first failed node and returns a `WorkflowRunSummary`:

- `run_id`
- `status`
- `completed_nodes`
- `failed_nodes`

## Sample workflow fixture

`Loom/examples/workflows/three-node-success.yaml` is the canonical v1 sample.
It defines:

- `start -> draft -> review`
- actor ids: `planner`, `writer`, `reviewer`

The CLI command:

```powershell
.\target\debug\loom.exe run sample.three_node --examples-dir .\examples
```

returns a tab-separated status and completed-node list, for example:

```text
succeeded    start,draft,review
```

## Hook workflow synchronization

Hook sends its live graph through the typed `loom.hook.workflow.sync` contract.
Loom stores and validates the resulting workflow with the same native workflow
store used by packaged workflow Arts. Parameter persistence uses
`loom.hook.workflow.node.update`; instantiation uses
`loom.hook.workflow.instantiate`. The removed ArtLoom YAML converter is not an
accepted import or runtime path.

## Workflow Art package dependencies

A packaged workflow Art may declare child Arts under
`metadata.dependencies.arts`. This package-install contract is separate from
the native `WorkflowGraph` execution contract above:

1. Loom reads the dependency graph without activating the parent, then installs
   missing children dependency-first through the normal secure ZIP path.
2. Cycles and fetched packages whose identity does not match the requested child
   reference are rejected.
3. A child already present in the tool registry is retained rather than copied
   into the parent package or reinstalled implicitly, but its integrity is
   revalidated before the parent can be locked.
4. The parent lockfile records every direct child as `kind: "art"` with the
   child's canonical publisher-qualified ID, exact package version, and
   canonical digest.
5. Every child is its own immutable package with its own activation pointer,
   canonical digest, framework lockfile, writable state/cache/output roots, and
   execution-time integrity verification.
6. Parent readiness, execution, and rollback recursively verify that the child
   graph still matches the exact locks. A child upgrade, rollback, activation
   edit, payload/lock tamper, or uninstall makes the parent not ready until the
   matching child state is restored or the parent is explicitly reinstalled to
   refresh its lock.

The parent workflow package never receives child source code and neither Loom
nor Hook source is modified. Uninstall does not maintain dependency reference
counts or automatically garbage-collect orphan child Arts; operators still
remove independently installed child packages explicitly.

## Image blend and compress example

The canonical example is `art-packages/samples/image-blend-compress`, with Art ID
`neuro.official/custom-image-blend-compress-workflow` and workflow ID
`image-blend-compress-workflow`. It requires the independently installed `workflow`
framework and two separately installed process Arts:
`neuro.official/custom-image-blend-script` and
`neuro.official/custom-1770146354922`. It does not bundle their implementations.

Two image inputs, `input` and `reference`, feed the blend node. `mix_ratio` ranges
from 0 to 100 (default 50); `quality_num` ranges from 60 to 100 (default 90).
Compression uses `lossless: false`; lower quality values permit stronger lossy
compression. Current manifest/YAML values are authoritative, including parameters
not exposed by this Art.

`compress` declares `needs: [blend]` and reads
`${{ nodes.blend.outputs.output_base64 }}` as its input. Manifest bindings map
the two root images and scalar parameters to their child arguments. The
`primaryOutput` uses `node_result` from `compress`, preserving the terminal image
response rather than returning an intermediate blend or stringifying it.

Missing required images and child failures must produce execution failures;
compression must not run after failed blending or report the intermediate image
as success. Package creation uses `scripts/Build-LoomSampleArtPackages.ps1`, and
installation uses the normal package lifecycle. Retired built-in `script`/
`cli_wrapper` frameworks, AHRP calls and one-off resource-tree installers are not
the current implementation. Hook uses the public `loom.hook.v1` execution contract.
