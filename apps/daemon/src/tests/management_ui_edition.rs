// Tests for server edition status metadata, embedded `/ui/` web console auth, and LAN host policy.

struct EditionTestDaemon {
    port: u16,
    shutdown: mpsc::Sender<()>,
    handle: Option<JoinHandle<Result<()>>>,
}

impl EditionTestDaemon {
    fn start(config: DaemonConfig) -> Self {
        let daemon = LoomDaemon::bind(config).expect("bind test daemon");
        let port = daemon.local_addr().expect("daemon local addr").port();
        let (shutdown, rx) = mpsc::channel();
        let handle = thread::spawn(move || daemon.serve_until(rx));
        Self {
            port,
            shutdown,
            handle: Some(handle),
        }
    }

    fn stop(mut self) {
        let _ = self.shutdown.send(());
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .expect("join test daemon")
                .expect("clean daemon shutdown");
        }
    }
}

fn raw_http_get(port: u16, target: &str, extra_headers: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect test daemon");
    let raw = format!(
        "GET {target} HTTP/1.1\r\n{extra_headers}Connection: close\r\n\r\n"
    );
    stream.write_all(raw.as_bytes()).expect("write request");
    let mut out = String::new();
    stream.read_to_string(&mut out).expect("read response");
    out
}

#[test]
fn management_ui_serves_login_gate_cookie_exchange_and_embedded_spa() {
    let server = EditionTestDaemon::start(
        DaemonConfig::localhost(0)
            .with_bearer_token("web-console-secret")
            .with_lan_management(true),
    );
    let port = server.port;

    let unauth = raw_http_get(port, "/ui/", &format!("Host: 127.0.0.1:{port}\r\n"));
    if cfg!(feature = "management-ui") {
        assert!(
            unauth.starts_with("HTTP/1.1 200 OK"),
            "unexpected unauthenticated /ui/ response (raw output withheld)"
        );
        assert!(unauth.contains("Loom Web 管理控制台"));

        let exchange = raw_http_get(
            port,
            "/ui/?token=web-console-secret",
            &format!("Host: 127.0.0.1:{port}\r\n"),
        );
        assert!(exchange.starts_with("HTTP/1.1 303 See Other"));
        assert!(exchange.contains("Location: /ui/\r\n"));
        assert!(exchange.contains(
            "Set-Cookie: loom_admin=web-console-secret; HttpOnly; SameSite=Strict; Path=/\r\n"
        ));

        let authed_ui = raw_http_get(
            port,
            "/ui/devices",
            &format!(
                "Host: 192.168.1.50:{port}\r\nOrigin: http://192.168.1.50:{port}\r\nCookie: loom_admin=web-console-secret\r\n"
            ),
        );
        assert!(
            authed_ui.starts_with("HTTP/1.1 200 OK"),
            "unexpected authenticated /ui/devices response (raw output withheld)"
        );
        assert!(authed_ui.contains("Content-Type: text/html"));
    } else {
        assert!(unauth.starts_with("HTTP/1.1 404 Not Found"));
    }

    let status_raw = raw_http_get(
        port,
        "/status",
        &format!("Host: 127.0.0.1:{port}\r\nAuthorization: Bearer web-console-secret\r\n"),
    );
    let status_body = status_raw
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default();
    let status: Value = serde_json::from_str(status_body).expect("status JSON");
    assert_eq!(
        status["edition"],
        if cfg!(feature = "management-ui") {
            "web"
        } else {
            "api"
        }
    );
    assert_eq!(status["managementUi"]["mountPath"], "/ui/");
    assert_eq!(
        status["managementUi"]["compiled"],
        cfg!(feature = "management-ui")
    );

    server.stop();
}

#[test]
fn management_ui_respects_runtime_disable_flag() {
    let server = EditionTestDaemon::start(
        DaemonConfig::localhost(0)
            .with_bearer_token("web-disabled-token")
            .with_management_ui(false),
    );
    let response = raw_http_get(
        server.port,
        "/ui/",
        &format!(
            "Host: 127.0.0.1:{}\r\nAuthorization: Bearer web-disabled-token\r\n",
            server.port
        ),
    );
    assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    server.stop();
}
