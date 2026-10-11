//! No managed child may execute before its platform isolation is established.

use std::process::Child;

#[cfg(not(target_os = "linux"))]
use crate::command::supervised_command;
use crate::error::ProcessError;
use crate::isolation::ProcessIsolation;
use crate::launch_control::LaunchControl;
use crate::model::ProcessSpec;

pub(crate) fn spawn_isolated(
    spec: &ProcessSpec,
) -> Result<(Child, ProcessIsolation), ProcessError> {
    spawn_controlled(spec, &LaunchControl::default())
}

pub(crate) fn spawn_controlled(
    spec: &ProcessSpec,
    control: &LaunchControl<'_>,
) -> Result<(Child, ProcessIsolation), ProcessError> {
    control.check()?;
    #[cfg(target_os = "linux")]
    return crate::linux_spawn::spawn(spec, control);
    #[cfg(not(target_os = "linux"))]
    spawn_platform(spec)
}

#[cfg(not(target_os = "linux"))]
fn spawn_platform(spec: &ProcessSpec) -> Result<(Child, ProcessIsolation), ProcessError> {
    if !cfg!(windows) {
        return Err(ProcessError::Isolation(
            "managed process containment is unsupported on this OS".into(),
        ));
    }
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
