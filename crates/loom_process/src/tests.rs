use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::path::{Path, PathBuf};

use crate::command::inherited_runtime_environment;
#[cfg(windows)]
use crate::path::process_path;
use crate::{run_with_input, run_with_input_cancellable, ProcessError, ProcessSpec};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[cfg(windows)]
fn remove_windows_test_tree(path: &Path) {
    for attempt in 0..20 {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error)
                if attempt < 19
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::DirectoryNotEmpty
                    ) =>
            {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("remove Windows test tree: {error}"),
        }
    }
}

#[cfg(windows)]
#[test]
fn process_runs_from_a_deep_windows_working_directory() {
    use std::os::windows::ffi::OsStrExt;

    let root = std::env::temp_dir().join(format!(
        "loom-process-deep-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    let mut deep_dir = root.clone();
    while deep_dir.as_os_str().encode_wide().count() <= 280 {
        deep_dir.push("framework-package-segment-0123456789");
    }
    std::fs::create_dir_all(&deep_dir).expect("create deep working directory");
    let prepared_dir = process_path(&deep_dir);
    assert!(
        prepared_dir.as_os_str().encode_wide().count() < 248,
        "deep working directory was not shortened: {}",
        prepared_dir.display()
    );

    let command = PathBuf::from(std::env::var_os("ComSpec").expect("ComSpec"));
    let deep_program = deep_dir.join("framework-runtime.exe");
    std::fs::copy(command, &deep_program).expect("copy deep framework runtime");
    let mut spec = ProcessSpec::new(deep_program);
    spec.args = vec![
        "/D".to_owned(),
        "/C".to_owned(),
        "echo deep-path-ok".to_owned(),
    ];
    spec.current_dir = Some(deep_dir);
    spec.limits.timeout = Duration::from_secs(5);

    let output = run_with_input(&spec, b"").expect("run from deep working directory");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "deep-path-ok"
    );

    remove_windows_test_tree(&root);
}

#[test]
fn bounded_output_is_reported_as_a_resource_limit() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("powershell.exe");
        spec.args = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "[Console]::Out.Write(('x' * 200000))".to_owned(),
        ];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec![
            "-c".to_owned(),
            "head -c 200000 /dev/zero | tr '\\0' x".to_owned(),
        ];
        spec
    };
    spec.limits.stdout_bytes = 1024;
    spec.limits.stderr_bytes = 1024;
    let error = run_with_input(&spec, b"").expect_err("output limit");
    assert!(matches!(error, ProcessError::OutputLimit { .. }));
}

#[test]
fn normal_process_reports_diagnostics() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("cmd.exe");
        spec.args = vec!["/C".to_owned(), "set /p x=& echo ok".to_owned()];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "cat >/dev/null; printf ok".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::from_secs(5);
    let output = run_with_input(&spec, b"input\n").expect("process");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    assert!(output.diagnostics.duration_ms.is_some());
}

#[test]
fn extreme_timeout_does_not_overflow_the_deadline() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("cmd.exe");
        spec.args = vec!["/C".to_owned(), "echo ok".to_owned()];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "printf ok".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::MAX;

    let output = run_with_input(&spec, b"").expect("extreme timeout");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
}

#[test]
fn completed_leader_does_not_leave_inherited_pipes_open() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("powershell.exe");
        spec.args = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "$null = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30' -NoNewWindow -PassThru; Write-Output ok".to_owned(),
        ];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "(sleep 30) & printf ok".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::from_secs(10);

    let started = Instant::now();
    let output = run_with_input(&spec, b"").expect("leader with detached descendant");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(windows)]
#[test]
fn default_windows_job_allows_a_framework_host_to_spawn_its_runtime() {
    let mut spec = ProcessSpec::new("powershell.exe");
    spec.args = vec![
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        "& powershell.exe -NoProfile -Command 'Write-Output nested-ok'".to_owned(),
    ];
    spec.limits.timeout = Duration::from_secs(15);

    let output = run_with_input(&spec, b"").expect("nested framework runtime");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "nested-ok");
}

