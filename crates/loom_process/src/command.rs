//! Sanitized command construction and host-environment allowlisting.

use std::process::{Command, Stdio};

use crate::isolation::configure_process_group;
use crate::model::ProcessSpec;
use crate::path::process_path;

pub(crate) fn supervised_command(spec: &ProcessSpec) -> Command {
    let mut command = Command::new(process_path(&spec.program));
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(process_path(current_dir));
    }
    apply_supervised_environment(&mut command, spec);
    configure_process_group(&mut command);
    command
}

fn apply_supervised_environment(command: &mut Command, spec: &ProcessSpec) {
    command.env_clear();
    for (key, value) in inherited_runtime_environment() {
        command.env(key, value);
    }
    command.envs(&spec.env);
}

/// The environment a managed process inherits, filtered down to an allowlist so that host secrets in
/// the Loom process environment never reach a plugin.
///
/// `LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` is a test seam rather than a product capability. The
/// image-search sample Art refuses to download an image from a loopback address, which is correct for
/// the shipped Art but leaves the install-and-execute test with nowhere to serve its fixture image
/// from. With the variable set, that Art — and only that Art — permits a loopback address written
/// literally in an image URL; a hostname that resolves to loopback stays refused, as does every other
/// blocked range. It is allowlisted here because an Art runs two spawns deep, so the daemon, the
/// framework runtime host, and the Art entry each scrub the environment. No package can set it: an
/// Art runtime manifest declares a command and its arguments and nothing else, so only whoever
/// launches Loom can turn the seam on.
pub(crate) fn inherited_runtime_environment() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    #[cfg(windows)]
    const ALLOWED: &[&str] = &[
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "SYSTEMDRIVE",
        "COMSPEC",
        "TEMP",
        "TMP",
        "OS",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "USERNAME",
        "USERDOMAIN",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "PUBLIC",
        "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES",
    ];
    #[cfg(not(windows))]
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "TERM",
        "TZ",
        "SHELL",
        "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES",
    ];
    std::env::vars_os()
        .filter(|(key, _)| {
            let key = key.to_string_lossy();
            ALLOWED
                .iter()
                .any(|allowed| key.eq_ignore_ascii_case(allowed))
        })
        .collect()
}
