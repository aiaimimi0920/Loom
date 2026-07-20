# Phase 2: Core and Durable Runtime

## Tasks

- [x] T2.1 Implement core primitives.
  - Acceptance: IDs, errors, result type, messages, events, run/session types,
    and serialized DTOs have unit tests.
- [x] T2.2 Implement in-memory event store.
  - Acceptance: append, query by run/session, and ordering tests pass.
- [x] T2.3 Implement actor mesh.
  - Acceptance: actor register, send, and terminate tests pass.

## Notes

No fake durable success APIs. Use in-memory behavior first if SQLite is not ready.

## Validation

- `cargo test --manifest-path Loom/Cargo.toml -p loom_core -p loom_durable`
