//! Offline/degraded transport and packaged daemon path contracts.

use super::*;

#[test]
fn daemon_that_disappears_after_core_probes_is_offline() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind disappearing fixture");
    let address = listener
        .local_addr()
        .expect("read disappearing fixture address");
    let server = thread::spawn(move || {
        for body in [r#"{"status":"ok"}"#, r#"{"status":"ready"}"#] {
            let (mut stream, _) = listener.accept().expect("accept core probe");
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).expect("read core probe");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write core probe response");
        }
    });

    let snapshot =
        read_loom_snapshot_blocking(Some(format!("http://127.0.0.1:{}", address.port())));
    server.join().expect("join disappearing fixture");

    assert_eq!(snapshot.connection_state, "offline");
    assert!(snapshot.error.is_some());
}

#[test]
fn malformed_optional_module_contract_is_reported_as_degraded() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind malformed fixture");
    let address = listener
        .local_addr()
        .expect("read malformed fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept malformed request");
        let mut request = [0_u8; 512];
        let _ = stream.read(&mut request).expect("read malformed request");
        let body = "{}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write malformed response");
    });

    let mut degraded_errors = Vec::new();
    let values = read_optional_daemon_array(
        &format!("http://127.0.0.1:{}", address.port()),
        "/v1/tools",
        "tools",
        &mut degraded_errors,
    );
    server.join().expect("join malformed fixture");

    assert!(values.is_empty());
    assert!(degraded_errors
        .iter()
        .any(|error| error.contains("/v1/tools") && error.contains("tools")));
}

#[test]
fn rejects_non_loopback_daemon_url() {
    let snapshot = read_loom_snapshot_blocking(Some("http://example.com:8765".to_string()));

    assert_eq!(snapshot.connection_state, "offline");
    assert_eq!(
        snapshot.error,
        Some("Loom 桌面端只连接回环地址上的本地服务。".to_string())
    );
}

#[test]
fn loopback_url_parser_accepts_localhost() {
    assert_eq!(
        parse_loopback_http_url("http://localhost:8765"),
        Ok(("localhost".to_string(), 8765))
    );
}

#[test]
fn loopback_url_parser_rejects_paths_queries_credentials_and_port_zero() {
    for url in [
        "http://localhost:8765/status",
        "http://localhost:8765?token=secret",
        "http://user@localhost:8765",
        "http://127.0.0.1:0",
    ] {
        assert!(parse_loopback_http_url(url).is_err(), "accepted {url}");
    }
    assert_eq!(
        parse_loopback_http_url("http://127.0.0.1:8765/"),
        Ok(("127.0.0.1".to_owned(), 8765))
    );
}

#[test]
fn daemon_request_rejects_method_and_path_injection_before_connecting() {
    let method_error = http_request_json("http://127.0.0.1:1", "GET\r\nX-Test:", "/health", None)
        .expect_err("invalid method must fail");
    assert!(method_error.contains("HTTP 方法无效"), "{method_error}");

    let path_error = http_request_json(
        "http://127.0.0.1:1",
        "GET",
        "/health\r\nX-Test: injected",
        None,
    )
    .expect_err("invalid path must fail");
    assert!(path_error.contains("API 路径"), "{path_error}");
}

#[test]
fn http_response_parser_enforces_status_length_headers_and_body_bounds() {
    let response = b"HTTP/1.0 204 No Content\r\nContent-Length: 0\r\n\r\n";
    let parsed = parse_http_response(response, "/health", 8).expect("parse valid response");
    assert_eq!(parsed.status_code, 204);

    let invalid_status = b"HTTP/1.1 2000 Invalid\r\nContent-Length: 0\r\n\r\n";
    assert!(parse_http_response(invalid_status, "/health", 8).is_err());
    let oversized = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n1234";
    assert!(parse_http_response(oversized, "/health", 3).is_err());
    let mismatch = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n1234";
    assert!(parse_http_response(mismatch, "/health", 8).is_err());
    let duplicate = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n";
    assert!(parse_http_response(duplicate, "/health", 8).is_err());
    let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
    assert!(parse_http_response(chunked, "/health", 8).is_err());
}

#[test]
fn bounded_http_reader_stops_after_the_configured_response_limit() {
    let mut response = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
    response.extend_from_slice(&vec![0_u8; MAX_DAEMON_RESPONSE_HEADER_BYTES + 9]);
    let error = read_bounded_http_response(&mut std::io::Cursor::new(response), "/preview", 8)
        .expect_err("oversized response must fail");
    assert!(error.contains("字节正文限制"), "{error}");
}

