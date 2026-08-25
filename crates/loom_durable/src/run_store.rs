use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

mod core;
mod memory;
mod sqlite;
mod sqlite_path;

pub use memory::InMemoryRunEvidenceStore;
pub use sqlite::SqliteRunEvidenceStore;

pub type RunStoreResult<T> = Result<T, RunStoreError>;

#[derive(Debug, Error)]
pub enum RunStoreError {
    #[error("invalid run evidence: {0}")]
    InvalidRun(String),
    #[error("invalid run event: {0}")]
    InvalidEvent(String),
    #[error("run `{0}` already exists")]
    DuplicateRun(String),
    #[error("run `{0}` was not found")]
    RunNotFound(String),
    #[error("run store JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("run store IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("run store schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: i32, supported: i32 },
    #[error("run store integrity check failed: {0}")]
    Integrity(String),
    #[error("SQLite run store error: {0}")]
    Sqlite(String),
    #[error("SQLite run store locking protocol error: {0}")]
    SqliteLockProtocol(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunEventDraft {
    pub kind: String,
    pub fields: Map<String, Value>,
}

impl RunEventDraft {
    pub fn new(kind: impl Into<String>, fields: Value) -> RunStoreResult<Self> {
        let kind = kind.into();
        if kind.trim().is_empty() {
            return Err(RunStoreError::InvalidEvent("kind is required".to_owned()));
        }
        let fields = fields
            .as_object()
            .cloned()
            .ok_or_else(|| RunStoreError::InvalidEvent("fields must be an object".to_owned()))?;
        Ok(Self { kind, fields })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStoreStatus {
    pub mode: &'static str,
    pub persistent: bool,
}

pub trait RunEvidenceStore: Send {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()>;
    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()>;
    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>>;
    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>>;
    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize>;
    fn status(&self) -> RunStoreStatus;
}

pub const RUN_STORE_SCHEMA_VERSION: i32 = 1;

#[cfg(test)]
mod tests;
