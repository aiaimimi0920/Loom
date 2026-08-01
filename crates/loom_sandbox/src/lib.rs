//! Safe execution contracts for Loom.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of the sandbox crate.
pub const LOOM_SANDBOX_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Sandbox execution errors.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("command `{program}` is denied by sandbox policy")]
    Denied { program: String },
    #[error("command `{program}` failed to execute: {source}")]
    Io {
        program: String,
        source: std::io::Error,
    },
    #[error("command `{program}` was stopped by sandbox resource policy: {reason}")]
    Resource { program: String, reason: String },
}

/// Result alias for sandbox operations.
pub type SandboxResult<T> = Result<T, SandboxError>;

/// Process command requested by Loom tool execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxCommand {
    program: String,
    args: Vec<String>,
}

impl SandboxCommand {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

/// Deny-by-default execution policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    allowed_commands: BTreeSet<String>,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
    #[serde(default = "default_output_bytes")]
    stdout_bytes: usize,
    #[serde(default = "default_output_bytes")]
    stderr_bytes: usize,
    #[serde(default = "default_memory_bytes")]
    memory_bytes: Option<usize>,
    #[serde(default = "default_max_processes")]
    max_processes: Option<u32>,
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_output_bytes() -> usize {
    8 * 1024 * 1024
}

fn default_memory_bytes() -> Option<usize> {
    Some(512 * 1024 * 1024)
}

fn default_max_processes() -> Option<u32> {
    Some(4)
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            allowed_commands: BTreeSet::new(),
            timeout_ms: default_timeout_ms(),
            stdout_bytes: default_output_bytes(),
            stderr_bytes: default_output_bytes(),
            memory_bytes: default_memory_bytes(),
            max_processes: default_max_processes(),
        }
    }
}

impl SandboxPolicy {
    #[must_use]
    pub fn allow_command(mut self, program: impl Into<String>) -> Self {
        self.allowed_commands.insert(program.into());
        self
    }

    #[must_use]
    pub fn permits(&self, command: &SandboxCommand) -> bool {
        self.allowed_commands.contains(command.program())
    }

    #[must_use]
    pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms.max(1);
        self
    }

    #[must_use]
    pub fn output_limits(mut self, stdout_bytes: usize, stderr_bytes: usize) -> Self {
        self.stdout_bytes = stdout_bytes.max(1);
        self.stderr_bytes = stderr_bytes.max(1);
        self
    }
}

/// Captured process output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxOutput {
    pub status_success: bool,
    pub stdout: String,
    pub stderr: String,
    pub diagnostics: loom_protocol::ExecutionDiagnostics,
}

/// Safe execution facade. Default construction denies every command.
#[derive(Clone, Debug, Default)]
pub struct Sandbox {
    policy: SandboxPolicy,
}

impl Sandbox {
    #[must_use]
    pub fn new(policy: SandboxPolicy) -> Self {
        Self { policy }
    }

    pub fn execute(&self, command: &SandboxCommand) -> SandboxResult<SandboxOutput> {
        if !self.policy.permits(command) {
            return Err(SandboxError::Denied {
                program: command.program().to_owned(),
            });
        }

        self.execute_input(command, &[])
    }

    pub fn execute_with_stdin(
        &self,
        command: &SandboxCommand,
        stdin: &str,
    ) -> SandboxResult<SandboxOutput> {
        if !self.policy.permits(command) {
            return Err(SandboxError::Denied {
                program: command.program().to_owned(),
            });
        }

        self.execute_input(command, stdin.as_bytes())
    }

