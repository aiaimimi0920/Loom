//! Recover the suspended primary thread using stable, documented Windows APIs.

use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Child;

use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::Threading::{
    GetProcessIdOfThread, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SUSPEND_RESUME,
};

use crate::isolation::ProcessIsolation;

fn primary_thread_id(process_id: u32) -> io::Result<u32> {
    // CREATE_SUSPENDED keeps the entry thread from creating any additional threads.
    // Reject ambiguity instead of guessing which thread may execute plugin code.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // Ownership begins only after the INVALID_HANDLE_VALUE check; all exits close it.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut found = None;
    let mut present = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
    while present != 0 {
        if entry.th32OwnerProcessID == process_id {
            if found.replace(entry.th32ThreadID).is_some() {
                return Err(io::Error::other(
                    "Suspended process has ambiguous primary thread",
                ));
            }
        }
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        present = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
        return Err(error);
    }
    found.ok_or_else(|| io::Error::other("Suspended process primary thread is unavailable"))
}

pub(crate) fn resume_assigned_child(
    child: &Child,
    _isolation: &ProcessIsolation,
) -> io::Result<()> {
    // Stable std::process::Child does not expose the primary thread handle.
    // Keep the child and assigned job alive while obtaining a verified thread handle.
    let thread_id = primary_thread_id(child.id())?;
    let thread = unsafe {
        OpenThread(
            THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
            0,
            thread_id,
        )
    };
    if thread.is_null() {
        return Err(io::Error::last_os_error());
    }
    let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
    let owner = unsafe { GetProcessIdOfThread(thread.as_raw_handle()) };
    if owner == 0 {
        return Err(io::Error::last_os_error());
    }
    if owner != child.id() {
        return Err(io::Error::other(
            "Suspended process thread ownership changed",
        ));
    }
    let previous_count = unsafe { ResumeThread(thread.as_raw_handle()) };
    match previous_count {
        1 => Ok(()),
        u32::MAX => Err(io::Error::last_os_error()),
        _ => Err(io::Error::other("Unexpected primary thread suspend count")),
    }
}
