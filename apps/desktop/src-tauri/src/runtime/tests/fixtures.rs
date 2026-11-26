//! Temporary-directory, environment, and loopback HTTP test helpers.

use super::*;

// Callers hold ENV_LOCK; requests must be explicitly bound to their fixture origin.
pub(super) struct TestDaemonOrigin {
    url: Option<std::ffi::OsString>,
    active: Option<String>,
    root: Option<std::ffi::OsString>,
    token: Option<std::ffi::OsString>,
    isolated_root: Option<PathBuf>,
}

impl TestDaemonOrigin {
    pub(super) fn bind(listener: &TcpListener) -> Self {
        let root = unique_temp_dir("transport-origin");
        let mut fixture = Self::bind_with_root(listener, &root);
        fixture.isolated_root = Some(root);
        fixture
    }

    pub(super) fn bind_with_root(listener: &TcpListener, root: &Path) -> Self {
        let previous = std::env::var_os("LOOM_DAEMON_URL");
        let previous_root = std::env::var_os("LOOM_CONTROL_PLANE_ROOT");
        let previous_token = std::env::var_os("LOOM_DAEMON_TOKEN");
        let mut active = ACTIVE_DAEMON_URL
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap();
        let previous_active = active.take();
        std::env::set_var(
            "LOOM_DAEMON_URL",
            format!("http://{}", listener.local_addr().unwrap()),
        );
        std::env::set_var("LOOM_CONTROL_PLANE_ROOT", root);
        std::env::remove_var("LOOM_DAEMON_TOKEN");
        Self {
            url: previous,
            active: previous_active,
            root: previous_root,
            token: previous_token,
            isolated_root: None,
        }
    }
}

impl Drop for TestDaemonOrigin {
    fn drop(&mut self) {
        match self.url.take() {
            Some(value) => std::env::set_var("LOOM_DAEMON_URL", value),
            None => std::env::remove_var("LOOM_DAEMON_URL"),
        }
        *ACTIVE_DAEMON_URL
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap() = self.active.take();
        for (name, value) in [
            ("LOOM_CONTROL_PLANE_ROOT", self.root.take()),
            ("LOOM_DAEMON_TOKEN", self.token.take()),
        ] {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        if let Some(root) = self.isolated_root.take() {
            fs::remove_dir_all(root).unwrap();
        }
    }
}

pub(super) fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("loom-desktop-{name}-{nonce}"));
    fs::create_dir_all(&root).expect("create temp dir");
    root
}

pub(super) fn read_test_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set test request timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let bytes = stream.read(&mut buffer).expect("read test request");
        if bytes == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..bytes]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }
    String::from_utf8(request).expect("test request is utf8")
}

pub(super) fn write_test_json_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write test response");
}

pub(super) fn restore_env(name: &str, value: Option<String>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}
