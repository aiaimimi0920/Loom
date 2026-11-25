//! Restricted HTTPS-to-loopback adapter. Never forwards admin credentials, cookies,
//! browser origins, arbitrary URLs or management routes. Live WSS transfers ownership
//! to the original authenticated media worker; it is not a transparent proxy.
use anyhow::{bail, ensure, Result};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::SocketAddr,
    time::{Duration, Instant},
};

const MAX_BODY: usize = 1024 * 1024;
const MAX_RESPONSE: u64 = 64 * 1024 * 1024;

fn allowed(method: &str, path: &str) -> bool {
    if super::lan_http_live::allowed(method, path)
        || super::lan_http_surface::allowed(method, path)
        || super::lan_http_wall::allowed(method, path)
    {
        return true;
    }
    if method == "POST"
        && (shared_projection(path) || offline_projection(path) || peer_projection(path))
    {
        return true;
    }
    match (method, path) {
        ("GET", "/v1/otter/identity" | "/v1/otter/connection-info" | "/v1/art/local/issuer") => {
            true
        }
        (
            "POST",
            "/v1/otter/connections/request"
            | "/v1/otter/connections/status"
            | "/v1/otter/connections/cancel"
            | "/v1/otter/connections/activate"
            | "/v1/otter/connections/disconnect"
            | "/v1/otter/locale-report"
            | "/v1/device-sessions/challenges"
            | "/v1/device-sessions"
            | "/v1/devices/requests",
        ) => true,
        (
            "POST",
            "/v1/art/local/describe"
            | "/v3/art/local/prepare-package"
            | "/v1/art/local/resource"
            | "/v1/art/local/live-viewer/attach"
            | "/v1/art/local/live-viewer/control"
            | "/v1/art/local/live-trigger"
            | "/v1/art/local/service/submit"
            | "/v1/art/local/service/submit-images"
            | "/v1/art/local/service/submit-image-output"
            | "/v1/art/local/service/poll-image-output"
            | "/v1/art/local/service/submit-confirmable"
            | "/v1/art/local/service/poll-confirmable"
            | "/v1/art/local/service/confirm"
            | "/v1/art/local/service/poll"
            | "/v1/art/local/service/cancel",
        ) => true,
        ("POST", path) => matches!(
            path.strip_prefix("/v1/projections/v2/"),
            Some("context" | "create" | "inspect" | "accept" | "update" | "read" | "unlink")
        ),
        _ => false,
    }
}

fn shared_projection(path: &str) -> bool {
    matches!(
        path.strip_prefix("/v1/projections/"),
        Some(
            "edit"
                | "targets"
                | "inbox"
                | "receipt"
                | "create"
                | "inspect"
                | "accept"
                | "update"
                | "read"
                | "unlink"
        )
    )
}

fn offline_projection(path: &str) -> bool {
    matches!(
        path.strip_prefix("/v1/offline-projections/"),
        Some(
            "import"
                | "edit"
                | "create"
                | "inbox"
                | "inspect"
                | "accept"
                | "read"
                | "update"
                | "receipt"
                | "unlink"
        )
    )
}

fn peer_projection(path: &str) -> bool {
    // These routes authenticate pinned signed challenges, never administrator credentials.
    matches!(
        path.strip_prefix("/v1/projection-peer/"),
        Some("handshake" | "catalog" | "raster-check" | "transfer")
    )
}

fn body_limit(path: &str) -> usize {
    if path == "/v1/otter/locale-report" {
        8 * 1024
    } else if super::lan_http_wall::matches(path) {
        512 * 1024
    } else if super::lan_http_surface::matches(path) {
        16 * 1024
    } else if matches!(
        path,
        "/v1/projections/edit" | "/v1/offline-projections/edit"
    ) {
        // Editing never carries a raster: keep both transports on the existing v1 edit budget.
        256 * 1024
    } else if matches!(
        path,
        "/v1/art/local/describe" | "/v3/art/local/prepare-package"
    ) {
        16 * 1024
    } else if path == "/v1/art/local/live-trigger" {
        loom_protocol::local_art::live_trigger::MAX_REQUEST
    } else if path == "/v1/art/local/live-viewer/control" {
        loom_protocol::local_art::live_control::MAX_REQUEST
    } else if path == "/v1/art/local/live-viewer/attach" {
        loom_protocol::local_art::live_viewer::MAX_REQUEST
    } else if path == "/v1/art/local/resource" {
        32 * 1024
    } else if matches!(
        path,
        "/v1/art/local/service/submit-images"
            | "/v1/art/local/service/submit-image-output"
            | "/v1/art/local/service/submit-confirmable"
    ) {
        loom_protocol::local_art::service::images::MAX_REQUEST
    } else if matches!(
        path,
        "/v1/art/local/service/submit"
            | "/v1/art/local/service/poll"
            | "/v1/art/local/service/poll-image-output"
            | "/v1/art/local/service/poll-confirmable"
            | "/v1/art/local/service/confirm"
            | "/v1/art/local/service/cancel"
    ) {
        96 * 1024
    } else if matches!(
        path,
        "/v1/projections/v2/create" | "/v1/projections/v2/update"
    ) || shared_projection(path)
        || matches!(
            path,
            "/v1/offline-projections/create"
                | "/v1/offline-projections/update"
                | "/v1/projection-peer/raster-check"
                | "/v1/projection-peer/transfer"
        )
    {
        loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
    } else if peer_projection(path) {
        16 * 1024
    } else {
        MAX_BODY
    }
}

