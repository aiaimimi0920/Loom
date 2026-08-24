//! Errors reported by package intake, trust, persistence, and integrity checks.

use super::*;

#[derive(Debug, Error)]
pub enum McpPackageError {
    #[error("MCP server package exceeds {MAX_MCP_SERVER_PACKAGE_BYTES} bytes")]
    PackageTooLarge,
    #[error("MCP server package archive is invalid: {0}")]
    InvalidArchive(String),
    #[error("MCP server package path is unsafe: {0}")]
    UnsafePath(String),
    #[error("MCP server package manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("MCP server package entry is missing: {0}")]
    MissingEntry(String),
    #[error("MCP server package integrity check failed: {0}")]
    Integrity(String),
    #[error("MCP server package trust check failed: {0}")]
    Trust(String),
    #[error("MCP server package IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP server package JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}
