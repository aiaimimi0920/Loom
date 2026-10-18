//! Restricted HTTPS-to-loopback adapter. Never forwards admin credentials, cookies,
//! browser origins, upgrades, arbitrary URLs or management routes.
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
    if method == "POST" && shared_projection(path) {
        return true;
    }
    match (method, path) {
        ("GET", "/v1/otter/identity") => true,
        (
            "POST",
            "/v1/otter/connections/request"
            | "/v1/otter/connections/status"
            | "/v1/otter/connections/cancel"
            | "/v1/otter/connections/activate"
            | "/v1/device-sessions/challenges"
            | "/v1/device-sessions"
            | "/v1/devices/requests",
        ) => true,
        ("POST", path) => matches!(
            path.strip_prefix("/v1/projections/v2/"),
            Some(
                "context"
                    | "configuration"
                    | "create"
                    | "inspect"
                    | "accept"
                    | "publish"
                    | "read"
                    | "unlink"
                    | "sync"
                    | "peer"
            )
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

fn body_limit(path: &str) -> usize {
    if shared_projection(path) {
        loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
    } else {
        MAX_BODY
    }
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
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
    for banned in ["origin", "cookie", "transfer-encoding", "upgrade", "expect"] {
        ensure!(!headers.contains_key(banned), "Unsupported LAN header");
    }
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
    Ok(Request {
        method: method.into(),
        path: path.into(),
        headers,
        body,
    })
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
    stream: &mut loom_local_channel::lan_tls::LanStream,
    host: &str,
    backend: SocketAddr,
    ingress: &str,
) {
    let result = read_request(stream, host).and_then(|request| forward(request, backend, ingress));
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
mod tests {
    use super::*;
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
                loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
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
