//! Platform-specific process-tree isolation, termination, and accounting.

use std::process::Command;

use crate::model::ProcessLimits;

pub(crate) struct ProcessIsolation {
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    process_group: i32,
}

impl ProcessIsolation {
    pub(crate) fn attach(
        child: &std::process::Child,
        limits: &ProcessLimits,
    ) -> Result<Self, String> {
        attach_process_isolation(child, limits)
    }

    pub(crate) fn kill_tree(&self, child: &mut std::process::Child) {
        kill_process_tree(self, child);
    }

    /// Peak memory charged to this isolation group so far, in bytes, or `None` when the platform
    /// keeps no such counter. Valid only while the group is open.
    pub(crate) fn peak_memory_bytes(&self) -> Option<u64> {
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
pub(crate) fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
pub(crate) fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn configure_process_group(_command: &mut Command) {}

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
