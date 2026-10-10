use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::command::supervised_command;
use crate::isolation::ProcessIsolation;
use crate::{run_with_input, ManagedChild, ProcessError, ProcessSpec};

struct FixtureRoot(PathBuf);

impl FixtureRoot {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "loom-windows-spawn-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn spec(&self, action: &str) -> ProcessSpec {
        let mut spec = ProcessSpec::new(std::env::current_exe().unwrap());
        spec.args = vec![
            "--exact".into(),
            "windows_spawn_tests::child_fixture".into(),
            "--nocapture".into(),
        ];
        spec.current_dir = Some(self.0.clone());
        spec.env
            .insert("LOOM_PROCESS_SPAWN_FIXTURE".into(), action.into());
        spec.env.insert(
            "LOOM_PROCESS_SPAWN_MARKER".into(),
            self.marker().display().to_string(),
        );
        spec.limits.timeout = Duration::from_secs(15);
        spec
    }

    fn marker(&self) -> PathBuf {
        self.0.join("executed.txt")
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn child_fixture() {
    let Ok(action) = std::env::var("LOOM_PROCESS_SPAWN_FIXTURE") else {
        return;
    };
    match action.as_str() {
        "marker" => std::fs::write(
            std::env::var_os("LOOM_PROCESS_SPAWN_MARKER").unwrap(),
            "executed",
        )
        .unwrap(),
        "spawn" => {
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "windows_spawn_tests::child_fixture",
                    "--nocapture",
                ])
                .env("LOOM_PROCESS_SPAWN_FIXTURE", "marker")
                .status();
            println!(
                "{}",
                if status.is_ok_and(|s| s.success()) {
                    "spawn-allowed"
                } else {
                    "spawn-denied"
                }
            );
        }
        _ => panic!("unknown fixture action"),
    }
}

#[test]
fn suspended_child_executes_only_after_job_assignment_and_resume() {
    let root = FixtureRoot::new();
    let spec = root.spec("marker");
    let mut child = ChildGuard(supervised_command(&spec).spawn().unwrap());
    thread::sleep(Duration::from_millis(150));
    assert!(!root.marker().exists(), "child executed before isolation");
    let isolation = ProcessIsolation::attach(&child.0, &spec.limits).unwrap();
    // Resume also requires an exact initial suspend count of one, so this fails
    // deterministically if CREATE_SUSPENDED is accidentally removed.
    crate::windows_spawn::resume_assigned_child(&child.0, &isolation).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.0.try_wait().unwrap().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(root.marker()).unwrap(), "executed");
}

#[test]
fn failed_job_assignment_never_executes_either_launch_path() {
    let root = FixtureRoot::new();
    let mut spec = root.spec("marker");
    spec.limits.max_processes = Some(0);
    assert!(matches!(
        run_with_input(&spec, b""),
        Err(ProcessError::Isolation(_))
    ));
    assert!(matches!(
        ManagedChild::spawn(&spec),
        Err(ProcessError::Isolation(_))
    ));
    assert!(!root.marker().exists());
}

fn managed_stdout(spec: &ProcessSpec) -> String {
    let (mut child, pipes) = ManagedChild::spawn(spec).unwrap();
    drop(pipes.stdin);
    let deadline = Instant::now() + spec.limits.timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    child.terminate();
    assert!(
        status.is_some_and(|status| status.success()),
        "fixture did not complete"
    );
    let mut output = String::new();
    pipes
        .stdout
        .take(16 * 1024)
        .read_to_string(&mut output)
        .unwrap();
    output
}

#[test]
fn both_launch_paths_enforce_process_count_before_first_descendant() {
    let root = FixtureRoot::new();
    let mut spec = root.spec("spawn");
    spec.limits.max_processes = Some(1);
    let output = run_with_input(&spec, b"").unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("spawn-denied"));
    assert!(!root.marker().exists());
    assert!(managed_stdout(&spec).contains("spawn-denied"));
    assert!(!root.marker().exists());

    // The fixture can actually spawn when the job budget permits it.
    spec.limits.max_processes = Some(2);
    assert!(managed_stdout(&spec).contains("spawn-allowed"));
    assert_eq!(std::fs::read_to_string(root.marker()).unwrap(), "executed");
}
