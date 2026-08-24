// Loom daemon tests fragment 1; included into the shared crate test module.
use super::*;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use std::fs;
use std::io::{BufRead, Cursor, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static HOOK_ART_REQUEST_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take one of the test serialization locks above, treating an earlier panic as no obstacle.
///
/// Both mutexes guard `()`. They exist to stop tests that mutate process-wide state — environment
/// variables, the Hook canvas runtime — from running at the same time, and they carry no data that a
/// panicking test could have left half-written. Poisoning is therefore the wrong signal here: the
/// state each test depends on is state that test sets up itself, so the next holder of the lock is
/// unaffected by whatever the previous one did before it panicked.
///
/// With `.expect(...)` at every site it was worse than merely wrong. One test panicking while holding
/// the lock poisoned it for the rest of the run, so every later env-locked test failed on the poison
/// rather than on anything of its own: a single flake was observed reporting as 38 or 39 simultaneous
/// failures, which buries the one real fault in a wall of noise and makes a full-suite run useless as
/// evidence. Recovering the guard from the poison keeps the failure count equal to the fault count.
fn lock_ignoring_poison(
    mutex: &'static std::sync::Mutex<()>,
) -> std::sync::MutexGuard<'static, ()> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
#[test]
fn acl_repair_retries_a_transiently_unreadable_legacy_child() {
    let root = unique_temp_dir("acl-repair-retry");
    fs::create_dir_all(&root).expect("create ACL repair fixture");
    let trust_path = root.join("plugin-trust.json");
    let trust_attempts = std::cell::Cell::new(0_u32);
    let repair_attempts = std::cell::Cell::new(0_u32);

    let run_repair = || {
        repair_legacy_control_plane_permissions_with(
            &root,
            |path, _directory| {
                if path == trust_path {
                    let attempt = trust_attempts.get();
                    trust_attempts.set(attempt + 1);
                    if attempt == 0 {
                        return Err(std::io::Error::new(
                            ErrorKind::PermissionDenied,
                            "transient fixture denial",
                        ));
                    }
                    return Ok(());
                }
                if path == root.join("plugin-credentials.json") {
                    return Err(std::io::Error::from(ErrorKind::NotFound));
                }
                Ok(())
            },
            |_path| {
                let attempt = repair_attempts.get();
                repair_attempts.set(attempt + 1);
                Ok(if attempt == 0 {
                    vec![trust_path.clone()]
                } else {
                    Vec::new()
                })
            },
        )
    };

    run_repair().expect("first repair records the skipped child");
    let marker = root.join("migrations").join(ACL_MIGRATION_MARKER);
    let first_marker = fs::read_to_string(&marker).expect("read retry marker");
    assert!(first_marker.starts_with("2 skipped=1\n"));
    assert!(!acl_migration_marker_is_complete(&first_marker));

    run_repair().expect("second repair retries and completes");
    let second_marker = fs::read_to_string(&marker).expect("read completed marker");
    assert!(acl_migration_marker_is_complete(&second_marker));
    assert_eq!(trust_attempts.get(), 2);
    assert_eq!(repair_attempts.get(), 2);
    fs::remove_dir_all(root).expect("cleanup ACL repair fixture");
}

#[cfg(windows)]
#[test]
fn acl_repair_keeps_a_root_permission_failure_fatal() {
    let root = unique_temp_dir("acl-root-fatal");
    let error = repair_legacy_control_plane_permissions_with(
        &root,
        |_path, _directory| {
            Err(std::io::Error::new(
                ErrorKind::PermissionDenied,
                "fatal root denial",
            ))
        },
        |_path| Ok(Vec::new()),
    )
    .expect_err("root ACL failure must remain fatal");
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    assert!(!root.join("migrations").exists());
}

#[cfg(windows)]
#[test]
fn acl_repair_marker_requires_an_exact_completed_record() {
    assert!(acl_migration_marker_is_complete("2 skipped=0\n"));
    assert!(!acl_migration_marker_is_complete(
        "2 skipped=1\nskipped-path=x\n"
    ));
    assert!(!acl_migration_marker_is_complete(""));
    assert!(!acl_migration_marker_is_complete("garbage\n"));
}

#[test]
fn peer_read_admission_preserves_capacity_for_an_unrelated_peer() {
    let admission = PeerReadAdmission::new(3);
    let first_peer = "192.0.2.10".parse().expect("first fixture IP");
    let other_peer = "198.51.100.20".parse().expect("other fixture IP");
    let mut first_permits = (0..3)
        .map(|_| {
            admission
                .try_acquire(first_peer)
                .expect("first peer remains under its limit")
        })
        .collect::<Vec<_>>();

    assert!(admission.try_acquire(first_peer).is_none());
    let other_permit = admission
        .try_acquire(other_peer)
        .expect("unrelated peer retains independent capacity");
    drop(other_permit);
    drop(first_permits.pop());
    assert!(admission.try_acquire(first_peer).is_some());
}

#[test]
fn partial_request_can_read_a_bounded_refusal_without_a_reset() {
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind refusal fixture");
    let address = listener.local_addr().expect("refusal fixture address");
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept refused client");
        let (status, body) = daemon_busy_response();
        let started = Instant::now();
        drain_and_write_refusal(stream, status, &body);
        started.elapsed()
    });

    let mut client = TcpStream::connect(address).expect("connect refused client");
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set refused client timeout");
    client
        .write_all(
            b"POST /v1/invoke HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 64\r\n\r\npartial",
        )
        .expect("write partial refused request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read refusal without connection reset");
    let elapsed = server.join().expect("join refusal fixture");

    assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
    assert_eq!(
        response_json_body(&response)["error"]["code"],
        "daemon_busy"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "refusal drain exceeded its bounded grace: {elapsed:?}"
    );
}

