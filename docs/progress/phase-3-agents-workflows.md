# Phase 3: Agents and Workflows

## Tasks

- [x] T3.1 Implement agent definition loader.
  - Acceptance: markdown/YAML-frontmatter specs load; project/user precedence is
    deterministic; tool permission metadata parses.
- [x] T3.2 Implement workflow graph model.
  - Acceptance: graph validation detects missing entry, missing nodes, disallowed
    cycles, and orphaned edges.
- [x] T3.3 Implement workflow executor.
  - Acceptance: successful 3-node DAG and mixed failure DAG tests pass; durable
    events are recorded.
- [x] T3.4 Implement cognitive orchestration facade.
  - Acceptance: System 1 workflow execution is exposed; System 2 planning trait
    exists; Courtroom/MoA roles map to agent specs.

## Notes

ArtLoom workflow concepts may inform graph shape, but do not copy UI code.

## Validation

- `cargo test --manifest-path Loom/Cargo.toml -p loom_agent -p loom_workflow`
