//! Linux descendant containment: Bubblewrap PID namespace plus a pinned init pidfd.

use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::command::{inherited_runtime_environment, supervised_command};
use crate::error::ProcessError;
use crate::isolation::ProcessIsolation;
use crate::launch_control::LaunchControl;
use crate::linux_exec::ExecAck;
use crate::linux_handshake::{isolation_error, Handshake};
use crate::model::ProcessSpec;

pub(crate) fn spawn(
    spec: &ProcessSpec,
    control: &LaunchControl<'_>,
) -> Result<(Child, ProcessIsolation), ProcessError> {
    spawn_using(spec, Path::new("/usr/bin/bwrap"), control)
}

fn trusted_supervisor_file(path: &Path) -> io::Result<()> {
    let resolved = path.canonicalize()?;
    let root_uid = visible_root_uid()?;
    // Neither PATH nor plugin-provided environment can substitute the supervisor.
    for component in resolved.ancestors() {
        let metadata = component.metadata()?;
        if metadata.uid() != root_uid || metadata.mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "supervisor executable and ancestors must be root-owned and not group/world-writable",
            ));
        }
    }
    if !resolved.is_file() {
        return Err(io::Error::other(
            "supervisor executable is not a regular file",
        ));
    }
    Ok(())
}

fn visible_root_uid() -> io::Result<u32> {
    // Kernel ownership is translated in nested user namespaces. Root-owned host
    // files appear as the mapped root ID, or the kernel overflow UID when root
    // is unmapped; assuming uid==0 here would break every nested nonroot runtime.
    for line in std::fs::read_to_string("/proc/self/uid_map")?.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() == 3 && fields[1] == "0" {
            return fields[0].parse().map_err(io::Error::other);
        }
    }
    std::fs::read_to_string("/proc/sys/kernel/overflowuid")?
        .trim()
        .parse()
        .map_err(io::Error::other)
}

pub(crate) fn spawn_using(
    spec: &ProcessSpec,
    executable: &Path,
    control: &LaunchControl<'_>,
) -> Result<(Child, ProcessIsolation), ProcessError> {
    trusted_supervisor_file(executable).map_err(isolation_error)?;
    trusted_supervisor_file(Path::new("/usr/bin/perl")).map_err(isolation_error)?;
    let target = supervised_command(spec);
    let mut command = Command::new(executable);
    command.env_clear().envs(inherited_runtime_environment());
    command.args([
        "--unshare-user",
        "--unshare-pid",
        "--die-with-parent",
        "--cap-drop",
        "ALL",
    ]);
    // Mapping virtual UID 0 again requires CAP_SETFCAP in the parent namespace.
    // Use a nonzero virtual identity for a root host so nested runtimes still work
    // after capability removal. Files retain their original host ownership/access.
    if unsafe { libc::geteuid() } == 0 {
        command.args(["--uid", "65534"]);
    }
    if unsafe { libc::getegid() } == 0 {
        command.args(["--gid", "65534"]);
    }
    // This is a process-lifetime boundary, not a new filesystem/network sandbox.
    // A fresh proc mount hides ancestor PID namespaces from the plugin.
    command.args([
        "--bind",
        "/",
        "/",
        "--dev-bind",
        "/dev",
        "/dev",
        "--proc",
        "/proc",
    ]);
    if let Some(directory) = target.get_current_dir() {
        // Bubblewrap resolves --chdir after pivoting to /, unlike Command::current_dir.
        let directory = directory.canonicalize().map_err(ProcessError::Spawn)?;
        command
            .current_dir(&directory)
            .arg("--chdir")
            .arg(&directory);
    }
    let mut handshake = Handshake::new(&mut command).map_err(isolation_error)?;
    let mut exec_ack = ExecAck::prepare(&mut command, &target).map_err(isolation_error)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    let setup_deadline = Instant::now() + Duration::from_secs(5);
    let mut child = command.spawn().map_err(ProcessError::Spawn)?;
    // ManagedChild callers may use a 1 ms protocol-request timeout; that is not
    // the launch setup budget. Keep setup separately bounded, like OS spawning.
    let init = match handshake.pin_init(setup_deadline, control) {
        Ok(init) => init,
        Err(error) => {
            // No untrusted instruction ran. Kill trusted setup processes before
            // dropping the gate: Bubblewrap treats EOF on --block-fd as release.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let isolation = ProcessIsolation::from_linux(init);
    if let Err(error) = control.check() {
        isolation.kill_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    if let Err(error) = handshake.release() {
        isolation.kill_tree(&mut child);
        let _ = child.wait();
        return Err(isolation_error(error));
    }
    if let Err(error) = exec_ack.wait(control, setup_deadline) {
        isolation.kill_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    Ok((child, isolation))
}

pub(crate) fn kill_namespace(init: &OwnedFd) {
    // Killing PID namespace init makes the kernel kill every descendant, including
    // setsid/double-fork/nested-namespace children, regardless of inherited pipes.
    loop {
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                init.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
    }
}
