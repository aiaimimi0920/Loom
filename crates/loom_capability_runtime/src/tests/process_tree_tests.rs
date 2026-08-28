use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::json;

use super::*;

#[test]
fn timeout_removes_descendant_processes() {
    run_timeout_tree_case();
}

#[test]
fn cancel_removes_descendant_processes() {
    run_cancel_tree_case();
}

#[test]
fn crash_removes_descendant_processes() {
    run_crash_tree_case();
}

#[test]
fn deactivate_removes_descendant_processes() {
    run_deactivate_tree_case();
}

fn run_timeout_tree_case() {
    let (root, host, pid_file) = tree_host("tree-timeout", "tree");
    let error = host
        // The full test binary compiles and starts several fixtures concurrently on Windows.
        // Leave enough time for both descendants to publish their PIDs before testing teardown.
        .invoke(tree_invocation("tree-timeout", Duration::from_secs(5)))
        .expect_err("tree command must time out");
    assert!(matches!(error, CapabilityHostError::Timeout));
    assert_tree_stopped(&pid_file);
    host.deactivate_all();
    cleanup(&root);
}

fn run_cancel_tree_case() {
    let (root, host, pid_file) = tree_host("tree-cancel", "tree");
    let host = Arc::new(host);
    let invoking = Arc::clone(&host);
    let thread = thread::spawn(move || {
        invoking.invoke(tree_invocation("tree-cancel", Duration::from_secs(30)))
    });
    wait_for_tree(&pid_file);
    assert!(host.cancel_request("tree-cancel").expect("cancel tree"));
    assert!(thread.join().expect("join invocation").is_err());
    assert_tree_stopped(&pid_file);
    host.deactivate_all();
    cleanup(&root);
}

fn run_crash_tree_case() {
    let (root, host, pid_file) = tree_host("tree-crash", "crash-tree");
    host.invoke(tree_invocation("tree-crash", Duration::from_secs(5)))
        .expect_err("crashed runtime must fail");
    assert_tree_stopped(&pid_file);
    host.deactivate_all();
    cleanup(&root);
}

fn run_deactivate_tree_case() {
    let (root, host, pid_file) = tree_host("tree-deactivate", "tree");
    let host = Arc::new(host);
    let invoking = Arc::clone(&host);
    let thread = thread::spawn(move || {
        invoking.invoke(tree_invocation("tree-deactivate", Duration::from_secs(30)))
    });
    wait_for_tree(&pid_file);
    assert!(host
        .deactivate("publisher.example/fixture")
        .expect("deactivate tree runtime"));
    assert!(thread.join().expect("join invocation").is_err());
    assert_tree_stopped(&pid_file);
    cleanup(&root);
}

fn tree_host(label: &str, mode: &str) -> (PathBuf, CapabilityRuntimeHost, PathBuf) {
    let root = temp_root(label);
    let executable = compile_fixture(&root);
    let pid_file = root.join("descendants.pid");
    let pid_arg = pid_file.to_string_lossy().into_owned();
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits::default());
    host.activate(package_with_process_limit(
        &root,
        &executable,
        &[mode, &pid_arg],
        4,
    ))
    .expect("activate tree fixture");
    (root, host, pid_file)
}

fn tree_invocation(request_id: &str, timeout: Duration) -> CapabilityInvocation {
    CapabilityInvocation {
        request_id: request_id.to_owned(),
        command_id: "publisher.example/fixture.run".to_owned(),
        input: json!({}),
        target: None,
        resource_refs: Vec::new(),
        staged_resources: Vec::new(),
        user_gesture_token: None,
        timeout: Some(timeout),
    }
}

fn wait_for_tree(pid_file: &Path) -> Vec<u32> {
    for _ in 0..200 {
        let pids = read_pids(pid_file);
        if pids.len() >= 2 {
            return pids;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("descendant process IDs were not reported");
}

fn assert_tree_stopped(pid_file: &Path) {
    let pids = wait_for_tree(pid_file);
    for _ in 0..200 {
        if pids.iter().all(|pid| !process_exists(*pid)) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let alive = pids
        .into_iter()
        .filter(|pid| process_exists(*pid))
        .collect::<Vec<_>>();
    panic!("descendant processes survived teardown: {alive:?}");
}

fn read_pids(path: &Path) -> Vec<u32> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|value| value.trim().parse().ok())
        .collect()
}

fn process_exists(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("query process");
    String::from_utf8_lossy(&output.stdout)
        .split(|character: char| !character.is_ascii_digit())
        .any(|value| value == pid.to_string())
}
