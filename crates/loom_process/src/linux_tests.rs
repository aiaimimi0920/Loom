use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::{run_with_input, run_with_input_cancellable, ManagedChild, ProcessError, ProcessSpec};

fn escape_spec(mode: &str) -> ProcessSpec {
    let mut spec = ProcessSpec::new("/usr/bin/python3");
    spec.args = vec![
        "-u".into(),
        "-c".into(),
        r#"
import ctypes, os, sys, time
read, write = os.pipe()
if os.fork() == 0:
    os.close(read)
    os.setsid()
    if os.fork() != 0:
        os._exit(0)
    ctypes.CDLL(None).prctl(1, 0, 0, 0, 0)
    print('escaped-descendant-ready', flush=True)
    if os.environ.get('LOOM_TEST_READY_FILE'):
        with open(os.environ['LOOM_TEST_READY_FILE'], 'w') as ready:
            ready.write('ready')
    if sys.argv[1] == 'flood':
        print('x' * 200000, flush=True)
    os.write(write, b'1')
    os.close(write)
    time.sleep(60)
else:
    os.close(write)
    os.read(read, 1)
    os.close(read)
    if sys.argv[1] != 'exit':
        time.sleep(60)
"#
        .into(),
        mode.into(),
    ];
    spec.limits.timeout = Duration::from_secs(5);
    spec
}

#[test]
fn escaped_double_fork_pipes_close_after_leader_exit() {
    let start = Instant::now();
    let output = run_with_input(&escape_spec("exit"), b"").unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("escaped-descendant-ready"));
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn escaped_descendant_cannot_hold_stdin_or_output_after_timeout() {
    let mut spec = escape_spec("wait");
    spec.limits.timeout = Duration::from_secs(1);
    let start = Instant::now();
    let error = run_with_input(&spec, &vec![0; 8 * 1024 * 1024]).unwrap_err();
    let ProcessError::Timeout { stdout, .. } = error else {
        panic!("{error:?}")
    };
    assert!(String::from_utf8_lossy(&stdout).contains("escaped-descendant-ready"));
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn managed_drop_kills_setsid_descendants_before_pipe_join() {
    let start = Instant::now();
    let (child, pipes) = ManagedChild::spawn(&escape_spec("wait")).unwrap();
    let mut output = BufReader::new(pipes.stdout);
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "escaped-descendant-ready");
    drop(child);
    let mut remainder = Vec::new();
    output.read_to_end(&mut remainder).unwrap();
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn cancellation_closes_escaped_descendant_pipes() {
    let ready_path = std::env::temp_dir().join(format!(
        "loom-namespace-ready-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ready_path)
        .unwrap();
    let mut spec = escape_spec("wait");
    spec.env.insert(
        "LOOM_TEST_READY_FILE".into(),
        ready_path.to_str().unwrap().into(),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancel);
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(run_with_input_cancellable(&spec, b"", &signal))
            .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while std::fs::metadata(&ready_path).unwrap().len() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let ready = std::fs::metadata(&ready_path).unwrap().len() > 0;
    cancel.store(true, Ordering::Release);
    let result = rx
        .recv_timeout(Duration::from_secs(4))
        .expect("cancel cleanup deadline");
    assert!(
        matches!(result, Err(ProcessError::Cancelled { .. })),
        "{result:?}"
    );
    worker.join().unwrap();
    std::fs::remove_file(ready_path).unwrap();
    assert!(ready, "escaped child must run before cancellation");
}

#[test]
fn output_limit_kills_escaped_descendant() {
    let mut spec = escape_spec("flood");
    spec.limits.stdout_bytes = 1024;
    let start = Instant::now();
    let error = run_with_input(&spec, b"").unwrap_err();
    assert!(
        matches!(error, ProcessError::OutputLimit { .. }),
        "{error:?}"
    );
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn nested_managed_runtime_keeps_independent_namespace() {
    let mut spec = ProcessSpec::new(std::env::current_exe().unwrap());
    spec.args = vec![
        "--exact".into(),
        "linux_tests::nested_worker".into(),
        "--nocapture".into(),
    ];
    spec.env
        .insert("LOOM_TEST_NAMESPACE_WORKER".into(), "1".into());
    let output = run_with_input(&spec, b"").unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("NESTED_NAMESPACE_OK"));
}

#[test]
fn nested_worker() {
    if std::env::var("LOOM_TEST_NAMESPACE_WORKER").as_deref() != Ok("1") {
        return;
    }
    // A nested Loom runtime creates another independent namespace, not a shared PGID.
    let output = run_with_input(&escape_spec("exit"), b"").unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("escaped-descendant-ready"));
    println!("NESTED_NAMESPACE_OK");
}

#[test]
fn missing_supervisor_fails_closed_without_running_command() {
    let result = crate::linux_spawn::spawn_using(
        &escape_spec("wait"),
        std::path::Path::new("/loom-missing-bwrap"),
        &crate::launch_control::LaunchControl::default(),
    );
    assert!(matches!(result, Err(ProcessError::Isolation(_))));
}

#[test]
fn relative_working_directory_preserves_command_semantics() {
    let mut spec = ProcessSpec::new("/bin/pwd");
    spec.current_dir = Some(std::path::PathBuf::from("."));
    let output = run_with_input(&spec, b"").unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        std::env::current_dir()
            .unwrap()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
}

#[test]
fn exec_errors_are_spawn_errors_but_target_exit_127_is_not() {
    for (program, expected) in [
        ("/loom-target-does-not-exist", std::io::ErrorKind::NotFound),
        ("/etc/passwd", std::io::ErrorKind::PermissionDenied),
    ] {
        let result = ManagedChild::spawn(&ProcessSpec::new(program));
        assert!(matches!(result, Err(ProcessError::Spawn(ref error)) if error.kind() == expected));
    }
    let mut spec = ProcessSpec::new("/bin/sh");
    spec.args = vec!["-c".into(), "exit 127".into()];
    assert_eq!(run_with_input(&spec, b"").unwrap().status.code(), Some(127));
}

#[test]
fn plugin_interpreter_environment_cannot_run_in_exec_shim() {
    let mut spec = ProcessSpec::new("/bin/true");
    spec.env
        .insert("PERL5OPT".into(), "-Mnonexistent_plugin_module".into());
    assert!(run_with_input(&spec, b"").unwrap().status.success());
}

#[test]
fn expired_or_cancelled_setup_never_releases_target() {
    use crate::launch_control::LaunchControl;
    let marker = std::env::temp_dir().join(format!("loom-launch-gate-{}", std::process::id()));
    assert!(!marker.exists());
    let mut spec = ProcessSpec::new("/usr/bin/touch");
    spec.args = vec![marker.to_str().unwrap().into()];
    let cancelled = AtomicBool::new(true);
    for control in [
        LaunchControl {
            started: Some(Instant::now()),
            deadline: Some(Instant::now()),
            cancellation: None,
        },
        LaunchControl {
            cancellation: Some(&cancelled),
            ..LaunchControl::default()
        },
    ] {
        // Exercise the final gate, not just spawn_controlled's initial precheck.
        let result = crate::linux_spawn::spawn_using(
            &spec,
            std::path::Path::new("/usr/bin/bwrap"),
            &control,
        );
        assert!(matches!(
            result,
            Err(ProcessError::Timeout { .. } | ProcessError::Cancelled { .. })
        ));
        assert!(!marker.exists(), "expired target executed");
    }
}
