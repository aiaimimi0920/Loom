use super::*;

enum PersistentHostStdoutEvent {
    Line(Vec<u8>),
    Oversized,
    Error(String),
    Eof,
}

#[derive(Default)]
struct PersistentHostStderr {
    bytes: Vec<u8>,
    truncated: bool,
}

impl PersistentHostStderr {
    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= PERSISTENT_HOST_ERROR_BYTES {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&chunk[chunk.len() - PERSISTENT_HOST_ERROR_BYTES..]);
            self.truncated = true;
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(PERSISTENT_HOST_ERROR_BYTES);
        if overflow > 0 {
            self.bytes.drain(..overflow);
            self.truncated = true;
        }
        self.bytes.extend_from_slice(chunk);
    }

    fn text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes).trim().to_owned();
        if self.truncated {
            text.insert_str(0, "[truncated] ");
        }
        text
    }
}

pub(super) struct PersistentFrameworkHost {
    key: String,
    process: ManagedChild,
    stdin: Option<ChildStdin>,
    stdout: Receiver<PersistentHostStdoutEvent>,
    stderr: Arc<Mutex<PersistentHostStderr>>,
    last_used: Instant,
    _slot: PersistentHostSlot,
}

impl Drop for PersistentFrameworkHost {
    fn drop(&mut self) {
        // EOF gives runtime-host a short grace window to drop its bounded MCP session cache, which
        // invokes the transport close lifecycle (HTTP DELETE or stdio child termination). A wedged
        // host is still killed as one managed process tree after the grace window.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match self.process.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        self.process.terminate();
    }
}

enum PersistentHostError {
    Process(ProcessError),
    Exited { code: Option<i32>, stderr: String },
    Reader(String),
    Timeout { stderr: String },
    Cancelled { stderr: String },
    OutputLimit { stderr: String },
    PoolExhausted,
}

#[derive(Default)]
struct PersistentFrameworkHostPool {
    hosts: Vec<PersistentFrameworkHost>,
}

thread_local! {
    static PERSISTENT_FRAMEWORK_HOST_POOL: RefCell<PersistentFrameworkHostPool> =
        RefCell::new(PersistentFrameworkHostPool::default());
}

static PERSISTENT_HOST_COUNT: AtomicUsize = AtomicUsize::new(0);

struct PersistentHostSlot;

impl PersistentHostSlot {
    fn acquire() -> Result<Self, PersistentHostError> {
        PERSISTENT_HOST_COUNT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < MAX_PERSISTENT_MCP_HOSTS).then_some(current + 1)
            })
            .map(|_| Self)
            .map_err(|_| PersistentHostError::PoolExhausted)
    }
}

impl Drop for PersistentHostSlot {
    fn drop(&mut self) {
        let previous = PERSISTENT_HOST_COUNT.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "persistent host count underflow");
    }
}

pub(super) struct TempDirectoryGuard {
    path: PathBuf,
}

