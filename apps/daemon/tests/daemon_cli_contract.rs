use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CTRL_BREAK_EVENT: u32 = 1;

#[cfg(windows)]
unsafe extern "system" {
    fn GenerateConsoleCtrlEvent(ctrl_event: u32, process_group_id: u32) -> i32;
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "loom-daemon-cli-contract-{}-{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn wait_for_manifest(manifest_path: &Path) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if manifest_path.exists() {
            let raw = fs::read_to_string(manifest_path).expect("read manifest");
            return serde_json::from_str(&raw).expect("valid manifest json");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for manifest {}",
            manifest_path.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn http_json_get(base_url: &str, path: &str, bearer_token: &str) -> serde_json::Value {
    let authority = base_url
        .strip_prefix("http://")
        .expect("loopback HTTP base URL");
    let mut stream = TcpStream::connect(authority).expect("connect daemon status endpoint");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set daemon status timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {bearer_token}\r\nConnection: close\r\n\r\n"
    )
    .expect("write daemon status request");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read daemon status response");
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected daemon status response: {response}"
    );
    let body = response.split_once("\r\n\r\n").expect("status body").1;
    serde_json::from_str(body).expect("valid status JSON")
}

fn wait_for_exit_or_stop(mut child: Child, timeout: Duration) -> (bool, Output) {
    let deadline = Instant::now() + timeout;
    let exited_before_deadline = loop {
        if child
            .try_wait()
            .expect("read daemon process status")
            .is_some()
        {
            break true;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break false;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let output = child.wait_with_output().expect("collect daemon output");
    (exited_before_deadline, output)
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

#[test]
fn daemon_binary_writes_local_capability_manifest_from_cli_arg() {
    let temp_dir = unique_temp_dir("manifest-arg");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .arg("--manifest-dir")
            .arg(&manifest_dir)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
            .env_remove("LOOM_RUN_STORE_PATH")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn loom daemon"),
    );

    let manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    child.stop();

    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["appId"], "loom");
    assert_eq!(manifest["displayName"], "Loom");
    assert_eq!(manifest["transport"]["type"], "http");
    assert_eq!(manifest["transport"]["auth"], "bearer");
    assert!(manifest["transport"]["authToken"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));
    assert!(manifest["transport"]["baseUrl"]
        .as_str()
        .expect("base url")
        .starts_with("http://127.0.0.1:"));
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .contains(&serde_json::Value::String("brain.plan".to_owned())));
}

#[test]
fn daemon_binary_writes_local_capability_manifest_from_env_var() {
    let temp_dir = unique_temp_dir("manifest-env");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
            .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
            .env_remove("LOOM_RUN_STORE_PATH")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn loom daemon"),
    );

    let manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    child.stop();

    assert_eq!(manifest["appId"], "loom");
    assert_eq!(manifest["transport"]["auth"], "bearer");
    assert!(manifest["transport"]["authToken"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));
    assert!(manifest["transport"]["baseUrl"]
        .as_str()
        .expect("base url")
        .starts_with("http://127.0.0.1:"));
}

#[test]
fn daemon_binary_starts_with_gateway_planner_configuration() {
    let temp_dir = unique_temp_dir("gateway-planner");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
            .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
            .env_remove("LOOM_RUN_STORE_PATH")
            .env("LOOM_GATEWAY_MODEL", "test-model")
            .env("LOOM_GATEWAY_BASE_URL", "http://127.0.0.1:4200")
            .env("LOOM_GATEWAY_TOKEN", "test-token")
            .env("LOOM_GATEWAY_TIMEOUT_SECS", "1")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn Gateway-configured Loom daemon"),
    );

    let manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    child.stop();

    assert_eq!(manifest["appId"], "loom");
    assert_eq!(manifest["transport"]["auth"], "bearer");
    assert!(manifest["transport"]["authToken"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));
    assert!(manifest["transport"]["baseUrl"]
        .as_str()
        .expect("base url")
        .starts_with("http://127.0.0.1:"));
}

#[test]
fn daemon_binary_creates_sqlite_run_store_under_control_plane_root() {
    let temp_dir = unique_temp_dir("sqlite-store");
    let manifest_dir = temp_dir.join("capabilities");
    let control_plane_root = temp_dir.join("control-plane");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
            .env("LOOM_CONTROL_PLANE_ROOT", &control_plane_root)
            .env_remove("LOOM_RUN_STORE_PATH")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn daemon"),
    );

    let _manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    child.stop();

    assert!(control_plane_root
        .join("runs")
        .join("loom-runs.sqlite3")
        .exists());
    fs::remove_dir_all(temp_dir).expect("cleanup");
}

#[test]
fn daemon_binary_honors_run_store_path_override() {
    let temp_dir = unique_temp_dir("sqlite-store-override");
    let manifest_dir = temp_dir.join("capabilities");
    let control_plane_root = temp_dir.join("control-plane");
    let override_path = temp_dir.join("custom").join("evidence.sqlite3");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
            .env("LOOM_CONTROL_PLANE_ROOT", &control_plane_root)
            .env("LOOM_RUN_STORE_PATH", &override_path)
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn daemon"),
    );

    let _manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    child.stop();

    assert!(override_path.exists());
    assert!(!control_plane_root
        .join("runs")
        .join("loom-runs.sqlite3")
        .exists());
    fs::remove_dir_all(temp_dir).expect("cleanup");
}