fn hook_art_request(request_id: &str, node_id: &str, generation: u64) -> HookArtExecuteRequest {
    HookArtExecuteRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: request_id.to_owned(),
        node_id: node_id.to_owned(),
        art_id: "neuro.official/art".to_owned(),
        generation,
        device_id: Some("device:local".to_owned()),
        output_transports: vec![HookTransportMode::SharedMemory],
        inputs: BTreeMap::new(),
        parameters: BTreeMap::new(),
        disabled_parameters: Vec::new(),
        deadline_at_ms: None,
    }
}

#[test]
fn daemon_rejects_surface_resource_gc_age_below_the_safety_floor() {
    let error = match LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_surface_resource_gc_min_age_ms(MIN_RESOURCE_GC_AGE_MILLIS - 1),
    ) {
        Ok(_) => panic!("unsafe GC age must fail before daemon startup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("must be at least"));
}

#[test]
fn unit_test_daemon_bind_owns_an_isolated_control_plane_root() {
    let control_plane_before = std::env::var_os("LOOM_CONTROL_PLANE_ROOT");
    let framework_root_before = std::env::var_os("LOOM_FRAMEWORK_PACKAGES_DIR");
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind isolated daemon");
    let root = daemon.runtime.control_plane_root.clone();

    assert!(root.starts_with(std::env::temp_dir()));
    assert!(root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("loom-daemon-test-")));
    assert_eq!(
        std::env::var_os("LOOM_CONTROL_PLANE_ROOT"),
        control_plane_before
    );
    assert_eq!(
        std::env::var_os("LOOM_FRAMEWORK_PACKAGES_DIR"),
        framework_root_before
    );
    drop(daemon);
    fs::remove_dir_all(root).expect("cleanup isolated daemon root");
}

#[test]
fn hook_bridge_status_advertises_only_formal_namespaced_protocol() {
    let runtime = HookBridgeRuntime::new(unique_temp_dir("hook-status").join("workflows"));
    let status = hook_bridge_status_json(&runtime);
    assert_eq!(status["protocol"], loom_protocol::HOOK_PROTOCOL_VERSION);
    let methods = status["methods"].as_array().expect("Hook methods");
    assert!(methods.iter().all(|method| {
        method
            .as_str()
            .is_some_and(|method| method.starts_with("loom.hook."))
    }));
    assert!(!status
        .as_object()
        .expect("status object")
        .contains_key("sessionMethod"));
}