impl TempDirectoryGuard {
    pub(super) fn create(path: PathBuf) -> std::io::Result<Self> {
        // `create_dir_all` accepts an attacker-precreated leaf. A single, atomic
        // create instead makes request-directory collisions fail closed.
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectoryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl PersistentFrameworkHost {
    fn spawn(key: String, spec: &ProcessSpec) -> Result<Self, PersistentHostError> {
        let slot = PersistentHostSlot::acquire()?;
        let (process, pipes) = ManagedChild::spawn(spec).map_err(PersistentHostError::Process)?;
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_limit = spec.limits.stdout_bytes;
        thread::spawn(move || read_persistent_host_stdout(pipes.stdout, stdout_limit, stdout_tx));
        let stderr = Arc::new(Mutex::new(PersistentHostStderr::default()));
        let stderr_capture = Arc::clone(&stderr);
        thread::spawn(move || drain_persistent_host_stderr(pipes.stderr, stderr_capture));
        Ok(Self {
            key,
            process,
            stdin: Some(pipes.stdin),
            stdout: stdout_rx,
            stderr,
            last_used: Instant::now(),
            _slot: slot,
        })
    }

    fn request(
        &mut self,
        payload: &[u8],
        timeout: Duration,
        cancellation: Option<&AtomicBool>,
    ) -> Result<Vec<u8>, PersistentHostError> {
        if let Some(status) = self
            .process
            .try_wait()
            .map_err(|error| PersistentHostError::Reader(error.to_string()))?
        {
            return Err(PersistentHostError::Exited {
                code: status.code(),
                stderr: self.stderr_text(),
            });
        }
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            PersistentHostError::Reader("persistent framework stdin is closed".to_owned())
        })?;
        stdin
            .write_all(payload)
            .and_then(|()| stdin.flush())
            .map_err(|error| PersistentHostError::Process(ProcessError::Stdin(error)))?;

        let deadline = Instant::now() + timeout.max(Duration::from_millis(1));
        loop {
            if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                self.process.terminate();
                return Err(PersistentHostError::Cancelled {
                    stderr: self.stderr_text(),
                });
            }
            let now = Instant::now();
            if now >= deadline {
                self.process.terminate();
                return Err(PersistentHostError::Timeout {
                    stderr: self.stderr_text(),
                });
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(25));
            match self.stdout.recv_timeout(wait) {
                Ok(PersistentHostStdoutEvent::Line(line)) => return Ok(line),
                Ok(PersistentHostStdoutEvent::Oversized) => {
                    self.process.terminate();
                    return Err(PersistentHostError::OutputLimit {
                        stderr: self.stderr_text(),
                    });
                }
                Ok(PersistentHostStdoutEvent::Error(error)) => {
                    self.process.terminate();
                    return Err(PersistentHostError::Reader(error));
                }
                Ok(PersistentHostStdoutEvent::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let code = self
                        .process
                        .try_wait()
                        .ok()
                        .flatten()
                        .and_then(|status| status.code());
                    return Err(PersistentHostError::Exited {
                        code,
                        stderr: self.stderr_text(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn stderr_text(&self) -> String {
        self.stderr
            .lock()
            .map(|stderr| stderr.text())
            .unwrap_or_else(|poisoned| poisoned.into_inner().text())
    }
}

fn read_persistent_host_stdout(
    mut stdout: std::process::ChildStdout,
    limit: usize,
    sender: mpsc::Sender<PersistentHostStdoutEvent>,
) {
    let mut buffer = [0_u8; 16 * 1024];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => {
                if oversized {
                    let _ = sender.send(PersistentHostStdoutEvent::Oversized);
                } else if !line.is_empty() {
                    let _ = sender.send(PersistentHostStdoutEvent::Line(line));
                }
                let _ = sender.send(PersistentHostStdoutEvent::Eof);
                return;
            }
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(PersistentHostStdoutEvent::Error(error.to_string()));
                return;
            }
        };
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                let event = if oversized {
                    PersistentHostStdoutEvent::Oversized
                } else {
                    PersistentHostStdoutEvent::Line(std::mem::take(&mut line))
                };
                if sender.send(event).is_err() {
                    return;
                }
                line.clear();
                oversized = false;
            } else if line.len() < limit {
                line.push(*byte);
            } else {
                oversized = true;
            }
        }
    }
}

fn drain_persistent_host_stderr(
    mut stderr: std::process::ChildStderr,
    capture: Arc<Mutex<PersistentHostStderr>>,
) {
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => match capture.lock() {
                Ok(mut capture) => capture.push(&buffer[..read]),
                Err(poisoned) => poisoned.into_inner().push(&buffer[..read]),
            },
        }
    }
}

pub(super) fn persistent_host_key(
    command_path: &Path,
    manifest_text: &str,
    args: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(command_path.to_string_lossy().as_bytes());
    if let Ok(metadata) = fs::metadata(command_path) {
        hasher.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified().and_then(|value| {
            value
                .duration_since(UNIX_EPOCH)
                .map_err(|error| std::io::Error::other(error.to_string()))
        }) {
            hasher.update(modified.as_nanos().to_le_bytes());
        }
    }
    hasher.update(manifest_text.as_bytes());
    hasher.update(format!("{:?}", crate::network_policy::runtime_proxy()).as_bytes());
    for arg in args {
        hasher.update([0]);
        hasher.update(arg.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn take_persistent_host(key: &str) -> Option<PersistentFrameworkHost> {
    let now = Instant::now();
    let (host, expired) = PERSISTENT_FRAMEWORK_HOST_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut expired = Vec::new();
        let mut index = 0;
        while index < pool.hosts.len() {
            if now.saturating_duration_since(pool.hosts[index].last_used)
                >= PERSISTENT_MCP_HOST_IDLE_LIFETIME
            {
                expired.push(pool.hosts.remove(index));
            } else {
                index += 1;
            }
        }
        let host = pool
            .hosts
            .iter()
            .position(|host| host.key == key)
            .map(|index| pool.hosts.remove(index));
        (host, expired)
    });
    drop(expired);
    host
}

pub(super) fn return_persistent_host(mut host: PersistentFrameworkHost) {
    host.last_used = Instant::now();
    let evicted = PERSISTENT_FRAMEWORK_HOST_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let duplicate = pool.hosts.iter().any(|existing| existing.key == host.key);
        if duplicate {
            Some(host)
        } else {
            pool.hosts.push(host);
            if pool.hosts.len() > MAX_PERSISTENT_MCP_HOSTS {
                let oldest = pool
                    .hosts
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, host)| host.last_used)
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                Some(pool.hosts.remove(oldest))
            } else {
                None
            }
        }
    });
    drop(evicted);
}