pub(super) struct Request {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: HashMap<String, String>,
    pub(super) body: Vec<u8>,
}

fn read_request(stream: &mut impl Read, host: &str) -> Result<Request> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        ensure!(head.len() < 16 * 1024, "LAN headers too large");
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    let head = std::str::from_utf8(&head)?;
    let mut lines = head.split("\r\n");
    let parts: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    ensure!(
        parts.len() == 3 && parts[2] == "HTTP/1.1",
        "Invalid LAN request line"
    );
    let (method, path) = (parts[0], parts[1]);
    ensure!(allowed(method, path), "Route not available over LAN");
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid header"))?;
        ensure!(
            name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "Invalid header name"
        );
        ensure!(
            !value.bytes().any(|b| b.is_ascii_control()),
            "Invalid header value"
        );
        ensure!(
            headers.len() < 48
                && headers
                    .insert(name.to_ascii_lowercase(), value.trim().to_owned())
                    .is_none(),
            "Duplicate header"
        );
    }
    ensure!(
        headers.get("host").map(String::as_str) == Some(host),
        "Unexpected LAN Host"
    );
    for banned in ["origin", "cookie", "transfer-encoding", "expect"] {
        ensure!(!headers.contains_key(banned), "Unsupported LAN header");
    }
    ensure!(
        !headers.contains_key("upgrade")
            || super::lan_http_live::media(path)
            || super::lan_http_wall::media(path),
        "Upgrade not available over LAN"
    );
    if let Some(auth) = headers.get("authorization") {
        ensure!(
            auth.starts_with("Device ") && auth.len() <= 4096,
            "LAN accepts only device sessions"
        );
    }
    let length = headers
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    ensure!(length <= body_limit(path), "LAN request body exceeds limit");
    ensure!(
        method != "GET" || length == 0,
        "LAN GET body is not allowed"
    );
    if method == "POST" {
        ensure!(
            headers
                .get("content-type")
                .is_some_and(|s| s.split(';').next() == Some("application/json")),
            "JSON required"
        );
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    let request = Request {
        method: method.into(),
        path: path.into(),
        headers,
        body,
    };
    if super::lan_http_live::media(path) {
        super::lan_http_live::validate(&request)?;
    }
    if super::lan_http_surface::matches(path) {
        super::lan_http_surface::validate(&request)?;
    }
    if super::lan_http_wall::matches(path) {
        super::lan_http_wall::validate(&request)?;
    }
    Ok(request)
}

