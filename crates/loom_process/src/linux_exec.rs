//! CLOEXEC acknowledgement separates target exec failure from its eventual exit code.

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::launch_control::LaunchControl;
use crate::linux_handshake::pipe;
use crate::ProcessError;

// The distribution's perl-base is already a Loom runtime dependency. This tiny
// exec-only shim runs with the scrubbed supervisor environment, never plugin
// loader/interpreter variables. Arguments are data, not interpolated source.
// The private descriptor closes atomically on successful exec. Bubblewrap passes
// inherited descriptors to its target but closes them in its monitor and reaper.
const EXEC_SHIM: &str = r#"
my $fd = shift @ARGV;
open(my $status, '>&=' . $fd) or die "exec acknowledgement fd";
fcntl($status, 2, 1) or die "exec acknowledgement CLOEXEC";
syswrite($status, 'R') == 1 or die "exec acknowledgement ready";
my $count = shift @ARGV;
%ENV = ();
for (1 .. $count) {
    my $key = shift @ARGV;
    $ENV{$key} = shift @ARGV;
}
exec { $ARGV[0] } @ARGV;
my $error = 0 + $!;
syswrite($status, pack('N', $error));
exit 126;
"#;

pub(crate) struct ExecAck {
    read: File,
    inherited: Option<OwnedFd>,
}

impl ExecAck {
    pub(crate) fn prepare(command: &mut Command, target: &Command) -> io::Result<Self> {
        let (read, write) = pipe()?;
        if unsafe { libc::fcntl(read.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = write.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command.args(["--", "/usr/bin/perl", "-f", "-e", EXEC_SHIM, "--"]);
        command.arg(fd.to_string());
        let environment = target
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect::<Vec<_>>();
        command.arg(environment.len().to_string());
        for (key, value) in environment {
            command.arg(key).arg(value);
        }
        command.arg(target.get_program()).args(target.get_args());
        Ok(Self {
            read: File::from(read),
            inherited: Some(write),
        })
    }

    pub(crate) fn wait(
        &mut self,
        control: &LaunchControl<'_>,
        deadline: Instant,
    ) -> Result<(), ProcessError> {
        self.inherited.take();
        let mut bytes = Vec::new();
        loop {
            control.check()?;
            if Instant::now() >= deadline {
                return Err(ProcessError::Isolation(
                    "target exec acknowledgement deadline".into(),
                ));
            }
            let mut chunk = [0; 6];
            match self.read.read(&mut chunk) {
                Ok(0) if bytes == b"R" => return Ok(()),
                Ok(0) => {
                    return Err(ProcessError::Isolation(
                        "target exec acknowledgement closed before success".into(),
                    ))
                }
                Ok(count) => {
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.len() == 5 && bytes[0] == b'R' {
                        let errno = i32::from_be_bytes(bytes[1..5].try_into().unwrap());
                        return Err(ProcessError::Spawn(io::Error::from_raw_os_error(errno)));
                    }
                    if bytes.len() > 5 || bytes[0] != b'R' {
                        return Err(ProcessError::Isolation(
                            "invalid target exec acknowledgement".into(),
                        ));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(ProcessError::Spawn(error)),
            }
        }
    }
}
