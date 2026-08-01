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

## ArtLoom conversion contract

`loom_workflow::artloom::convert_artloom_yaml(workflow_id, yaml)` accepts the
selected ArtLoom YAML subset:

```yaml
name: ArtLoom Success DAG
description: Optional text
nodes:
  - id: root
    uses: planner
  - id: draft
    uses: writer
    needs:
      - root
    with:
      input: ${{ nodes.root.outputs.output }}
```

Conversion behavior:

- `name` and `description` are preserved on `ConvertedArtLoomWorkflow`.
- `workflow_id` becomes the native `WorkflowGraph.id`.
- `nodes[].id` becomes the native node id.
- `nodes[].uses` becomes the native `ActorId`.
- `nodes[].needs` becomes native edges.
- ArtLoom output references in `with` recursively add native edges.
- duplicate edges are de-duplicated.
- the converted graph is validated before being returned.

The converter intentionally ignores ArtLoom canvas metadata, visual state,
desktop IPC, ArtHook behavior, and embedded Python runtime concerns.

## Workflow Art package dependencies

A packaged workflow Art may declare child Arts under
`metadata.dependencies.arts`. This package-install contract is separate from
the native `WorkflowGraph` execution contract above:

1. Loom installs the root Art through the normal secure ZIP and framework
   readiness path.
2. Child Art IDs are fetched and installed breadth-first.
3. A visited set suppresses duplicate work and terminates dependency cycles.
4. A child already present in the tool registry is retained rather than copied
   into the parent package or reinstalled implicitly.
5. Every child is its own immutable package with its own activation pointer,
   canonical digest, framework lockfile, writable state/cache/output roots, and
   execution-time integrity verification.

The parent workflow package never receives child source code and neither Loom
nor Hook source is modified. The current v1 parent lockfile does **not** pin a
child Art version/digest, and uninstall does not maintain dependency reference
counts or automatically garbage-collect orphan child Arts. Operators must
upgrade, rollback, or uninstall those independently installed child packages
explicitly until a future Art dependency lock/GC contract is introduced.
