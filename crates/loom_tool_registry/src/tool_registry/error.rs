//! Registry error contracts.

use super::*;

#[derive(Debug, Error)]
pub enum ToolRegistryError {
    #[error("invalid tool definition `{id}`: {reason}")]
    InvalidToolDefinition { id: String, reason: String },
    #[error("tool `{id}` is disabled")]
    ExecutionRejected { id: String },
    #[error("tool `{id}` execution was cancelled")]
    ExecutionCancelled { id: String },
    #[error("tool `{id}` parameter binding failed: {reason}")]
    ParameterBinding { id: String, reason: String },
    #[error("tool id `{id}` is ambiguous; use a publisher-qualified id")]
    AmbiguousToolId { id: String },
    #[error("tool `{id}` execution type `{execution_type}` is not supported by this runtime")]
    UnsupportedExecution {
        id: String,
        execution_type: &'static str,
    },
    #[error("MCP server `{server_id}` for tool `{tool_id}` was not found or is disabled")]
    MissingMcpServer { tool_id: String, server_id: String },
    #[error("MCP execution failed: {0}")]
    Mcp(#[from] loom_mcp::McpError),
    #[error("MCP dependency `{server_id}` for tool `{tool_id}` failed [{code}]: {reason}")]
    McpDependency {
        tool_id: String,
        server_id: String,
        code: String,
        reason: String,
    },
    #[error("cloud API method `{method}` for tool `{id}` is not supported")]
    CloudInvalidMethod { id: String, method: String },
    #[error("cloud API request to `{endpoint}` for tool `{id}` failed: {source}")]
    CloudRequest {
        id: String,
        endpoint: String,
        source: reqwest::Error,
    },
    #[error("cloud API endpoint `{endpoint}` for tool `{id}` violates network policy: {reason}")]
    CloudSecurity {
        id: String,
        endpoint: String,
        reason: String,
    },
    #[error("cloud API request to `{endpoint}` for tool `{id}` returned HTTP {status}: {body}")]
    CloudHttpStatus {
        id: String,
        endpoint: String,
        status: u16,
        body: String,
    },
    #[error("cloud API response from `{endpoint}` for tool `{id}` returned invalid JSON: {source}; body: {body}")]
    CloudJson {
        id: String,
        endpoint: String,
        source: serde_json::Error,
        body: String,
    },
    #[error("cloud API `{field}` template for tool `{id}` is invalid: {reason}")]
    CloudTemplate {
        id: String,
        field: &'static str,
        reason: String,
    },
    #[error("framework package `{framework}` for tool `{id}` was not found: {path}")]
    FrameworkPackageNotFound {
        id: String,
        framework: String,
        path: String,
    },
    #[error("framework Art directory for tool `{id}` was not found: {path}")]
    FrameworkArtDirectoryNotFound { id: String, path: String },
    #[error("framework `{framework}` for tool `{id}` failed to spawn: {reason}")]
    FrameworkProcessSpawn {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` timed out after {timeout_ms}ms")]
    FrameworkProcessTimeout {
        id: String,
        framework: String,
        timeout_ms: u128,
    },
    #[error("framework `{framework}` for tool `{id}` process I/O failed: {reason}")]
    FrameworkProcessIo {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` returned invalid protocol data: {reason}")]
    FrameworkProcessProtocol {
        id: String,
        framework: String,
        reason: String,
    },
    #[error("framework `{framework}` for tool `{id}` failed [{code}]: {message}{detail}")]
    FrameworkProcessFailed {
        id: String,
        framework: String,
        code: String,
        message: String,
        detail: String,
    },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Art settings error: {0}")]
    ArtSettings(#[from] art_settings::ArtSettingsError),
}

pub type ToolRegistryResult<T> = Result<T, ToolRegistryError>;
