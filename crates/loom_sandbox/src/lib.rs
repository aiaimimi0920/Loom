//! Safe execution contracts for Loom.

use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

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
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    allowed_commands: BTreeSet<String>,
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
}

/// Captured process output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SandboxOutput {
    pub status_success: bool,
    pub stdout: String,
    pub stderr: String,
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

        let output = Command::new(command.program())
            .args(command.args())
            .output()
            .map_err(|source| SandboxError::Io {
                program: command.program().to_owned(),
                source,
            })?;

        Ok(SandboxOutput {
            status_success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
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

        let mut child = Command::new(command.program())
            .args(command.args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| SandboxError::Io {
                program: command.program().to_owned(),
                source,
            })?;

        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin
                .write_all(stdin.as_bytes())
                .map_err(|source| SandboxError::Io {
                    program: command.program().to_owned(),
                    source,
                })?;
        }

        let output = child
            .wait_with_output()
            .map_err(|source| SandboxError::Io {
                program: command.program().to_owned(),
                source,
            })?;

        Ok(SandboxOutput {
            status_success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
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
}