    fn execute_input(
        &self,
        command: &SandboxCommand,
        stdin: &[u8],
    ) -> SandboxResult<SandboxOutput> {
        let mut spec = loom_process::ProcessSpec::new(command.program());
        spec.args = command.args().to_vec();
        spec.limits.timeout = Duration::from_millis(self.policy.timeout_ms.max(1));
        spec.limits.stdout_bytes = self.policy.stdout_bytes;
        spec.limits.stderr_bytes = self.policy.stderr_bytes;
        spec.limits.memory_bytes = self.policy.memory_bytes;
        spec.limits.max_processes = self.policy.max_processes;
        let output = loom_process::run_with_input(&spec, stdin).map_err(|error| match error {
            loom_process::ProcessError::Spawn(source) => SandboxError::Io {
                program: command.program().to_owned(),
                source,
            },
            error => SandboxError::Resource {
                program: command.program().to_owned(),
                reason: error.to_string(),
            },
        })?;
        Ok(SandboxOutput {
            status_success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            diagnostics: output.diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_denies_commands_without_running_them() {
        let sandbox = Sandbox::default();
        let command = SandboxCommand::new("definitely-not-a-real-command-neuro-loom");

        let error = sandbox
            .execute(&command)
            .expect_err("default policy must deny");

        assert!(matches!(error, SandboxError::Denied { .. }));
    }

    #[test]
    fn explicit_allow_policy_runs_safe_fixture_command() {
        let command = safe_echo_command();
        let sandbox = Sandbox::new(SandboxPolicy::default().allow_command(command.program()));

        let output = sandbox.execute(&command).expect("allowed command runs");

        assert!(output.status_success);
        assert!(output.stdout.contains("loom-sandbox-fixture"));
    }

    #[test]
    fn execute_with_stdin_is_denied_by_default_and_feeds_allowed_commands() {
        let command = safe_stdin_command();
        let denied = Sandbox::default()
            .execute_with_stdin(&command, "loom-sandbox-stdin\n")
            .expect_err("default policy denies stdin command");
        assert!(matches!(denied, SandboxError::Denied { .. }));

        let sandbox = Sandbox::new(SandboxPolicy::default().allow_command(command.program()));
        let output = sandbox
            .execute_with_stdin(&command, "loom-sandbox-stdin\n")
            .expect("allowed stdin command runs");

        assert!(output.status_success);
        assert!(output.stdout.contains("loom-sandbox-stdin"));
    }

    #[test]
    fn sandbox_enforces_timeout_and_output_limits() {
        let slow = slow_command();
        let sandbox = Sandbox::new(
            SandboxPolicy::default()
                .allow_command(slow.program())
                .timeout_ms(100),
        );
        assert!(matches!(
            sandbox.execute(&slow),
            Err(SandboxError::Resource { .. })
        ));

        let noisy = noisy_command();
        let sandbox = Sandbox::new(
            SandboxPolicy::default()
                .allow_command(noisy.program())
                .output_limits(1024, 1024),
        );
        assert!(matches!(
            sandbox.execute(&noisy),
            Err(SandboxError::Resource { .. })
        ));
    }

    #[cfg(windows)]
    fn safe_echo_command() -> SandboxCommand {
        SandboxCommand::new("cmd")
            .arg("/C")
            .arg("echo loom-sandbox-fixture")
    }

    #[cfg(not(windows))]
    fn safe_echo_command() -> SandboxCommand {
        SandboxCommand::new("printf").arg("loom-sandbox-fixture")
    }

    #[cfg(windows)]
    fn safe_stdin_command() -> SandboxCommand {
        SandboxCommand::new("cmd")
            .arg("/C")
            .arg("findstr loom-sandbox-stdin")
    }

    #[cfg(not(windows))]
    fn safe_stdin_command() -> SandboxCommand {
        SandboxCommand::new("grep").arg("loom-sandbox-stdin")
    }

    #[cfg(windows)]
    fn slow_command() -> SandboxCommand {
        SandboxCommand::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("Start-Sleep -Seconds 5")
    }

    #[cfg(not(windows))]
    fn slow_command() -> SandboxCommand {
        SandboxCommand::new("sh").arg("-c").arg("sleep 5")
    }

    #[cfg(windows)]
    fn noisy_command() -> SandboxCommand {
        SandboxCommand::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("[Console]::Out.Write(('x' * 100000))")
    }

    #[cfg(not(windows))]
    fn noisy_command() -> SandboxCommand {
        SandboxCommand::new("sh")
            .arg("-c")
            .arg("head -c 100000 /dev/zero")
    }
}
