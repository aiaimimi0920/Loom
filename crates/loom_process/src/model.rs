//! Public process specifications, limits, and successful output contracts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Duration;

use loom_protocol::ExecutionDiagnostics;

#[derive(Clone, Debug)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    /// Windows job-object memory limit. Unix process groups currently provide no equivalent
    /// tree-wide memory boundary, so this value is not enforced there.
    pub memory_bytes: Option<usize>,
    /// Windows job-object active-process limit. Unix process groups currently provide no safe
    /// per-tree equivalent, so this value is not enforced there.
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
