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
    /// Peak memory ever charged to the child and everything it started, in bytes. This is a local
    /// observation rather than part of `diagnostics`, because `ExecutionDiagnostics` is a wire type
    /// shared with framework responses and a framework cannot report this number about itself.
    ///
    /// `None` means no measurement rather than a measurement of zero: platforms without a Windows
    /// job object have no equivalent counter, and Windows reports an unrecorded counter as zero.
    pub peak_memory_bytes: Option<u64>,
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
        let mut command = supervised_command(spec);
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
    run_with_input_internal(spec, input, None)
}

pub fn run_with_input_cancellable(
    spec: &ProcessSpec,
    input: &[u8],
    cancellation: &AtomicBool,
) -> Result<SupervisedOutput, ProcessError> {
    run_with_input_internal(spec, input, Some(cancellation))
}

fn supervised_command(spec: &ProcessSpec) -> Command {
    let mut command = Command::new(process_path(&spec.program));
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(process_path(current_dir));
    }
    apply_supervised_environment(&mut command, spec);
    configure_process_group(&mut command);
    command
}

fn apply_supervised_environment(command: &mut Command, spec: &ProcessSpec) {
    command.env_clear();
    for (key, value) in inherited_runtime_environment() {
        command.env(key, value);
    }
    command.envs(&spec.env);
}

/// The environment a managed process inherits, filtered down to an allowlist so that host secrets in
/// the Loom process environment never reach a plugin.
///
/// `LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` is a test seam rather than a product capability. The
/// image-search sample Art refuses to download an image from a loopback address, which is correct for
/// the shipped Art but leaves the install-and-execute test with nowhere to serve its fixture image
/// from. With the variable set, that Art — and only that Art — permits a loopback address written
/// literally in an image URL; a hostname that resolves to loopback stays refused, as does every other
/// blocked range. It is allowlisted here because an Art runs two spawns deep, so the daemon, the
/// framework runtime host, and the Art entry each scrub the environment. No package can set it: an
/// Art runtime manifest declares a command and its arguments and nothing else, so only whoever
/// launches Loom can turn the seam on.
fn inherited_runtime_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    #[cfg(windows)]
    const ALLOWED: &[&str] = &[
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "SYSTEMDRIVE",
        "COMSPEC",
        "TEMP",
        "TMP",
        "OS",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "USERNAME",
        "USERDOMAIN",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PUBLIC",
        "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES",
    ];
    #[cfg(not(windows))]
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "TERM",
        "TZ",
        "SHELL",
        "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES",
    ];
    std::env::vars_os()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            ALLOWED
                .iter()
                .any(|allowed| key.eq_ignore_ascii_case(allowed))
        })
        .collect()
}

#[cfg(windows)]
fn process_path(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::iter;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    const LEGACY_MAX_DIRECTORY_PATH: usize = 248;
    if !path.is_absolute() || path.as_os_str().encode_wide().count() < LEGACY_MAX_DIRECTORY_PATH {
        return path.to_path_buf();
    }

    // CreateProcessW does not accept a verbatim (\\?\) current directory.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let wide = canonical
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let mut short = vec![0u16; 32_768];
    let written =
        unsafe { GetShortPathNameW(wide.as_ptr(), short.as_mut_ptr(), short.len() as u32) };
    if written == 0 || written as usize >= short.len() {
        return path.to_path_buf();
    }
    let short = &short[..written as usize];
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    if let Some(rest) = short.strip_prefix(VERBATIM_UNC_PREFIX) {
        let ordinary_unc = [b'\\' as u16, b'\\' as u16]
            .into_iter()
            .chain(rest.iter().copied())
            .collect::<Vec<_>>();
        OsString::from_wide(&ordinary_unc).into()
    } else if let Some(rest) = short.strip_prefix(VERBATIM_PREFIX) {
        OsString::from_wide(rest).into()
    } else {
        OsString::from_wide(short).into()
    }
}

