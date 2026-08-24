use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowStoreError {
    #[error("invalid workflow id `{0}`")]
    InvalidWorkflowId(String),
    #[error("workflow `{0}` was not found")]
    NotFound(String),
    #[error("workflow YAML is invalid: {0}")]
    InvalidWorkflowYaml(String),
    #[error("workflow graph is invalid: {0}")]
    InvalidWorkflowGraph(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type WorkflowStoreResult<T> = Result<T, WorkflowStoreError>;
