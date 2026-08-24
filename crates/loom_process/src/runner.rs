//! Bounded one-shot process execution and concurrent pipe capture.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use loom_protocol::ExecutionDiagnostics;

use crate::command::supervised_command;
use crate::error::ProcessError;
use crate::isolation::ProcessIsolation;
use crate::model::{ProcessSpec, SupervisedOutput};

pub fn run_with_input(spec: &ProcessSpec, input: &[u8]) -> Result<SupervisedOutput, ProcessError> {
    run_with_input_internal(spec, input, None)
}

pub fn run_with_input_cancellable(
    spec: &ProcessSpec,
    input: &[u8],
    cancellation: &AtomicBool,
) -> Result<SupervisedOutput, ProcessError> {
    run_with_input_internal(spec, input, Some(cancellation))
}

fn run_with_input_internal(
    spec: &ProcessSpec,
    input: &[u8],
    cancellation: Option<&AtomicBool>,
) -> Result<SupervisedOutput, ProcessError> {
    let started = Instant::now();
    let mut command = supervised_command(spec);

    let mut child = command.spawn().map_err(ProcessError::Spawn)?;
    let isolation = ProcessIsolation::attach(&child, &spec.limits).map_err(|error| {
        let _ = child.kill();
        let _ = child.wait();
        ProcessError::Isolation(error)
    })?;

    let output_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_reader = child.stdout.take().map(|stdout| {
        spawn_bounded_reader(
            stdout,
            spec.limits.stdout_bytes,
            Arc::clone(&output_exceeded),
        )
    });
    let stderr_reader = child.stderr.take().map(|stderr| {
        spawn_bounded_reader(
            stderr,
            spec.limits.stderr_bytes,
            Arc::clone(&output_exceeded),
        )
    });

    let mut stdin_writer = child.stdin.take().map(|mut stdin| {
        let input = input.to_vec();
        thread::spawn(move || stdin.write_all(&input))
    });

    let deadline = started.checked_add(spec.limits.timeout);
    let status = loop {
        if stdin_writer
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            if let Err(error) = join_stdin_writer(stdin_writer.take()) {
                isolation.kill_tree(&mut child);
                let _ = child.wait();
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(error);
            }
        }
        if output_exceeded.load(Ordering::Relaxed) {
            isolation.kill_tree(&mut child);
            let _ = child.wait();
            let _ = join_stdin_writer(stdin_writer.take());
            let stdout = join_reader(stdout_reader)?;
            let stderr = join_reader(stderr_reader)?;
            return Err(ProcessError::OutputLimit {
                diagnostics: diagnostics(started, None, &stdout, &stderr, false, true),
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            });
        }
        if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
            isolation.kill_tree(&mut child);
            let _ = child.wait();
            let _ = join_stdin_writer(stdin_writer.take());
            let stdout = join_reader(stdout_reader)?;
            let stderr = join_reader(stderr_reader)?;
            return Err(ProcessError::Cancelled {
                diagnostics: diagnostics(started, None, &stdout, &stderr, false, false),
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            });
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // A detached descendant can inherit the pipes after the leader exits. Terminating
                // the isolation group closes those handles before the reader/writer joins below.
                isolation.kill_tree(&mut child);
                break status;
            }
            Ok(None) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                isolation.kill_tree(&mut child);
                let _ = child.wait();
                let _ = join_stdin_writer(stdin_writer.take());
                let stdout = join_reader(stdout_reader)?;
                let stderr = join_reader(stderr_reader)?;
                return Err(ProcessError::Timeout {
                    diagnostics: diagnostics(started, None, &stdout, &stderr, true, false),
                    stdout: stdout.bytes,
                    stderr: stderr.bytes,
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                isolation.kill_tree(&mut child);
                let _ = child.wait();
                let _ = join_stdin_writer(stdin_writer.take());
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(ProcessError::Wait(error));
            }
        }
    };
    join_stdin_writer(stdin_writer.take())?;
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    // Read the counter while the isolation group is still open; it is destroyed when `isolation`
    // drops at the end of this function.
    let peak_memory_bytes = isolation.peak_memory_bytes();
    Ok(SupervisedOutput {
        diagnostics: diagnostics(started, status.code(), &stdout, &stderr, false, false),
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        peak_memory_bytes,
    })
}

fn join_stdin_writer(
    writer: Option<thread::JoinHandle<std::io::Result<()>>>,
) -> Result<(), ProcessError> {
    let Some(writer) = writer else {
        return Ok(());
    };
    writer
        .join()
        .map_err(|_| ProcessError::Stdin(std::io::Error::other("stdin writer panicked")))?
        .map_err(ProcessError::Stdin)
}

#[derive(Debug)]
struct BoundedCapture {
    bytes: Vec<u8>,
    total_bytes: u64,
    truncated: bool,
}

fn spawn_bounded_reader(
    mut reader: impl Read + Send + 'static,
    limit: usize,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<std::io::Result<BoundedCapture>> {
    thread::spawn(move || {
        let mut retained = Vec::with_capacity(limit.min(64 * 1024));
        let mut total = 0u64;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            let remaining = limit.saturating_sub(retained.len());
            retained.extend_from_slice(&buffer[..read.min(remaining)]);
            if total > limit as u64 {
                exceeded.store(true, Ordering::Relaxed);
            }
        }
        Ok(BoundedCapture {
            bytes: retained,
            total_bytes: total,
            truncated: total > limit as u64,
        })
    })
}

fn join_reader(
    reader: Option<thread::JoinHandle<std::io::Result<BoundedCapture>>>,
) -> Result<BoundedCapture, ProcessError> {
    let Some(reader) = reader else {
        return Ok(BoundedCapture {
            bytes: Vec::new(),
            total_bytes: 0,
            truncated: false,
        });
    };
    reader
        .join()
        .map_err(|_| ProcessError::Reader("output reader panicked".to_owned()))?
        .map_err(|error| ProcessError::Reader(error.to_string()))
}

fn diagnostics(
    started: Instant,
    exit_code: Option<i32>,
    stdout: &BoundedCapture,
    stderr: &BoundedCapture,
    timed_out: bool,
    resource_limited: bool,
) -> ExecutionDiagnostics {
    ExecutionDiagnostics {
        duration_ms: Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64),
        exit_code,
        stdout_bytes: stdout.total_bytes,
        stderr_bytes: stderr.total_bytes,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        timed_out,
        resource_limited,
    }
}