#[test]
fn daemon_binary_uses_bounded_request_executor_by_default() {
    let temp_dir = unique_temp_dir("bounded-executor-default");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = ChildGuard::new(
        Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", "0")
            .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
            .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
            .env_remove("LOOM_RUN_STORE_PATH")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .spawn()
            .expect("spawn daemon"),
    );

    let manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    let bearer_token = manifest["transport"]["authToken"]
        .as_str()
        .expect("manifest bearer token");
    let status = http_json_get(
        manifest["transport"]["baseUrl"].as_str().expect("base URL"),
        "/status",
        bearer_token,
    );
    child.stop();

    assert_eq!(status["requestExecutor"]["mode"], "bounded_workers");
    assert_eq!(status["requestExecutor"]["workers"], 4);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 32);
    fs::remove_dir_all(temp_dir).expect("cleanup");
}

#[cfg(windows)]
#[test]
fn daemon_binary_exits_cleanly_on_console_break() {
    let temp_dir = unique_temp_dir("console-break-shutdown");
    let manifest_dir = temp_dir.join("capabilities");
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let child = Command::new(exe)
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .env("LOOM_DAEMON_HOST", "127.0.0.1")
        .env("LOOM_DAEMON_PORT", "0")
        .env("LOOM_CAPABILITY_MANIFEST_DIR", &manifest_dir)
        .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
        .env_remove("LOOM_RUN_STORE_PATH")
        .env_remove("LOOM_DAEMON_WORKERS")
        .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn console-break Loom daemon");

    let _manifest = wait_for_manifest(&manifest_dir.join("loom.json"));
    let signaled = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
    assert_ne!(signaled, 0, "send CTRL_BREAK_EVENT to Loom daemon");

    let (exited_before_deadline, output) = wait_for_exit_or_stop(child, Duration::from_secs(5));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::remove_dir_all(temp_dir).expect("cleanup console-break fixture");

    assert!(
        exited_before_deadline,
        "daemon did not exit after CTRL_BREAK_EVENT; stdout={stdout}; stderr={stderr}"
    );
    assert!(
        output.status.success(),
        "daemon did not exit cleanly after CTRL_BREAK_EVENT; stdout={stdout}; stderr={stderr}"
    );
}

#[test]
fn daemon_binary_rejects_invalid_request_executor_environment() {
    let cases = [
        ("LOOM_DAEMON_WORKERS", "0"),
        ("LOOM_DAEMON_WORKERS", "33"),
        ("LOOM_DAEMON_WORKERS", "bad"),
        ("LOOM_DAEMON_QUEUE_CAPACITY", "0"),
        ("LOOM_DAEMON_QUEUE_CAPACITY", "1025"),
        ("LOOM_DAEMON_QUEUE_CAPACITY", "bad"),
    ];
    let exe = env!("CARGO_BIN_EXE_loom-daemon");
    let occupied_listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve loopback port");
    let occupied_port = occupied_listener
        .local_addr()
        .expect("reserved loopback address")
        .port()
        .to_string();

    for (name, value) in cases {
        let temp_dir = unique_temp_dir(&format!("invalid-executor-{name}-{value}"));
        let manifest_path = temp_dir.join("capabilities").join("loom.json");
        let child = Command::new(exe)
            .env("LOOM_DAEMON_HOST", "127.0.0.1")
            .env("LOOM_DAEMON_PORT", &occupied_port)
            .env(
                "LOOM_CAPABILITY_MANIFEST_DIR",
                temp_dir.join("capabilities"),
            )
            .env("LOOM_CONTROL_PLANE_ROOT", temp_dir.join("control-plane"))
            .env_remove("LOOM_RUN_STORE_PATH")
            .env_remove("LOOM_DAEMON_WORKERS")
            .env_remove("LOOM_DAEMON_QUEUE_CAPACITY")
            .env(name, value)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn invalid daemon configuration");

        let (exited_before_deadline, output) = wait_for_exit_or_stop(child, Duration::from_secs(2));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let manifest_created = manifest_path.exists();
        fs::remove_dir_all(&temp_dir).expect("cleanup invalid daemon fixture");

        assert!(
            exited_before_deadline,
            "{name}={value} started instead of rejecting configuration; stdout={stdout}; stderr={stderr}"
        );
        assert!(
            !output.status.success(),
            "{name}={value} exited successfully"
        );
        assert!(
            stderr.contains(name),
            "stderr did not name {name}: {stderr}"
        );
        assert!(
            !stderr.contains("bind loom daemon"),
            "{name}={value} attempted to bind before validation: {stderr}"
        );
        assert!(
            !stdout.contains("listening on"),
            "{name}={value} started a listener: {stdout}"
        );
        assert!(
            !manifest_created,
            "{name}={value} wrote a discovery manifest before failing"
        );
    }
}