#[test]
fn json_and_raster_content_types_are_derived_from_safe_contracts() {
    assert!(is_json_content_type("application/json; charset=utf-8"));
    assert!(is_json_content_type("application/problem+json"));
    assert!(!is_json_content_type("text/html"));

    assert_eq!(
        detect_raster_content_type(b"\x89PNG\r\n\x1a\nrest"),
        Some("image/png")
    );
    assert_eq!(
        detect_raster_content_type(b"RIFF\x04\x00\x00\x00WEBPrest"),
        Some("image/webp")
    );
    assert_eq!(detect_raster_content_type(b"<svg onload='x'>"), None);
    assert_eq!(detect_raster_content_type(b"<html>not an image"), None);
    let mut compatible_avif = b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00avif".to_vec();
    compatible_avif.extend_from_slice(b"rest");
    assert_eq!(
        detect_raster_content_type(&compatible_avif),
        Some("image/avif")
    );
}

#[test]
fn daemon_json_transport_rejects_non_json_content_type() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind content-type fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept content-type request");
        let _ = read_test_http_request(&mut stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .expect("write non-json response");
    });
    let error = http_get_json(&format!("http://127.0.0.1:{}", address.port()), "/health")
        .expect_err("non-json content type must fail");
    server.join().expect("join content-type fixture");
    assert!(error.contains("响应类型不是 JSON"), "{error}");
}

#[test]
fn binary_transport_derives_mime_from_bytes_instead_of_response_header() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind image fixture");
    let address = listener.local_addr().expect("read image fixture address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept image request");
        let _ = read_test_http_request(&mut stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 12\r\nConnection: close\r\n\r\n\x89PNG\r\n\x1a\nrest",
            )
            .expect("write image response");
    });
    let (content_type, body) = http_get_binary(
        &format!("http://127.0.0.1:{}", address.port()),
        "/preview.png",
    )
    .expect("valid PNG body must be accepted");
    server.join().expect("join image fixture");
    assert_eq!(content_type, "image/png");
    assert!(body.starts_with(b"\x89PNG"));
}

#[test]
fn daemon_sidecar_path_uses_packaged_runtime_directory() {
    let desktop_exe = std::path::Path::new(r"C:\apps\Loom\Loom.exe");

    let daemon_path = daemon_sidecar_path_for_exe(desktop_exe);

    assert_eq!(
        daemon_path,
        std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe")
    );
}

#[test]
fn daemon_candidates_prefer_explicit_override_then_runtime_then_development_target() {
    let desktop_exe = std::path::Path::new(r"C:\apps\Loom\Loom.exe");
    let repo_root = std::path::Path::new(r"C:\src\Loom");

    let candidates = daemon_executable_candidates(
        desktop_exe,
        Some(std::path::PathBuf::from(r"D:\loom\custom-daemon.exe")),
        Some(repo_root),
    );

    assert_eq!(
        candidates,
        vec![
            std::path::PathBuf::from(r"D:\loom\custom-daemon.exe"),
            std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe"),
            std::path::PathBuf::from(r"C:\apps\Loom\loom-daemon.exe"),
            std::path::PathBuf::from(r"C:\src\Loom\target\debug\loom-daemon.exe"),
        ]
    );
}

#[test]
fn daemon_candidates_include_root_sibling_fallback_before_development_target() {
    let desktop_exe = std::path::Path::new(r"C:\apps\Loom\loom-desktop.exe");
    let repo_root = std::path::Path::new(r"C:\src\Loom");

    let candidates = daemon_executable_candidates(desktop_exe, None, Some(repo_root));

    assert_eq!(
        candidates,
        vec![
            std::path::PathBuf::from(r"C:\apps\Loom\runtime\loom-daemon.exe"),
            std::path::PathBuf::from(r"C:\apps\Loom\loom-daemon.exe"),
            std::path::PathBuf::from(r"C:\src\Loom\target\debug\loom-daemon.exe"),
        ]
    );
}

#[test]
fn blank_daemon_override_is_ignored() {
    assert_eq!(configured_daemon_executable("  "), None);
}

#[test]
fn daemon_path_mismatch_warning_reports_old_running_daemon() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_override = std::env::var(LOOM_DAEMON_EXECUTABLE_ENV).ok();
    std::env::remove_var(LOOM_DAEMON_EXECUTABLE_ENV);

    let root = unique_temp_dir("daemon-path-mismatch");
    let desktop_dir = root.join("Loom");
    let runtime_dir = desktop_dir.join("runtime");
    fs::create_dir_all(&runtime_dir).expect("create runtime dir");
    let desktop_exe = desktop_dir.join("Loom.exe");
    let packaged_daemon = runtime_dir.join("loom-daemon.exe");
    fs::write(&desktop_exe, b"desktop").expect("write desktop exe placeholder");
    fs::write(&packaged_daemon, b"daemon").expect("write daemon exe placeholder");

    let warning = daemon_path_mismatch_warning(
        &desktop_exe,
        &serde_json::json!({
            "status": "ready",
            "executablePath": r"C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\older\runtime\loom-daemon.exe"
        }),
    )
    .expect("mismatch warning");

    assert!(warning.contains("旧 daemon"), "{warning}");
    assert!(warning.contains("127.0.0.1:8765"), "{warning}");
    assert!(
        warning.contains(&packaged_daemon.display().to_string()),
        "{warning}"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
    restore_env(LOOM_DAEMON_EXECUTABLE_ENV, previous_override);
}