fn forward(request: Request, backend: SocketAddr, ingress: &str) -> Result<(u16, Vec<u8>)> {
    ensure!(
        backend.ip().is_loopback(),
        "LAN backend must remain loopback"
    );
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()?;
    let mut call = client
        .request(
            request.method.parse()?,
            format!("http://{backend}{}", request.path),
        )
        .header("Content-Type", "application/json")
        // Never forward a caller-supplied origin claim. This secret is process-local.
        .header("X-Loom-Lan-Ingress", ingress)
        .body(request.body);
    for name in ["authorization", "x-loom-device-nonce"] {
        if let Some(value) = request.headers.get(name) {
            call = call.header(name, value);
        }
    }
    let response = call.send()?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response.take(MAX_RESPONSE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RESPONSE {
        bail!("LAN response exceeds limit");
    }
    Ok((status, bytes))
}

pub(crate) fn serve(
    mut stream: loom_local_channel::lan_tls::LanStream,
    host: &str,
    backend: SocketAddr,
    ingress: &str,
    runtime: &std::sync::OnceLock<std::sync::Weak<crate::DaemonRuntime>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let result = read_request(&mut stream, host).and_then(|request| {
        if super::lan_http_live::media(&request.path) || super::lan_http_wall::media(&request.path)
        {
            // Only a temporary strong reference during admission, never a registry/runtime cycle.
            let runtime = runtime
                .get()
                .and_then(std::sync::Weak::upgrade)
                .ok_or_else(|| anyhow::anyhow!("LAN runtime unavailable"))?;
            Ok(Err((
                runtime,
                super::lan_http_live::upgrade_request(request, backend),
            )))
        } else {
            forward(request, backend, ingress).map(Ok)
        }
    });
    if let Ok(Err((runtime, request))) = result {
        crate::handle_live_media_websocket_upgrade(
            crate::LanMediaStream::new(stream, stop),
            request,
            &runtime,
        );
        return;
    }
    let result =
        result.and_then(|value| value.map_err(|_| anyhow::anyhow!("Invalid media handoff")));
    let (status, body) = result.unwrap_or_else(|_| {
        (
            400,
            br#"{"error":{"code":"lan_request_rejected"}}"#.to_vec(),
        )
    });
    stream
        .sock
        .set_operation_deadline(Instant::now() + Duration::from_secs(3));
    let _ = write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

#[cfg(test)]
#[path = "lan_http_offline_tests.rs"]
mod offline_tests;

#[cfg(test)]
#[path = "lan_http_live_tests.rs"]
mod live_tests;

#[cfg(test)]
#[path = "lan_http_surface_tests.rs"]
mod surface_tests;
#[cfg(test)]
#[path = "lan_http_wall_tests.rs"]
mod wall_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_lan_adapter_only_admits_device_report() {
        assert!(allowed("POST", "/v1/otter/locale-report"));
        assert!(!allowed("GET", "/v1/otter/locale-report"));
        assert!(!allowed("POST", "/v1/devices/device-one/locale-requests"));
        assert!(!allowed("POST", "/v1/otter/locale-report/extra"));
        assert_eq!(body_limit("/v1/otter/locale-report"), 8 * 1024);
    }

    #[test]
    fn local_art_routes_preserve_device_only_exact_scope_and_body_limits() {
        assert!(allowed("GET", "/v1/art/local/issuer"));
        assert!(!allowed("POST", "/v1/art/local/issuer"));
        for (path, limit) in [
            ("/v1/art/local/describe", 16 * 1024),
            ("/v3/art/local/prepare-package", 16 * 1024),
            ("/v1/art/local/live-viewer/attach", 32 * 1024),
            ("/v1/art/local/live-viewer/control", 32 * 1024),
            ("/v1/art/local/live-trigger", 32 * 1024),
            ("/v1/art/local/service/submit", 96 * 1024),
            (
                "/v1/art/local/service/submit-images",
                loom_protocol::local_art::service::images::MAX_REQUEST,
            ),
            (
                "/v1/art/local/service/submit-image-output",
                loom_protocol::local_art::service::images::MAX_REQUEST,
            ),
            ("/v1/art/local/service/poll-image-output", 96 * 1024),
            ("/v1/art/local/service/poll", 96 * 1024),
            ("/v1/art/local/service/cancel", 96 * 1024),
        ] {
            assert!(allowed("POST", path));
            assert!(!allowed("GET", path));
            assert!(!allowed("PUT", path));
            assert!(!allowed("POST", &format!("{path}/")));
            assert!(!allowed("POST", &format!("{path}?admin=true")));
            assert_eq!(body_limit(path), limit);
            let head = format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1:8766\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", limit + 1);
            assert!(read_request(&mut head.as_bytes(), "127.0.0.1:8766").is_err());
        }
        for path in [
            "/v1/art/local/prepare",
            "/v1/art/local/prepare-cached",
            "/v1/art/local/prepare-resumable",
            "/v1/art/local/prepare-package",
            "/v2/art/local/prepare-package",
            "/v1/art/local/approvals",
            "/v1/invoke",
            "/v1/art/local/service/admin",
        ] {
            assert!(!allowed("POST", path));
            assert!(!allowed("GET", path));
        }
    }
    #[test]
    fn shared_projection_routes_have_bounded_image_budget() {
        for route in [
            "create", "inspect", "accept", "targets", "inbox", "receipt", "edit", "update", "read",
            "unlink",
        ] {
            let path = format!("/v1/projections/{route}");
            assert!(allowed("POST", &path));
            assert!(!allowed("GET", &path));
            assert_eq!(
                body_limit(&path),
                if route == "edit" {
                    256 * 1024
                } else {
                    loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
                }
            );
        }
        assert!(!allowed("POST", "/v1/projections/create?admin=true"));
        assert!(!allowed("POST", "/v1/projections/configuration"));
        assert_eq!(body_limit("/v1/otter/connections/request"), MAX_BODY);
    }
    #[test]
    fn lan_route_boundary_rejects_management_and_request_smuggling() {
        for path in [
            "/status",
            "/v1/otter/connections/policy",
            "/v1/devices",
            "/v1/otter/identity?token=x",
            "http://evil/v1/otter/identity",
        ] {
            assert!(!allowed("GET", path));
            assert!(!allowed("POST", path));
        }
        let valid = "GET /v1/otter/identity HTTP/1.1\r\nHost: 192.168.1.2:8766\r\n";
        assert!(read_request(&mut format!("{valid}\r\n").as_bytes(), "192.168.1.2:8766").is_ok());
        for extra in [
            "Authorization: Bearer admin\r\n",
            "Cookie: loom_admin=secret\r\n",
            "Origin: https://evil\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Host: other\r\n",
            "Content-Length: 1048577\r\n",
        ] {
            assert!(read_request(
                &mut format!("{valid}{extra}\r\n").as_bytes(),
                "192.168.1.2:8766"
            )
            .is_err());
        }
    }
}