#[test]
fn timeout_terminates_a_child_that_never_reads_large_stdin() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("powershell.exe");
        spec.args = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "Start-Sleep -Seconds 30".to_owned(),
        ];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "sleep 30".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::from_millis(250);
    let input = vec![b'x'; 8 * 1024 * 1024];
    let started = Instant::now();
    let error = run_with_input(&spec, &input).expect_err("stdin-blocked child must time out");
    assert!(matches!(error, ProcessError::Timeout { .. }));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn cancellation_terminates_the_managed_process_tree() {
    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("powershell.exe");
        spec.args = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "Start-Sleep -Seconds 30".to_owned(),
        ];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "sleep 30".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::from_secs(30);
    let cancellation = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancellation);
    let toggler = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        signal.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let error = run_with_input_cancellable(&spec, b"", cancellation.as_ref())
        .expect_err("managed process must be cancelled");
    toggler.join().expect("cancellation toggler");
    assert!(matches!(error, ProcessError::Cancelled { .. }));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn supervised_process_does_not_inherit_host_secrets() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    const SECRET: &str = "loom-process-should-not-leak";
    let previous = std::env::var_os("LOOM_DAEMON_TOKEN");
    std::env::set_var("LOOM_DAEMON_TOKEN", SECRET);
    let result = std::panic::catch_unwind(|| {
        let mut spec = if cfg!(windows) {
            let mut spec = ProcessSpec::new("cmd.exe");
            spec.args = vec!["/C".to_owned(), "echo %LOOM_DAEMON_TOKEN%".to_owned()];
            spec
        } else {
            let mut spec = ProcessSpec::new("sh");
            spec.args = vec![
                "-c".to_owned(),
                "printf '%s' \"$LOOM_DAEMON_TOKEN\"".to_owned(),
            ];
            spec
        };
        spec.limits.timeout = Duration::from_secs(5);
        run_with_input(&spec, b"").expect("echo secret env")
    });
    match previous {
        Some(value) => std::env::set_var("LOOM_DAEMON_TOKEN", value),
        None => std::env::remove_var("LOOM_DAEMON_TOKEN"),
    }
    let output = result.expect("supervised secret probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(SECRET),
        "child inherited host secret: {stdout:?}"
    );
}

#[test]
fn supervised_process_keeps_required_runtime_environment() {
    let required = if cfg!(windows) {
        ["PATH", "SYSTEMROOT", "TEMP", "USERPROFILE", "APPDATA"]
    } else {
        ["PATH", "HOME", "TMPDIR", "LANG", "SHELL"]
    };
    let inherited = inherited_runtime_environment()
        .into_iter()
        .map(|(key, value)| (key.to_string_lossy().to_string(), value))
        .collect::<std::collections::HashMap<_, _>>();
    for name in required {
        if std::env::var_os(name).is_some() {
            assert!(
                inherited.keys().any(|key| key.eq_ignore_ascii_case(name)),
                "runtime environment dropped {name}"
            );
        }
    }
}

#[test]
fn supervised_process_inherits_the_image_search_loopback_seam() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    const SEAM: &str = "LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES";
    const UNRELATED: &str = "LOOM_IMAGE_SEARCH_UNRELATED_SETTING";
    let previous_seam = std::env::var_os(SEAM);
    std::env::set_var(SEAM, "1");
    std::env::set_var(UNRELATED, "1");
    let result = std::panic::catch_unwind(|| {
        let inherited = inherited_runtime_environment()
            .into_iter()
            .map(|(key, value)| (key.to_string_lossy().to_string(), value))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            inherited.get(SEAM).map(|value| value.to_string_lossy()),
            Some(std::borrow::Cow::Borrowed("1")),
            "the loopback test seam must survive the environment scrub, because the Art that \
             reads it runs two spawns deep"
        );
        // The seam is one named exception, not a `LOOM_`-prefixed passthrough.
        assert!(
            !inherited.contains_key(UNRELATED),
            "an unrelated Loom variable must still be scrubbed"
        );
    });
    std::env::remove_var(UNRELATED);
    match previous_seam {
        Some(value) => std::env::set_var(SEAM, value),
        None => std::env::remove_var(SEAM),
    }
    result.expect("loopback seam inheritance probe");
}

/// Loom's peak-memory budget for one framework process. Every framework Loom ships runs as a
/// supervised interpreter started once per execution, so the number this measures is the floor
/// under every art execution: whatever the interpreter costs before the work begins.
///
/// The child here is PowerShell because that is what Loom's own sample art frameworks run on,
/// and the budget is generous on purpose. It is not a claim about how much memory PowerShell
/// should need; it exists so that supervising a framework process cannot quietly start costing
/// hundreds of megabytes more than it does today.
#[test]
fn one_framework_process_stays_within_its_peak_memory_budget() {
    // Measured at 65,544,192 bytes (about 63 MiB) on 2026-08-22. The ceiling is well above that
    // but still below the 512 MiB the default limits enforce, since a job that hits the enforced
    // limit is killed and would never reach this assertion.
    const BUDGET_BYTES: u64 = 256 * 1024 * 1024;

    let mut spec = if cfg!(windows) {
        let mut spec = ProcessSpec::new("powershell.exe");
        spec.args = vec![
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            "Write-Output ok".to_owned(),
        ];
        spec
    } else {
        let mut spec = ProcessSpec::new("sh");
        spec.args = vec!["-c".to_owned(), "printf ok".to_owned()];
        spec
    };
    spec.limits.timeout = Duration::from_secs(30);

    let output = run_with_input(&spec, b"").expect("run one framework-shaped process");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");

    let Some(peak) = output.peak_memory_bytes else {
        // The platform keeps no such counter, so there is no measurement to compare. Passing
        // here reports the truth: nothing was measured, as opposed to a small number measured.
        println!("perf budget framework_process_peak_memory_bytes: not measured on this platform");
        return;
    };
    loom_perf::assert_within(
        "framework_process_peak_memory_bytes",
        "bytes",
        peak,
        BUDGET_BYTES,
    );
}
