//! Public workflow runtime failure contract.

use super::*;

#[derive(Debug, Error)]
pub enum WorkflowRuntimeError {
    #[error("tool registry error: {0}")]
    ToolRegistry(#[from] ToolRegistryError),
    #[error("workflow store error: {0}")]
    WorkflowStore(#[from] WorkflowStoreError),
    #[error("workflow `{workflow_id}` is invalid: {source}")]
    WorkflowYaml {
        workflow_id: String,
        source: serde_yaml::Error,
    },
    #[error("workflow `{workflow_id}` child tool `{tool_id}` was not found")]
    ChildToolNotFound {
        workflow_id: String,
        tool_id: String,
    },
    #[error("workflow `{workflow_id}` contains unresolved dependencies or a cycle")]
    UnresolvedDependencies { workflow_id: String },
    #[error("workflow `{workflow_id}` node `{node_id}` requires image input")]
    MissingImageInput {
        workflow_id: String,
        node_id: String,
    },
    #[error("workflow `{workflow_id}` native node `{node_id}` failed: {message}")]
    NativeFailed {
        workflow_id: String,
        node_id: String,
        message: String,
    },
    #[error("workflow `{workflow_id}` preview policy is invalid: {reason}")]
    InvalidPreviewPolicy { workflow_id: String, reason: String },
    #[error("workflow `{workflow_id}` is invalid: {reason}")]
    InvalidWorkflow { workflow_id: String, reason: String },
    #[error("workflow `{workflow_id}` exceeded its resource budget: {reason}")]
    ResourceLimit { workflow_id: String, reason: String },
    #[error("workflow execution exceeded its caller-owned timeout")]
    Timeout,
    #[error("workflow execution was cancelled")]
    Cancelled,
}

pub type WorkflowRuntimeResult<T> = Result<T, WorkflowRuntimeError>;
