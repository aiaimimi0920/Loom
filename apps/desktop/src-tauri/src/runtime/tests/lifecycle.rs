//! Owned daemon lifecycle and runtime-configuration regressions.

use super::*;

#[test]
fn desktop_exit_terminates_and_reaps_the_owned_daemon_process() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    LOOM_EXITING.store(false, Ordering::Release);
    stop_owned_daemon_process();
    let child =
        std::process::Command::new(std::env::current_exe().expect("desktop test executable"))
            .args([
                "runtime::tests::owned_daemon_sleep_fixture",
                "--exact",
                "--nocapture",
            ])
            .env("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn owned daemon fixture");
    let pid = child.id();

    register_owned_daemon_process(child).expect("register owned daemon fixture");
    assert_eq!(owned_daemon_process_id(), Some(pid));
    assert_eq!(stop_owned_daemon_process(), Some(pid));
    assert_eq!(owned_daemon_process_id(), None);
}

#[test]
fn daemon_start_finishing_after_exit_is_reaped_instead_of_registered() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    LOOM_EXITING.store(false, Ordering::Release);
    stop_owned_daemon_process();
    let child =
        std::process::Command::new(std::env::current_exe().expect("desktop test executable"))
            .args([
                "runtime::tests::owned_daemon_sleep_fixture",
                "--exact",
                "--nocapture",
            ])
            .env("LOOM_DESKTOP_OWNED_DAEMON_FIXTURE", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn late daemon fixture");

    LOOM_EXITING.store(true, Ordering::Release);
    let error = register_owned_daemon_process(child).expect_err("exit rejects late daemon");

    assert!(error.contains("正在退出"));
    assert_eq!(owned_daemon_process_id(), None);
    LOOM_EXITING.store(false, Ordering::Release);
}

#[test]
fn default_runtime_config_points_at_loopback_loom_daemon() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_daemon_url = std::env::var("LOOM_DAEMON_URL").ok();
    let previous_bridge_url = std::env::var("LOOM_HOOK_BRIDGE_URL").ok();
    let previous_bridge_port = std::env::var("LOOM_HOOK_BRIDGE_PORT").ok();
    let previous_control_plane_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    let previous_token = std::env::var("LOOM_DAEMON_TOKEN").ok();
    let control_plane_root = unique_temp_dir("default-runtime-config");
    std::env::remove_var("LOOM_DAEMON_URL");
    std::env::remove_var("LOOM_HOOK_BRIDGE_URL");
    std::env::remove_var("LOOM_HOOK_BRIDGE_PORT");
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &control_plane_root);
    std::env::remove_var("LOOM_DAEMON_TOKEN");

    let config = resolve_loom_daemon_url();

    assert_eq!(config.loom_daemon_url, DEFAULT_LOOM_DAEMON_URL);
    assert_eq!(config.settings_url, "http://127.0.0.1:8765/settings");
    assert_eq!(config.hook_bridge_url, DEFAULT_HOOK_BRIDGE_URL);
    restore_env("LOOM_DAEMON_URL", previous_daemon_url);
    restore_env("LOOM_HOOK_BRIDGE_URL", previous_bridge_url);
    restore_env("LOOM_HOOK_BRIDGE_PORT", previous_bridge_port);
    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_control_plane_root);
    restore_env("LOOM_DAEMON_TOKEN", previous_token);
    let _ = fs::remove_dir_all(control_plane_root);
}

#[test]
fn native_daemon_client_discovers_persisted_bearer_token_and_settings_exchange_url() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let root = unique_temp_dir("daemon-auth-token");
    fs::write(root.join(DAEMON_AUTH_TOKEN_FILE), "desktop-secret")
        .expect("write desktop daemon token");
    let previous_root = std::env::var("LOOM_CONTROL_PLANE_ROOT").ok();
    let previous_token = std::env::var("LOOM_DAEMON_TOKEN").ok();
    std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &root);
    std::env::remove_var("LOOM_DAEMON_TOKEN");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind daemon auth fixture");
    let address = listener.local_addr().expect("daemon auth fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept daemon auth request");
        let request = read_test_http_request(&mut stream);
        assert!(request.contains("Authorization: Bearer desktop-secret\r\n"));
        write_test_json_response(&mut stream, "200 OK", r#"{"status":"ready"}"#);
    });

    let response = http_get_json(&format!("http://127.0.0.1:{}", address.port()), "/status")
        .expect("authenticated daemon request");
    assert_eq!(response["status"], "ready");
    server.join().expect("join daemon auth fixture");
    assert_eq!(
        settings_links("http://127.0.0.1:8765").tea,
        "http://127.0.0.1:8765/settings/tea?token=desktop-secret"
    );

    restore_env("LOOM_CONTROL_PLANE_ROOT", previous_root);
    restore_env("LOOM_DAEMON_TOKEN", previous_token);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_config_accepts_an_isolated_hook_bridge_port() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_bridge_url = std::env::var("LOOM_HOOK_BRIDGE_URL").ok();
    let previous_bridge_port = std::env::var("LOOM_HOOK_BRIDGE_PORT").ok();
    std::env::remove_var("LOOM_HOOK_BRIDGE_URL");
    std::env::set_var("LOOM_HOOK_BRIDGE_PORT", "43127");

    let config = resolve_loom_daemon_url();

    assert_eq!(config.hook_bridge_url, "ws://127.0.0.1:43127");
    restore_env("LOOM_HOOK_BRIDGE_URL", previous_bridge_url);
    restore_env("LOOM_HOOK_BRIDGE_PORT", previous_bridge_port);
}

#[cfg(target_os = "windows")]
#[test]
fn webview2_remote_debugging_port_builds_explicit_browser_arguments() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV).ok();
    std::env::set_var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, "43129");

    let arguments = configured_webview2_browser_args().expect("browser arguments");

    assert!(arguments.contains("--remote-debugging-port=43129"));
    assert!(arguments.contains("--remote-debugging-address=127.0.0.1"));
    assert!(arguments.contains("msSmartScreenProtection"));
    assert!(arguments.contains("--autoplay-policy=no-user-gesture-required"));
    restore_env(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, previous_port);
}

#[cfg(target_os = "windows")]
#[test]
fn webview2_remote_debugging_ignores_invalid_ports() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_port = std::env::var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV).ok();
    std::env::set_var(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, "invalid");

    assert_eq!(configured_webview2_browser_args(), None);
    restore_env(LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV, previous_port);
}