#[test]
fn hook_art_request_ids_are_idempotent_and_generations_replace_previous_work() {
    let _guard = lock_ignoring_poison(&HOOK_ART_REQUEST_TEST_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let first = hook_art_request("request:first", "node:one", 1);
    let first_token = match reserve_hook_art_request(&first, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("first request must execute"),
    };
    assert!(matches!(
        reserve_hook_art_request(&first, &store),
        HookArtReservation::Replay(_)
    ));

    let replacement = hook_art_request("request:replacement", "node:one", 2);
    assert!(matches!(
        reserve_hook_art_request(&replacement, &store),
        HookArtReservation::Execute(_)
    ));
    assert!(first_token.load(Ordering::Acquire));
    assert!(!hook_art_request_is_current(
        "request:first",
        "node:one",
        1,
        first.device_id.as_deref()
    ));
    assert!(hook_art_request_is_current(
        "request:replacement",
        "node:one",
        2,
        replacement.device_id.as_deref()
    ));

    let stale = hook_art_request("request:stale", "node:one", 1);
    let HookArtReservation::Reject(response) = reserve_hook_art_request(&stale, &store) else {
        panic!("stale request must be rejected");
    };
    let response: HookResponse = serde_json::from_str(&response).expect("stale response");
    assert_eq!(response.status, HookRequestStatus::Failed);
    assert_eq!(
        response.error.expect("stale error").code,
        "stale_generation"
    );
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_generation_resets_after_client_restart_when_node_is_idle() {
    let _guard = lock_ignoring_poison(&HOOK_ART_REQUEST_TEST_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let first = hook_art_request("request:completed", "node:restart", 5);
    match reserve_hook_art_request(&first, &store) {
        HookArtReservation::Execute(_) => {}
        _ => panic!("completed request must execute"),
    }
    finish_hook_art_request(
        &first.request_id,
        &first.node_id,
        first.generation,
        first.device_id.as_deref(),
        hook_art_request_fingerprint(&first),
        HookRequestStatus::Succeeded,
        "{}".to_owned(),
        &store,
    );

    let restarted = hook_art_request("request:after-restart", "node:restart", 1);
    assert!(matches!(
        reserve_hook_art_request(&restarted, &store),
        HookArtReservation::Execute(_)
    ));
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_request_coordination_is_scoped_by_device() {
    let _guard = lock_ignoring_poison(&HOOK_ART_REQUEST_TEST_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let device_one = hook_art_request("request:shared", "node:shared", 4);
    let mut device_two = hook_art_request("request:shared", "node:shared", 4);
    device_two.device_id = Some("device:remote".to_owned());

    let device_one_token = match reserve_hook_art_request(&device_one, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("first device request must execute"),
    };
    let device_two_token = match reserve_hook_art_request(&device_two, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("second device request must execute independently"),
    };

    assert!(!device_one_token.load(Ordering::Acquire));
    assert!(!device_two_token.load(Ordering::Acquire));
    assert!(hook_art_request_is_current(
        &device_one.request_id,
        &device_one.node_id,
        device_one.generation,
        device_one.device_id.as_deref(),
    ));
    assert!(hook_art_request_is_current(
        &device_two.request_id,
        &device_two.node_id,
        device_two.generation,
        device_two.device_id.as_deref(),
    ));
    assert_eq!(
        next_hook_art_preview_revision(
            &device_one.request_id,
            &device_one.node_id,
            device_one.generation,
            device_one.device_id.as_deref(),
        ),
        Some(1)
    );
    assert_eq!(
        next_hook_art_preview_revision(
            &device_two.request_id,
            &device_two.node_id,
            device_two.generation,
            device_two.device_id.as_deref(),
        ),
        Some(1)
    );
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_request_id_replay_requires_an_identical_request() {
    let _guard = lock_ignoring_poison(&HOOK_ART_REQUEST_TEST_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let request = hook_art_request("request:identity", "node:identity", 1);
    assert!(matches!(
        reserve_hook_art_request(&request, &store),
        HookArtReservation::Execute(_)
    ));

    let mut changed = request.clone();
    changed.parameters.insert("quality".to_owned(), json!(90));
    let HookArtReservation::Reject(response) = reserve_hook_art_request(&changed, &store) else {
        panic!("changed request payload must conflict with the active requestId");
    };
    let response: HookResponse = serde_json::from_str(&response).expect("conflict response");
    assert_eq!(
        response.error.expect("conflict error").code,
        "request_id_conflict"
    );
    clear_hook_canvas_runtime_state(Some(&store));
}

#[test]
fn hook_art_cancellation_binds_node_generation_and_device() {
    let _guard = lock_ignoring_poison(&HOOK_ART_REQUEST_TEST_LOCK);
    clear_hook_canvas_runtime_state(None);
    let store = Arc::new(Mutex::new(SharedImageStore::new()));
    let request = hook_art_request("request:cancel", "node:cancel", 4);
    let token = match reserve_hook_art_request(&request, &store) {
        HookArtReservation::Execute(token) => token,
        _ => panic!("request must execute"),
    };
    let wrong_device = HookArtCancelRequest {
        protocol_version: loom_protocol::HOOK_PROTOCOL_VERSION.to_owned(),
        request_id: request.request_id.clone(),
        node_id: request.node_id.clone(),
        generation: request.generation,
        device_id: Some("device:other".to_owned()),
    };
    let response: HookResponse =
        serde_json::from_str(&cancel_hook_art_request(&wrong_device, &store))
            .expect("wrong-device cancellation response");
    assert_eq!(response.status, HookRequestStatus::Failed);
    assert!(!token.load(Ordering::Acquire));

    let cancellation = HookArtCancelRequest {
        device_id: request.device_id.clone(),
        ..wrong_device
    };
    let response: HookResponse =
        serde_json::from_str(&cancel_hook_art_request(&cancellation, &store))
            .expect("cancellation response");
    assert_eq!(response.status, HookRequestStatus::CancelRequested);
    assert!(token.load(Ordering::Acquire));
    clear_hook_canvas_runtime_state(Some(&store));
}
