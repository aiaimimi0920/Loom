//! MCP error and result contracts.

use super::*;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("failed to start MCP process `{command}`: {source}")]
    ProcessStart {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("MCP process did not expose {pipe}")]
    MissingPipe { pipe: &'static str },
    #[error("MCP stdio error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MCP server returned JSON-RPC error: {0}")]
    JsonRpc(JsonValue),
    #[error("MCP protocol error: {0}")]
    Protocol(String),
    #[error("MCP process supervision failed for `{command}`: {reason}")]
    ProcessSupervision { command: String, reason: String },
    #[error("MCP request timed out after {timeout_ms}ms; stderr: {stderr}")]
    Timeout { timeout_ms: u128, stderr: String },
    #[error("MCP response exceeded the {limit} byte message limit")]
    OutputLimit { limit: usize },
    #[error("MCP process exited with code {code:?}; stderr: {stderr}")]
    ProcessExited { code: Option<i32>, stderr: String },
    #[error("MCP server `{server_id}` is disabled")]
    Disabled { server_id: String },
    #[error("invalid MCP configuration: {0}")]
    InvalidConfig(String),
    #[error("MCP server package integrity check failed: {0}")]
    PackageIntegrity(String),
    #[error("MCP transport `{0}` is not supported")]
    UnsupportedTransport(String),
    #[error("MCP HTTP request failed: {0}")]
    Http(String),
    #[error("MCP HTTP endpoint returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("MCP request was cancelled")]
    Cancelled,
}

pub type McpResult<T> = Result<T, McpError>;
