//! No managed child may execute before its platform isolation is established.

use std::process::Child;

use crate::command::supervised_command;
use crate::error::ProcessError;
use crate::isolation::ProcessIsolation;
use crate::model::ProcessSpec;

pub(crate) fn spawn_isolated(
    spec: &ProcessSpec,
) -> Result<(Child, ProcessIsolation), ProcessError> {
    let mut child = supervised_command(spec)
        .spawn()
        .map_err(ProcessError::Spawn)?;
    let isolation = ProcessIsolation::attach(&child, &spec.limits).map_err(|error| {
        let _ = child.kill();
        let _ = child.wait();
        ProcessError::Isolation(error)
    })?;
    #[cfg(windows)]
    if let Err(error) = crate::windows_spawn::resume_assigned_child(&child, &isolation) {
        isolation.kill_tree(&mut child);
        let _ = child.wait();
        return Err(ProcessError::Isolation(error.to_string()));
    }
    Ok((child, isolation))
}
