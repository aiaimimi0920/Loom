//! One-shot execution controls also apply while trusted isolation is being prepared.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use loom_protocol::ExecutionDiagnostics;

use crate::ProcessError;

#[derive(Default)]
pub(crate) struct LaunchControl<'a> {
    pub(crate) started: Option<Instant>,
    pub(crate) deadline: Option<Instant>,
    pub(crate) cancellation: Option<&'a AtomicBool>,
}

impl LaunchControl<'_> {
    pub(crate) fn check(&self) -> Result<(), ProcessError> {
        let cancelled = self
            .cancellation
            .is_some_and(|flag| flag.load(Ordering::Acquire));
        let timed_out = self.deadline.is_some_and(|limit| Instant::now() >= limit);
        if !cancelled && !timed_out {
            return Ok(());
        }
        let diagnostics = ExecutionDiagnostics {
            duration_ms: self
                .started
                .map(|start| start.elapsed().as_millis().min(u64::MAX as u128) as u64),
            timed_out: !cancelled,
            ..ExecutionDiagnostics::default()
        };
        if cancelled {
            Err(ProcessError::Cancelled {
                stdout: Vec::new(),
                stderr: Vec::new(),
                diagnostics,
            })
        } else {
            Err(ProcessError::Timeout {
                stdout: Vec::new(),
                stderr: Vec::new(),
                diagnostics,
            })
        }
    }
}
