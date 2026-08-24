//! Stable process-supervision failure contracts.

use loom_protocol::ExecutionDiagnostics;

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("failed to start process: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("failed to attach process isolation: {0}")]
    Isolation(String),
    #[error("failed to write process stdin: {0}")]
    Stdin(#[source] std::io::Error),
    #[error("failed while waiting for process: {0}")]
    Wait(#[source] std::io::Error),
    #[error("process timed out")]
    Timeout {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        diagnostics: ExecutionDiagnostics,
    },
    #[error("process was cancelled")]
    Cancelled {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        diagnostics: ExecutionDiagnostics,
    },
    #[error("process exceeded stdout/stderr resource limits")]
    OutputLimit {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        diagnostics: ExecutionDiagnostics,
    },
    #[error("process output reader failed: {0}")]
    Reader(String),
    #[error("process did not expose required {0} pipe")]
    MissingPipe(&'static str),
}
