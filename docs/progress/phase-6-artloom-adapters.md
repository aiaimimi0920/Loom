# Phase 6: ArtLoom Migration Adapters

## Tasks

- [x] T6.1 Add ArtLoom workflow converter.
  - Acceptance: selected ArtLoom YAML fixtures convert to Loom workflow fixtures;
    converted samples validate and run.
- [x] T6.2 Port ArtLoom smoke patterns.
  - Acceptance: success DAG and mixed-failure DAG smokes exist without requiring
    ArtHook or desktop UI.

## Notes

Do this after native Loom workflow contracts are stable.

## Validation

- `cargo test --manifest-path Loom/Cargo.toml -p loom_workflow --test artloom_conversion`

## Artifacts

- `Loom/crates/loom_workflow/src/artloom.rs`
- `Loom/crates/loom_workflow/tests/artloom_conversion.rs`
- `Loom/examples/artloom/success-dag.yaml`
- `Loom/examples/artloom/mixed-failure-dag.yaml`
