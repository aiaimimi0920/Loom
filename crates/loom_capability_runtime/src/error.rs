use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CapabilityHostError {
    #[error("capability runtime package is invalid: {0}")]
    InvalidPackage(String),
    #[error("capability runtime protocol failed: {0}")]
    Protocol(String),
    #[error("capability runtime is unavailable: {0}")]
    Unavailable(String),
    #[error("capability runtime request timed out")]
    Timeout,
    #[error("capability runtime queue is full")]
    Busy,
    #[error("capability contribution is not registered: {0}")]
    NotFound(String),
    #[error("capability runtime I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("capability process failed: {0}")]
    Process(#[from] loom_process::ProcessError),
    #[error("capability JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub type HostResult<T> = Result<T, CapabilityHostError>;
