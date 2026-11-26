//! Durable SQLite and in-memory run evidence store contracts for Loom.

pub mod run_store;
pub use run_store::{
    InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreResult,
    RunStoreStatus, SqliteRunEvidenceStore,
};

/// Version of the durable run-evidence store crate.
pub const LOOM_DURABLE_VERSION: &str = env!("CARGO_PKG_VERSION");
