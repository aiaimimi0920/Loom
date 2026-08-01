use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use loom_protocol::ExecutionDiagnostics;

#[derive(Clone, Debug)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub memory_bytes: Option<usize>,
    pub max_processes: Option<u32>,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            stdout_bytes: 8 * 1024 * 1024,
            stderr_bytes: 8 * 1024 * 1024,
            memory_bytes: Some(512 * 1024 * 1024),
            max_processes: Some(4),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub current_dir: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub limits: ProcessLimits,
}

impl ProcessSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            env: BTreeMap::new(),
            limits: ProcessLimits::default(),
        }
    }

    pub fn from_command(command: &Command) -> Self {
        let mut spec = Self::new(command.get_program());
        spec.args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        spec.current_dir = command.get_current_dir().map(Path::to_path_buf);
        spec.env = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    (
                        name.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect();
        spec
    }
}

#[derive(Debug)]
pub struct SupervisedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub diagnostics: ExecutionDiagnostics,
}

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

pub struct ManagedChildPipes {
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

pub struct ManagedChild {
    child: Child,
    isolation: ProcessIsolation,
}

impl ManagedChild {
    pub fn spawn(spec: &ProcessSpec) -> Result<(Self, ManagedChildPipes), ProcessError> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(current_dir) = &spec.current_dir {
            command.current_dir(current_dir);
        }
        command.envs(&spec.env);
        configure_process_group(&mut command);
        let mut child = command.spawn().map_err(ProcessError::Spawn)?;
        let isolation = ProcessIsolation::attach(&child, &spec.limits).map_err(|error| {
            let _ = child.kill();
            let _ = child.wait();
            ProcessError::Isolation(error)
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            ProcessError::MissingPipe("stdin")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            ProcessError::MissingPipe("stdout")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            ProcessError::MissingPipe("stderr")
        })?;
        Ok((
            Self { child, isolation },
            ManagedChildPipes {
                stdin,
                stdout,
                stderr,
            },
        ))
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, std::io::Error> {
        self.child.try_wait()
    }

    pub fn terminate(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.isolation.kill_tree(&mut self.child);
            let _ = self.child.wait();
        }
    }

    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

pub fn run_with_input(spec: &ProcessSpec, input: &[u8]) -> Result<SupervisedOutput, ProcessError> {
    let started = Instant::now();
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(current_dir);
    }
    command.envs(&spec.env);
    configure_process_group(&mut command);

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

    let deadline = started + spec.limits.timeout;
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
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
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
    Ok(SupervisedOutput {
        diagnostics: diagnostics(started, status.code(), &stdout, &stderr, false, false),
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
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

struct ProcessIsolation {
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    process_group: i32,
}

impl ProcessIsolation {
    fn attach(child: &std::process::Child, limits: &ProcessLimits) -> Result<Self, String> {
        attach_process_isolation(child, limits)
    }

    fn kill_tree(&self, child: &mut std::process::Child) {
        kill_process_tree(self, child);
    }
}

#[cfg(windows)]
impl Drop for ProcessIsolation {
    fn drop(&mut self) {
        if !self.job.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.job);
            }
        }
    }
}

#[cfg(not(windows))]
impl Drop for ProcessIsolation {
    fn drop(&mut self) {}
}

#[cfg(windows)]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(windows)]
fn attach_process_isolation(
    child: &std::process::Child,
    limits: &ProcessLimits,
) -> Result<ProcessIsolation, String> {
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Some(max_processes) = limits.max_processes {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            info.BasicLimitInformation.ActiveProcessLimit = max_processes;
        }
        if let Some(memory_bytes) = limits.memory_bytes {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.JobMemoryLimit = memory_bytes;
        }
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(std::io::Error::last_os_error().to_string());
        }
        if AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(ProcessIsolation { job })
    }
}

#[cfg(unix)]
fn attach_process_isolation(
    child: &std::process::Child,
    _limits: &ProcessLimits,
) -> Result<ProcessIsolation, String> {
    Ok(ProcessIsolation {
        process_group: child.id() as i32,
    })
}

#[cfg(not(any(windows, unix)))]
fn attach_process_isolation(
    _child: &std::process::Child,
    _limits: &ProcessLimits,
) -> Result<ProcessIsolation, String> {
    Ok(ProcessIsolation {})
}

#[cfg(windows)]
fn kill_process_tree(isolation: &ProcessIsolation, child: &mut std::process::Child) {
    if !isolation.job.is_null() {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(isolation.job, 1);
        }
    }
    let _ = child.kill();
}

#[cfg(unix)]
fn kill_process_tree(isolation: &ProcessIsolation, child: &mut std::process::Child) {
    unsafe {
        libc::kill(-isolation.process_group, libc::SIGKILL);
    }
    let _ = child.kill();
}

#[cfg(not(any(windows, unix)))]
fn kill_process_tree(_isolation: &ProcessIsolation, child: &mut std::process::Child) {
    let _ = child.kill();
}

pub fn executable_path_within(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let candidate =
        std::fs::canonicalize(root.join(relative)).map_err(|error| error.to_string())?;
    if !candidate.starts_with(&root) || !candidate.is_file() {
        return Err("executable resolves outside its package root".to_owned());
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_output_is_reported_as_a_resource_limit() {
        let mut spec = if cfg!(windows) {
            let mut spec = ProcessSpec::new("powershell.exe");
            spec.args = vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                "[Console]::Out.Write(('x' * 200000))".to_owned(),
            ];
            spec
        } else {
            let mut spec = ProcessSpec::new("sh");
            spec.args = vec![
                "-c".to_owned(),
                "head -c 200000 /dev/zero | tr '\\0' x".to_owned(),
            ];
            spec
        };
        spec.limits.stdout_bytes = 1024;
        spec.limits.stderr_bytes = 1024;
        let error = run_with_input(&spec, b"").expect_err("output limit");
        assert!(matches!(error, ProcessError::OutputLimit { .. }));
    }

    #[test]
    fn normal_process_reports_diagnostics() {
        let mut spec = if cfg!(windows) {
            let mut spec = ProcessSpec::new("cmd.exe");
            spec.args = vec!["/C".to_owned(), "set /p x=& echo ok".to_owned()];
            spec
        } else {
            let mut spec = ProcessSpec::new("sh");
            spec.args = vec!["-c".to_owned(), "cat >/dev/null; printf ok".to_owned()];
            spec
        };
        spec.limits.timeout = Duration::from_secs(5);
        let output = run_with_input(&spec, b"input\n").expect("process");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
        assert!(output.diagnostics.duration_ms.is_some());
    }

    #[test]
    fn timeout_terminates_a_child_that_never_reads_large_stdin() {
        let mut spec = if cfg!(windows) {
            let mut spec = ProcessSpec::new("powershell.exe");
            spec.args = vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                "Start-Sleep -Seconds 30".to_owned(),
            ];
            spec
        } else {
            let mut spec = ProcessSpec::new("sh");
            spec.args = vec!["-c".to_owned(), "sleep 30".to_owned()];
            spec
        };
        spec.limits.timeout = Duration::from_millis(250);
        let input = vec![b'x'; 8 * 1024 * 1024];
        let started = Instant::now();
        let error = run_with_input(&spec, &input).expect_err("stdin-blocked child must time out");
        assert!(matches!(error, ProcessError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
