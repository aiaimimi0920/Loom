use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

#[test]
fn implicit_credentials_are_lazy_and_loopback_only() {
    for url in ["http://192.168.1.10:8765", "http://example.com:8765"] {
        assert_eq!(
            select_daemon_auth_token(url, None, || panic!("remote read local token")).unwrap(),
            None
        );
        assert_eq!(
            select_daemon_auth_token(url, Some("chosen"), || panic!("explicit read local token"))
                .unwrap()
                .as_deref(),
            Some("chosen")
        );
    }
    for url in [
        "http://127.0.0.1:8765",
        "http://[::1]:8765",
        "http://localhost:8765",
    ] {
        assert_eq!(
            select_daemon_auth_token(url, None, || Ok(Some("local".into())))
                .unwrap()
                .as_deref(),
            Some("local")
        );
        assert_eq!(
            select_daemon_auth_token(url, Some(""), || panic!("disabled read local token"))
                .unwrap(),
            None
        );
    }
}

#[test]
fn help_and_version_do_not_require_a_daemon() {
    let mut help = Vec::new();
    run_cli_with_writer(["loom", "--help"], &mut help).expect("help command");
    let help = String::from_utf8(help).expect("help utf8");
    assert!(help.contains("Usage: loom"));
    assert!(help.contains("frameworks list"));
    assert!(help.contains("arts list"));
    assert!(help.contains("workflows list"));
    assert!(help.contains("mcp-stdio"));
    assert!(help.contains("ai status"));

    let mut version = Vec::new();
    run_cli_with_writer(["loom", "--version"], &mut version).expect("version command");
    let version = String::from_utf8(version).expect("version utf8");
    assert_eq!(version.trim(), format!("loom {}", loom_core::LOOM_VERSION));
}

#[test]
fn status_command_reads_daemon_status_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
    let address = listener.local_addr().expect("mock address");
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept status request");
        let request = read_http_request(&mut socket);
        assert!(
            request.starts_with("GET /status HTTP/1.1"),
            "unexpected request: {request:?}"
        );
        let body = r#"{"status":"ready","modules":[{"name":"core","initialized":true}]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
    });

    let mut output = Vec::new();
    run_cli_with_writer(
        [
            "loom",
            "status",
            "--daemon-url",
            &format!("http://{address}"),
        ],
        &mut output,
    )
    .expect("status command");

    server.join().expect("mock server");
    let output = String::from_utf8(output).expect("utf8 output");
    assert!(output.contains("ready"));
    assert!(output.contains("core"));
}

#[test]
fn status_command_sends_bearer_token_when_configured() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
    let address = listener.local_addr().expect("mock address");
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept status request");
        let request = read_http_request(&mut socket);
        assert!(
            request.contains("Authorization: Bearer test-token\r\n"),
            "missing authorization header: {request:?}"
        );
        let body = r#"{"status":"ready","modules":[]}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
    });

    let mut output = Vec::new();
    run_cli_with_writer(
        [
            "loom",
            "status",
            "--daemon-url",
            &format!("http://{address}"),
            "--daemon-token",
            "test-token",
        ],
        &mut output,
    )
    .expect("status command");

    server.join().expect("mock server");
    let output = String::from_utf8(output).expect("utf8 output");
    assert!(output.contains("ready"));
}

#[test]
fn list_commands_query_daemon_control_plane_endpoints() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock daemon");
    let address = listener.local_addr().expect("mock address");
    let server = thread::spawn(move || {
        for (expected_path, body) in [
            (
                "GET /v1/frameworks HTTP/1.1",
                r#"{"frameworks":[{"id":"process","version":"1.0.0","ready":true}]}"#,
            ),
            (
                "GET /v1/workflows HTTP/1.1",
                r#"{"workflows":[{"id":"image-blend-workflow"}]}"#,
            ),
        ] {
            let (mut socket, _) = listener.accept().expect("accept list request");
            let request = read_http_request(&mut socket);
            assert!(request.starts_with(expected_path), "got {request:?}");
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write list response");
        }
    });

    let url = format!("http://{address}");
    let mut frameworks_out = Vec::new();
    run_cli_with_writer(
        ["loom", "frameworks", "list", "--daemon-url", &url],
        &mut frameworks_out,
    )
    .expect("frameworks list");
    assert!(String::from_utf8(frameworks_out)
        .expect("utf8")
        .contains("process\t1.0.0\tready=true"));

    let mut workflows_out = Vec::new();
    run_cli_with_writer(
        ["loom", "workflows", "list", "--daemon-url", &url],
        &mut workflows_out,
    )
    .expect("workflows list");
    assert_eq!(
        String::from_utf8(workflows_out).expect("utf8").trim(),
        "image-blend-workflow"
    );

    server.join().expect("mock server");
}

fn read_http_request(socket: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 64];
    loop {
        let bytes = socket.read(&mut buffer).expect("read request");
        if bytes == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8(request).expect("utf8 request")
}
