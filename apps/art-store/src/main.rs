//! Local Loom art store server.
//!
//! A tiny std-TCP HTTP server backing the daemon's art-store client. Serves an
//! art catalog, raw art packages, third-party portable binaries, and accepts
//! published packages. Data lives under a store root:
//!   <root>/arts/<id>.zip        art packages
//!   <root>/binaries/<name>      third-party portable executables
//!
//! Endpoints (matching the daemon's client contract):
//!   GET  /catalog               -> { "arts": [ {id,name,description,framework} ] }
//!   GET  /arts/<id>.zip         -> raw art package bytes (application/zip)
//!   GET  /binaries/<name>       -> raw binary bytes (application/octet-stream)
//!   POST /publish               -> body = zip, header X-Art-Id: <id>
//!   GET  /health                -> { "ok": true }
//!
//! Configure with env: LOOM_ART_STORE_PORT (default 8790),
//! LOOM_ART_STORE_HOST (default 127.0.0.1), LOOM_ART_STORE_ROOT (default
//! %APPDATA%\Loom\art-store, or ./loom-art-store elsewhere).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use anyhow::{Context, Result};
use loom_art_store::{
    build_catalog, read_art_zip, read_binary, read_framework_runtime, store_published_zip,
    StoreError,
};

fn main() -> Result<()> {
    let port = std::env::var("LOOM_ART_STORE_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8790);
    let host = std::env::var("LOOM_ART_STORE_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let root = store_root();
    std::fs::create_dir_all(root.join(loom_art_store::ARTS_DIR))
        .with_context(|| format!("create arts dir under {}", root.display()))?;
    std::fs::create_dir_all(root.join(loom_art_store::BINARIES_DIR))
        .with_context(|| format!("create binaries dir under {}", root.display()))?;

    let listener = TcpListener::bind((host.as_str(), port))
        .with_context(|| format!("bind art store on {host}:{port}"))?;
    let address = listener.local_addr()?;
    println!("loom-art-store listening on http://{address}");
    println!("  store root: {}", root.display());
    println!("  set LOOM_ART_STORE_URL=http://{address} for the Loom daemon");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_connection(stream, &root) {
                    eprintln!("art store: connection error: {error}");
                }
            }
            Err(error) => eprintln!("art store: accept error: {error}"),
        }
    }
    Ok(())
}

/// Resolve the store root: LOOM_ART_STORE_ROOT wins, else a per-OS default.
fn store_root() -> PathBuf {
    if let Ok(explicit) = std::env::var("LOOM_ART_STORE_ROOT") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("Loom").join("art-store");
        }
    }
    PathBuf::from("loom-art-store")
}

/// A parsed HTTP request: method, path (no query), headers, and body bytes.
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn handle_connection(mut stream: TcpStream, root: &std::path::Path) -> Result<()> {
    let request = match read_request(&mut stream)? {
        Some(request) => request,
        None => return Ok(()),
    };
    let response = route(&request, root);
    write_response(&mut stream, response)
}

/// Read a full HTTP request off the socket: headers until CRLFCRLF, then the
/// body per Content-Length.
fn read_request(stream: &mut TcpStream) -> Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    // Read until we have the header terminator.
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            // Connection closed before a full header block.
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.len() > 64 * 1024 * 1024 {
            anyhow::bail!("request headers too large");
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let raw_target = parts.next().unwrap_or_default().to_owned();
    let path = raw_target
        .split('?')
        .next()
        .unwrap_or(&raw_target)
        .to_owned();

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    let mut body = buf[header_end + 4..].to_vec();
    if body.len() > 256 * 1024 * 1024 {
        anyhow::bail!("request body too large");
    }
    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > 256 * 1024 * 1024 {
            anyhow::bail!("request body too large");
        }
    }
    body.truncate(content_length);

    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// An outgoing response: status, content type, and body bytes.
struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(status: u16, value: serde_json::Value) -> Self {
        Response {
            status,
            content_type: "application/json; charset=utf-8",
            body: value.to_string().into_bytes(),
        }
    }

    fn bytes(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Response {
            status,
            content_type,
            body,
        }
    }
}

fn route(request: &Request, root: &std::path::Path) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => Response::json(200, serde_json::json!({ "ok": true })),
        ("GET", "/catalog") => match build_catalog(root) {
            Ok(arts) => Response::json(200, serde_json::json!({ "arts": arts })),
            Err(error) => store_error_response(error),
        },
        ("POST", "/publish") => handle_publish(request, root),
        ("GET", path) if path.starts_with("/arts/") => {
            let file = &path["/arts/".len()..];
            let Some(id) = file.strip_suffix(".zip") else {
                return Response::json(
                    404,
                    serde_json::json!({ "error": "art package must end with .zip" }),
                );
            };
            match read_art_zip(root, id) {
                Ok(Some(bytes)) => Response::bytes(200, "application/zip", bytes),
                Ok(None) => Response::json(
                    404,
                    serde_json::json!({ "error": format!("art `{id}` not found") }),
                ),
                Err(error) => store_error_response(error),
            }
        }
        ("GET", path) if path.starts_with("/binaries/") => {
            let name = &path["/binaries/".len()..];
            match read_binary(root, name) {
                Ok(Some(bytes)) => Response::bytes(200, "application/octet-stream", bytes),
                Ok(None) => Response::json(
                    404,
                    serde_json::json!({ "error": format!("binary `{name}` not found") }),
                ),
                Err(error) => store_error_response(error),
            }
        }
        ("GET", path) if path.starts_with("/frameworks/") => {
            let file = &path["/frameworks/".len()..];
            let Some(id) = file.strip_suffix(".zip") else {
                return Response::json(
                    404,
                    serde_json::json!({ "error": "framework runtime must end with .zip" }),
                );
            };
            match read_framework_runtime(root, id) {
                Ok(Some(bytes)) => Response::bytes(200, "application/zip", bytes),
                Ok(None) => Response::json(
                    404,
                    serde_json::json!({ "error": format!("framework runtime `{id}` not found") }),
                ),
                Err(error) => store_error_response(error),
            }
        }
        ("GET", _) => Response::json(404, serde_json::json!({ "error": "not found" })),
        _ => Response::json(405, serde_json::json!({ "error": "method not allowed" })),
    }
}

fn handle_publish(request: &Request, root: &std::path::Path) -> Response {
    if request.body.is_empty() {
        return Response::json(400, serde_json::json!({ "error": "empty publish body" }));
    }
    let declared = request.header("X-Art-Id");
    match store_published_zip(root, declared, &request.body) {
        Ok(id) => Response::json(200, serde_json::json!({ "artId": id, "published": true })),
        Err(error) => store_error_response(error),
    }
}

fn store_error_response(error: StoreError) -> Response {
    let status = match error {
        StoreError::InvalidArtId(_)
        | StoreError::InvalidResourceName(_)
        | StoreError::MissingManifest
        | StoreError::ArtIdMismatch { .. }
        | StoreError::Zip(_)
        | StoreError::Json(_) => 400,
        StoreError::Io(_) => 500,
    };
    Response::json(status, serde_json::json!({ "error": error.to_string() }))
}

fn write_response(stream: &mut TcpStream, response: Response) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()?;
    Ok(())
}