#[cfg(test)]
pub(super) fn clear_persistent_host_pool() {
    PERSISTENT_FRAMEWORK_HOST_POOL.with(|pool| pool.borrow_mut().hosts.clear());
}

#[cfg(test)]
pub(super) fn persistent_host_count() -> usize {
    PERSISTENT_HOST_COUNT.load(Ordering::Acquire)
}

#[cfg(test)]
pub(super) fn exercise_persistent_host_slot_limit() -> (usize, bool, usize) {
    let slots = (0..MAX_PERSISTENT_MCP_HOSTS)
        .map(|_| match PersistentHostSlot::acquire() {
            Ok(slot) => slot,
            Err(_) => panic!("reserve host slot within the limit"),
        })
        .collect::<Vec<_>>();
    let reserved = persistent_host_count();
    let exhausted = matches!(
        PersistentHostSlot::acquire(),
        Err(PersistentHostError::PoolExhausted)
    );
    drop(slots);
    (reserved, exhausted, persistent_host_count())
}

pub(super) fn request_persistent_mcp_host(
    key: String,
    spec: &ProcessSpec,
    payload: &[u8],
    cancellation: Option<&AtomicBool>,
    tool: &ToolDefinition,
    framework: &str,
) -> ToolRegistryResult<(Vec<u8>, PersistentFrameworkHost)> {
    let mut host = match take_persistent_host(&key) {
        Some(host) => host,
        None => PersistentFrameworkHost::spawn(key, spec).map_err(|error| {
            map_persistent_host_error(tool, framework, spec.limits.timeout, error)
        })?,
    };
    let stdout = host
        .request(payload, spec.limits.timeout, cancellation)
        .map_err(|error| map_persistent_host_error(tool, framework, spec.limits.timeout, error))?;
    Ok((stdout, host))
}

fn map_persistent_host_error(
    tool: &ToolDefinition,
    framework: &str,
    timeout: Duration,
    error: PersistentHostError,
) -> ToolRegistryError {
    match error {
        PersistentHostError::Process(error) => map_process_error(tool, framework, timeout, error),
        PersistentHostError::Exited { code, stderr } => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            message: "persistent framework process exited unexpectedly".to_owned(),
            detail: crate::bounded_error_text(&stderr),
        },
        PersistentHostError::Reader(reason) => ToolRegistryError::FrameworkProcessIo {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            reason,
        },
        PersistentHostError::Timeout { stderr } => map_process_error(
            tool,
            framework,
            timeout,
            ProcessError::Timeout {
                stdout: Vec::new(),
                stderr: stderr.into_bytes(),
                diagnostics: ExecutionDiagnostics {
                    timed_out: true,
                    ..ExecutionDiagnostics::default()
                },
            },
        ),
        PersistentHostError::Cancelled { stderr } => map_process_error(
            tool,
            framework,
            timeout,
            ProcessError::Cancelled {
                stdout: Vec::new(),
                stderr: stderr.into_bytes(),
                diagnostics: ExecutionDiagnostics::default(),
            },
        ),
        PersistentHostError::OutputLimit { stderr } => map_process_error(
            tool,
            framework,
            timeout,
            ProcessError::OutputLimit {
                stdout: Vec::new(),
                stderr: stderr.into_bytes(),
                diagnostics: ExecutionDiagnostics {
                    stdout_truncated: true,
                    resource_limited: true,
                    ..ExecutionDiagnostics::default()
                },
            },
        ),
        PersistentHostError::PoolExhausted => ToolRegistryError::FrameworkProcessFailed {
            id: tool.id.clone(),
            framework: framework.to_owned(),
            code: "resource_limit".to_owned(),
            message: format!(
                "persistent framework host limit ({MAX_PERSISTENT_MCP_HOSTS}) is exhausted"
            ),
            detail: String::new(),
        },
    }
}
