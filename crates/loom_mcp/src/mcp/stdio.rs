//! Bounded synchronous stdio MCP transport.

use super::*;

/// Synchronous stdio MCP JSON-RPC client.
pub struct StdioMcpClient {
    process: loom_process::ManagedChild,
    stdin: std::process::ChildStdin,
    stdout: Receiver<StdoutEvent>,
    stderr: Arc<Mutex<BoundedStderr>>,
    sensitive_values: Vec<String>,
    request_timeout: Duration,
    next_id: u64,
}

const MCP_STDOUT_QUEUE_CAPACITY: usize = 4;
pub(super) const MCP_MAX_MALFORMED_MESSAGES: usize = 32;
pub(super) enum StdoutEvent {
    Line(Vec<u8>),
    Eof,
    Error(String),
    Oversized,
}

#[derive(Default)]
pub(super) struct BoundedStderr {
    bytes: Vec<u8>,
    total: u64,
}

impl BoundedStderr {
    fn text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.bytes).trim().to_owned();
        if self.total > self.bytes.len() as u64 {
            text.push_str(" [truncated]");
        }
        text
    }
}

impl StdioMcpClient {
    pub fn spawn(config: &McpServerConfig) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::Stdio {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }
        Self::spawn_with_timeout(
            config,
            Duration::from_secs(MCP_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed)),
        )
    }

    pub fn spawn_with_timeout(
        config: &McpServerConfig,
        request_timeout: Duration,
    ) -> McpResult<Self> {
        if !config.enabled {
            return Err(McpError::Disabled {
                server_id: config.id.clone(),
            });
        }
        config.validate()?;
        if config.transport != McpTransport::Stdio {
            return Err(McpError::UnsupportedTransport(
                config.transport.label().to_owned(),
            ));
        }
        // A package-backed server runs a file the installer extracted, and it runs it with the
        // user's credentials in its environment, so the digests recorded at install are checked
        // here rather than trusted from `servers.json`.
        crate::package::verify_installed_entry(config)
            .map_err(|error| McpError::PackageIntegrity(error.to_string()))?;
        let request_timeout = request_timeout.max(Duration::from_millis(1));
        let spawn_spec = spawn_command_spec(config);
        let mut process_spec = loom_process::ProcessSpec::new(&spawn_spec.program);
        process_spec.args = spawn_spec.args;
        process_spec.env = config.env.clone();
        process_spec.limits.timeout = request_timeout;
        process_spec.limits.stdout_bytes = MCP_MAX_MESSAGE_BYTES;
        process_spec.limits.stderr_bytes = MCP_MAX_STDERR_BYTES;
        process_spec.limits.memory_bytes = Some(
            usize::try_from(MCP_MEMORY_LIMIT_BYTES.load(Ordering::Relaxed)).unwrap_or(usize::MAX),
        );
        process_spec.limits.max_processes = Some(8);
        let (process, pipes) = match loom_process::ManagedChild::spawn(&process_spec) {
            Ok(value) => value,
            Err(loom_process::ProcessError::Spawn(source)) => {
                return Err(McpError::ProcessStart {
                    command: config.command.clone(),
                    source,
                })
            }
            Err(error) => {
                return Err(McpError::ProcessSupervision {
                    command: config.command.clone(),
                    reason: error.to_string(),
                })
            }
        };
        let (stdout_tx, stdout_rx) = mpsc::sync_channel(MCP_STDOUT_QUEUE_CAPACITY);
        thread::spawn(move || read_stdout_lines(pipes.stdout, stdout_tx));
        let stderr = Arc::new(Mutex::new(BoundedStderr::default()));
        let stderr_capture = Arc::clone(&stderr);
        thread::spawn(move || drain_stderr(pipes.stderr, stderr_capture));

        Ok(Self {
            process,
            stdin: pipes.stdin,
            stdout: stdout_rx,
            stderr,
            sensitive_values: collect_sensitive_values(config.env.values()),
            request_timeout,
            next_id: 1,
        })
    }

    pub fn initialize(&mut self) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(None)
    }

    pub fn initialize_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.initialize_with_cancellation(Some(cancellation))
    }

    fn initialize_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        for (version_index, protocol_version) in
            MCP_SUPPORTED_PROTOCOL_VERSIONS.iter().copied().enumerate()
        {
            let id = self.next_request_id();
            self.write_message(&initialize_request_for_version(id, protocol_version))?;
            match self.read_result_with_cancellation(id, cancellation) {
                Ok(result) => {
                    validate_initialize_result(&result)?;
                    self.write_message(&initialized_notification())?;
                    return Ok(result);
                }
                Err(error)
                    if is_protocol_compatibility_rejection(&error)
                        && version_index + 1 < MCP_SUPPORTED_PROTOCOL_VERSIONS.len() => {}
                Err(error) if is_protocol_compatibility_rejection(&error) => {
                    return Err(no_common_protocol_error(&error));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("MCP supported protocol revision table is non-empty")
    }

    pub fn list_tools(&mut self) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(None)
    }

    pub fn list_tools_cancellable(&mut self, cancellation: &AtomicBool) -> McpResult<JsonValue> {
        self.list_tools_with_cancellation(Some(cancellation))
    }

    fn list_tools_with_cancellation(
        &mut self,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let id = self.next_request_id();
        self.write_message(&tools_list_request(id))?;
        self.read_result_with_cancellation(id, cancellation)
    }

    pub fn call_tool(&mut self, name: &str, arguments: JsonValue) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, None)
    }

    pub fn call_tool_cancellable(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: &AtomicBool,
    ) -> McpResult<JsonValue> {
        self.call_tool_with_cancellation(name, arguments, Some(cancellation))
    }

    fn call_tool_with_cancellation(
        &mut self,
        name: &str,
        arguments: JsonValue,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        validate_tool_call_payload(name, &arguments)?;
        let id = self.next_request_id();
        self.write_message(&tools_call_request(id, name, arguments))?;
        self.read_result_with_cancellation(id, cancellation)
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn write_message(&mut self, message: &JsonValue) -> McpResult<()> {
        serde_json::to_writer(&mut self.stdin, message)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn read_result_with_cancellation(
        &mut self,
        expected_id: u64,
        cancellation: Option<&AtomicBool>,
    ) -> McpResult<JsonValue> {
        let deadline = Instant::now() + self.request_timeout;
        let mut malformed_messages = 0usize;
        loop {
            if cancellation.is_some_and(|token| token.load(Ordering::Acquire)) {
                self.process.terminate();
                return Err(McpError::Cancelled);
            }
            let now = Instant::now();
            if now >= deadline {
                self.process.terminate();
                return Err(McpError::Timeout {
                    timeout_ms: self.request_timeout.as_millis(),
                    stderr: self.stderr_text(),
                });
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(25));
            let line = match self.stdout.recv_timeout(wait) {
                Ok(StdoutEvent::Line(line)) => line,
                Ok(StdoutEvent::Oversized) => {
                    self.process.terminate();
                    return Err(McpError::OutputLimit {
                        limit: MCP_MAX_MESSAGE_BYTES,
                    });
                }
                Ok(StdoutEvent::Error(error)) => return Err(McpError::Protocol(error)),
                Ok(StdoutEvent::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let code = self
                        .process
                        .try_wait()
                        .ok()
                        .flatten()
                        .and_then(|status| status.code());
                    return Err(McpError::ProcessExited {
                        code,
                        stderr: self.stderr_text(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    continue;
                }
            };
            let trimmed = String::from_utf8_lossy(&line).trim().to_owned();
            if trimmed.is_empty() {
                continue;
            }

            let message = match serde_json::from_str::<JsonValue>(&trimmed) {
                Ok(message) => message,
                Err(_) => {
                    malformed_messages += 1;
                    if malformed_messages > MCP_MAX_MALFORMED_MESSAGES {
                        self.process.terminate();
                        return Err(McpError::Protocol(format!(
                            "MCP server emitted more than {MCP_MAX_MALFORMED_MESSAGES} malformed JSON messages"
                        )));
                    }
                    continue;
                }
            };

            if message.get("id") != Some(&serde_json::json!(expected_id)) {
                continue;
            }

            if let Some(error) = message.get("error") {
                return Err(McpError::JsonRpc(error.clone()));
            }

            return message.get("result").cloned().ok_or_else(|| {
                McpError::Protocol(format!("MCP response id {expected_id} missing result"))
            });
        }
    }

    pub fn cancel(&mut self) {
        self.process.terminate();
    }

    pub fn close(&mut self) -> McpResult<()> {
        self.process.terminate();
        Ok(())
    }

    fn stderr_text(&self) -> String {
        let text = self
            .stderr
            .lock()
            .map(|stderr| stderr.text())
            .unwrap_or_else(|_| "stderr capture unavailable".to_owned());
        redact_sensitive_text(&text, &self.sensitive_values)
    }
}
pub(super) fn read_stdout_lines(
    mut stdout: std::process::ChildStdout,
    sender: mpsc::SyncSender<StdoutEvent>,
) {
    let mut buffer = [0u8; 16 * 1024];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let read = match stdout.read(&mut buffer) {
            Ok(0) => {
                if oversized {
                    let _ = sender.send(StdoutEvent::Oversized);
                } else if !line.is_empty() {
                    let _ = sender.send(StdoutEvent::Line(line));
                }
                let _ = sender.send(StdoutEvent::Eof);
                return;
            }
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(StdoutEvent::Error(error.to_string()));
                return;
            }
        };
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                let event = if oversized {
                    StdoutEvent::Oversized
                } else {
                    StdoutEvent::Line(std::mem::take(&mut line))
                };
                if sender.send(event).is_err() {
                    return;
                }
                line.clear();
                oversized = false;
            } else if line.len() < MCP_MAX_MESSAGE_BYTES {
                line.push(*byte);
            } else {
                oversized = true;
            }
        }
    }
}

pub(super) fn drain_stderr(
    mut stderr: std::process::ChildStderr,
    capture: Arc<Mutex<BoundedStderr>>,
) {
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let Ok(mut capture) = capture.lock() else {
            return;
        };
        capture.total = capture.total.saturating_add(read as u64);
        let remaining = MCP_MAX_STDERR_BYTES.saturating_sub(capture.bytes.len());
        capture
            .bytes
            .extend_from_slice(&buffer[..read.min(remaining)]);
    }
}
