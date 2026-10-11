//! Bounded Bubblewrap setup handshake. The command stays gated until init is pinned.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::launch_control::LaunchControl;
use crate::ProcessError;

pub(crate) fn isolation_error(error: io::Error) -> ProcessError {
    ProcessError::Isolation(format!(
        "Linux managed processes require trusted /usr/bin/bwrap and /usr/bin/perl, user/PID namespaces and pidfd support: {error}"
    ))
}

pub(crate) struct Handshake {
    info: File,
    release: File,
    inherited: Option<[OwnedFd; 2]>,
}

pub(crate) fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // Owned descriptors are close-on-exec everywhere except the explicit child ends.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

impl Handshake {
    pub(crate) fn new(command: &mut Command) -> io::Result<Self> {
        let (info_read, info_write) = pipe()?;
        let (gate_read, gate_write) = pipe()?;
        if unsafe { libc::fcntl(info_read.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let fds = [info_write.as_raw_fd(), gate_read.as_raw_fd()];
        command.args([
            "--info-fd",
            &fds[0].to_string(),
            "--block-fd",
            &fds[1].to_string(),
        ]);
        // Only async-signal-safe fcntl operations run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                for fd in fds {
                    if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        Ok(Self {
            info: File::from(info_read),
            release: File::from(gate_write),
            inherited: Some([info_write, gate_read]),
        })
    }

    pub(crate) fn pin_init(
        &mut self,
        deadline: Instant,
        control: &LaunchControl<'_>,
    ) -> Result<OwnedFd, ProcessError> {
        // EOF now means Bubblewrap really closed its writer, not our duplicate.
        self.inherited.take();
        let mut bytes = Vec::new();
        loop {
            control.check()?;
            if Instant::now() >= deadline {
                return Err(isolation_error(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Bubblewrap setup deadline",
                )));
            }
            let mut chunk = [0; 256];
            match self.info.read(&mut chunk) {
                Ok(0) => {
                    return Err(isolation_error(io::Error::other(
                        "Bubblewrap failed before PID namespace admission",
                    )))
                }
                Ok(count) => {
                    if bytes.len() + count > 4096 {
                        return Err(isolation_error(io::Error::other(
                            "Bubblewrap setup response exceeded 4096 bytes",
                        )));
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.ends_with(b"}\n") {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(isolation_error(error)),
            }
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| isolation_error(io::Error::other(error)))?;
        let pid = text
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("\"child-pid\":")?
                    .trim()
                    .trim_end_matches(',')
                    .parse::<libc::pid_t>()
                    .ok()
            })
            .filter(|pid| *pid > 1)
            .ok_or_else(|| {
                isolation_error(io::Error::other("invalid Bubblewrap namespace init PID"))
            })?;
        // No plugin code has run: --block-fd still holds the trusted init in setup.
        // A pidfd pins identity across exit/reaping; never later signal a recycled PID.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd < 0 {
            return Err(isolation_error(io::Error::last_os_error()));
        }
        let init = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        // Reject hosts that permit pidfd_open but block the signalling syscall.
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                init.as_raw_fd(),
                0,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        } < 0
        {
            return Err(isolation_error(io::Error::last_os_error()));
        }
        Ok(init)
    }

    pub(crate) fn release(&mut self) -> io::Result<()> {
        self.release.write_all(b"1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn stalled_admission_obeys_deadline_and_cancellation() {
        for cancel in [false, true] {
            let mut handshake = Handshake::new(&mut Command::new("/usr/bin/bwrap")).unwrap();
            // Hold the producer open without sending info: model stalled trusted
            // setup rather than EOF, without running or trusting a fake supervisor.
            let _producer = handshake.inherited.as_ref().unwrap()[0]
                .try_clone()
                .unwrap();
            let token = AtomicBool::new(false);
            let start = Instant::now();
            let control = LaunchControl {
                started: Some(start),
                deadline: (!cancel).then_some(start + Duration::from_millis(20)),
                cancellation: Some(&token),
            };
            std::thread::scope(|scope| {
                if cancel {
                    scope.spawn(|| {
                        std::thread::sleep(Duration::from_millis(20));
                        token.store(true, Ordering::Release);
                    });
                }
                let error = handshake
                    .pin_init(start + Duration::from_secs(5), &control)
                    .unwrap_err();
                if cancel {
                    assert!(matches!(error, ProcessError::Cancelled { .. }));
                } else {
                    assert!(matches!(error, ProcessError::Timeout { .. }));
                }
                assert!(start.elapsed() < Duration::from_secs(1));
            });
        }
    }
}