#[cfg(not(windows))]
fn process_path(path: &Path) -> PathBuf {
    path.to_path_buf()
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

    /// Peak memory charged to this isolation group so far, in bytes, or `None` when the platform
    /// keeps no such counter. Valid only while the group is open.
    fn peak_memory_bytes(&self) -> Option<u64> {
        isolation_peak_memory_bytes(self)
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

#[cfg(windows)]
fn isolation_peak_memory_bytes(isolation: &ProcessIsolation) -> Option<u64> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::System::JobObjects::{
        JobObjectExtendedLimitInformation, QueryInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    };

    if isolation.job.is_null() {
        return None;
    }
    unsafe {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        let mut returned = 0u32;
        if QueryInformationJobObject(
            isolation.job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            &mut returned,
        ) == 0
        {
            return None;
        }
        // A process that used no memory at all does not exist, so a zero counter means Windows
        // recorded nothing rather than that there is nothing to record.
        let peak = info.PeakJobMemoryUsed as u64;
        (peak > 0).then_some(peak)
    }
}

#[cfg(not(windows))]
fn isolation_peak_memory_bytes(_isolation: &ProcessIsolation) -> Option<u64> {
    // A process group is not an accounting boundary the way a job object is: there is no kernel
    // counter to read, and summing `/proc` samples would measure when Loom happened to look rather
    // than the peak. Reporting nothing is more honest than reporting a sample.
    None
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

    #[cfg(windows)]
    #[test]
    fn process_runs_from_a_deep_windows_working_directory() {
        use std::os::windows::ffi::OsStrExt;

        let root = std::env::temp_dir().join(format!(
            "loom-process-deep-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let mut deep_dir = root.clone();
        while deep_dir.as_os_str().encode_wide().count() <= 280 {
            deep_dir.push("framework-package-segment-0123456789");
        }
        std::fs::create_dir_all(&deep_dir).expect("create deep working directory");
        let prepared_dir = process_path(&deep_dir);
        assert!(
            prepared_dir.as_os_str().encode_wide().count() < 248,
            "deep working directory was not shortened: {}",
            prepared_dir.display()
        );

        let command = PathBuf::from(std::env::var_os("ComSpec").expect("ComSpec"));
        let deep_program = deep_dir.join("framework-runtime.exe");
        std::fs::copy(command, &deep_program).expect("copy deep framework runtime");
        let mut spec = ProcessSpec::new(deep_program);
        spec.args = vec![
            "/D".to_owned(),
            "/C".to_owned(),
            "echo deep-path-ok".to_owned(),
        ];
        spec.current_dir = Some(deep_dir);
        spec.limits.timeout = Duration::from_secs(5);

        let output = run_with_input(&spec, b"").expect("run from deep working directory");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "deep-path-ok"
        );

        std::fs::remove_dir_all(&root).expect("remove deep working directory");
    }

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

    #[test]
    fn cancellation_terminates_the_managed_process_tree() {
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
        spec.limits.timeout = Duration::from_secs(30);
        let cancellation = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancellation);
        let toggler = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            signal.store(true, Ordering::Release);
        });
        let started = Instant::now();
        let error = run_with_input_cancellable(&spec, b"", cancellation.as_ref())
            .expect_err("managed process must be cancelled");
        toggler.join().expect("cancellation toggler");
        assert!(matches!(error, ProcessError::Cancelled { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn supervised_process_does_not_inherit_host_secrets() {
        const SECRET: &str = "loom-process-should-not-leak";
        let previous = std::env::var_os("LOOM_DAEMON_TOKEN");
        std::env::set_var("LOOM_DAEMON_TOKEN", SECRET);
        let result = std::panic::catch_unwind(|| {
            let mut spec = if cfg!(windows) {
                let mut spec = ProcessSpec::new("cmd.exe");
                spec.args = vec!["/C".to_owned(), "echo %LOOM_DAEMON_TOKEN%".to_owned()];
                spec
            } else {
                let mut spec = ProcessSpec::new("sh");
                spec.args = vec![
                    "-c".to_owned(),
                    "printf '%s' \"$LOOM_DAEMON_TOKEN\"".to_owned(),
                ];
                spec
            };
            spec.limits.timeout = Duration::from_secs(5);
            run_with_input(&spec, b"").expect("echo secret env")
        });
        match previous {
            Some(value) => std::env::set_var("LOOM_DAEMON_TOKEN", value),
            None => std::env::remove_var("LOOM_DAEMON_TOKEN"),
        }
        let output = result.expect("supervised secret probe");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains(SECRET),
            "child inherited host secret: {stdout:?}"
        );
    }

    #[test]
    fn supervised_process_keeps_required_runtime_environment() {
        let required = if cfg!(windows) {
            ["PATH", "SYSTEMROOT", "TEMP", "USERPROFILE", "APPDATA"]
        } else {
            ["PATH", "HOME", "TMPDIR", "LANG", "SHELL"]
        };
        let inherited = inherited_runtime_environment()
            .into_iter()
            .map(|(key, value)| (key.to_string_lossy().to_string(), value))
            .collect::<std::collections::HashMap<_, _>>();
        for name in required {
            if std::env::var_os(name).is_some() {
                assert!(
                    inherited.keys().any(|key| key.eq_ignore_ascii_case(name)),
                    "runtime environment dropped {name}"
                );
            }
        }
    }

    #[test]
    fn supervised_process_inherits_the_image_search_loopback_seam() {
        const SEAM: &str = "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES";
        const UNRELATED: &str = "LOOM_IMAGE_SEARCH_UNRELATED_SETTING";
        let previous_seam = std::env::var_os(SEAM);
        std::env::set_var(SEAM, "1");
        std::env::set_var(UNRELATED, "1");
        let result = std::panic::catch_unwind(|| {
            let inherited = inherited_runtime_environment()
                .into_iter()
                .map(|(key, value)| (key.to_string_lossy().to_string(), value))
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(
                inherited.get(SEAM).map(|value| value.to_string_lossy()),
                Some(std::borrow::Cow::Borrowed("1")),
                "the loopback test seam must survive the environment scrub, because the Art that \
                 reads it runs two spawns deep"
            );
            // The seam is one named exception, not a `LOOM_`-prefixed passthrough.
            assert!(
                !inherited.contains_key(UNRELATED),
                "an unrelated Loom variable must still be scrubbed"
            );
        });
        std::env::remove_var(UNRELATED);
        match previous_seam {
            Some(value) => std::env::set_var(SEAM, value),
            None => std::env::remove_var(SEAM),
        }
        result.expect("loopback seam inheritance probe");
    }

    /// Loom's peak-memory budget for one framework process. Every framework Loom ships runs as a
    /// supervised interpreter started once per execution, so the number this measures is the floor
    /// under every art execution: whatever the interpreter costs before the work begins.
    ///
    /// The child here is PowerShell because that is what Loom's own sample art frameworks run on,
    /// and the budget is generous on purpose. It is not a claim about how much memory PowerShell
    /// should need; it exists so that supervising a framework process cannot quietly start costing
    /// hundreds of megabytes more than it does today.
    #[test]
    fn one_framework_process_stays_within_its_peak_memory_budget() {
        // Measured at 65,544,192 bytes (about 63 MiB) on 2026-08-22. The ceiling is well above that
        // but still below the 512 MiB the default limits enforce, since a job that hits the enforced
        // limit is killed and would never reach this assertion.
        const BUDGET_BYTES: u64 = 256 * 1024 * 1024;

        let mut spec = if cfg!(windows) {
            let mut spec = ProcessSpec::new("powershell.exe");
            spec.args = vec![
                "-NoProfile".to_owned(),
                "-Command".to_owned(),
                "Write-Output ok".to_owned(),
            ];
            spec
        } else {
            let mut spec = ProcessSpec::new("sh");
            spec.args = vec!["-c".to_owned(), "printf ok".to_owned()];
            spec
        };
        spec.limits.timeout = Duration::from_secs(30);

        let output = run_with_input(&spec, b"").expect("run one framework-shaped process");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");

        let Some(peak) = output.peak_memory_bytes else {
            // The platform keeps no such counter, so there is no measurement to compare. Passing
            // here reports the truth: nothing was measured, as opposed to a small number measured.
            println!(
                "perf budget framework_process_peak_memory_bytes: not measured on this platform"
            );
            return;
        };
        loom_perf::assert_within(
            "framework_process_peak_memory_bytes",
            "bytes",
            peak,
            BUDGET_BYTES,
        );
    }
}
