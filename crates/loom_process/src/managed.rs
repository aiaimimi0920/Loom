//! Long-lived managed child handles used by streaming protocol hosts.

use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, ExitStatus};

use crate::error::ProcessError;
use crate::isolation::ProcessIsolation;
use crate::model::ProcessSpec;
use crate::spawn::spawn_isolated;

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
        let (mut child, isolation) = spawn_isolated(spec)?;
        let stdin = child.stdin.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            let _ = child.wait();
            ProcessError::MissingPipe("stdin")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            let _ = child.wait();
            ProcessError::MissingPipe("stdout")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            isolation.kill_tree(&mut child);
            let _ = child.wait();
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
        // The leader may have exited while descendants still hold inherited pipes. Always tear
        // down the isolation group before protocol hosts join their pipe threads.
        self.isolation.kill_tree(&mut self.child);
        let _ = self.child.wait();
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
