//! Local Loom art store server.
//!
//! A tiny std-TCP HTTP server backing the daemon's art-store client. Serves an
//! art catalog, raw art packages, third-party portable binaries, and accepts
//! published packages. Data lives under a store root:
//!   <root>/arts/<id>/<version>.zip immutable art package versions
//!   <root>/binaries/<name>      third-party portable executables
//!
//! Endpoints (matching the daemon's client contract):
//!   GET  /catalog               -> version-aware Art catalog
//!   GET  /arts/<id>/<version>.zip -> exact package version
//!   GET  /arts/<id>/<version>.zip.sha256 -> package digest sidecar (text/plain)
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
use std::sync::{mpsc, Arc, Mutex};

use anyhow::{Context, Result};
use loom_art_store::{
    build_catalog, read_art_zip_version, read_art_zip_version_sha256, read_binary,
    read_framework_package, read_publisher, register_publisher_with_id, rotate_publisher_key,
    store_verified_published_zip, PublisherRotationRequest, StoreError, MAX_PUBLISHED_ZIP_BYTES,
};

const MAX_REQUEST_HEADER_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = MAX_PUBLISHED_ZIP_BYTES as usize;
const CONNECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const CONNECTION_WORKERS: usize = 4;
const CONNECTION_QUEUE_CAPACITY: usize = 32;

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

    serve(listener, root)
}

fn serve(listener: TcpListener, root: PathBuf) -> Result<()> {
    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(CONNECTION_QUEUE_CAPACITY);
    let receiver = Arc::new(Mutex::new(receiver));
    let root = Arc::new(root);
    for _ in 0..CONNECTION_WORKERS {
        let receiver = Arc::clone(&receiver);
        let root = Arc::clone(&root);
        std::thread::spawn(move || loop {
            let stream = {
                let receiver = receiver
                    .lock()
                    .expect("Art Store connection queue poisoned");
                receiver.recv()
            };
            let Ok(stream) = stream else {
                break;
            };
            if let Err(error) = handle_connection(stream, &root) {
                eprintln!("art store: connection error: {error}");
            }
        });
    }
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match sender.try_send(stream) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(mut stream)) => {
                    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
                    let response =
                        Response::json(503, serde_json::json!({ "error": "Art Store is busy" }));
                    let _ = write_response(&mut stream, response);
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    anyhow::bail!("Art Store connection workers stopped")
                }
            },
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
    peer_is_loopback: bool,
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
    stream.set_read_timeout(Some(CONNECTION_TIMEOUT))?;
    stream.set_write_timeout(Some(CONNECTION_TIMEOUT))?;
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
    let peer_is_loopback = stream
        .peer_addr()
        .map(|address| address.ip().is_loopback())
        .unwrap_or(false);
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
        if buf.len() > MAX_REQUEST_HEADER_BYTES {
            anyhow::bail!("request headers too large");
        }
    };

    let header_text =
        std::str::from_utf8(&buf[..header_end]).context("request headers are not valid UTF-8")?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(raw_target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        anyhow::bail!("malformed request line");
    };
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") || !raw_target.starts_with('/') {
        anyhow::bail!("unsupported HTTP request line");
    }
    let method = method.to_owned();
    let raw_target = raw_target.to_owned();
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
        let Some((key, value)) = line.split_once(':') else {
            anyhow::bail!("malformed request header");
        };
        if key != key.trim()
            || key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value
                .bytes()
                .any(|byte| (byte < b' ' && byte != b'\t') || byte == 0x7f)
        {
            anyhow::bail!("invalid request header");
        }
        headers.push((key.to_owned(), value.trim().to_owned()));
    }

    if headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("transfer-encoding"))
    {
        anyhow::bail!("transfer-encoding is not supported");
    }
    let content_lengths = headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>())
        .collect::<Result<Vec<_>, _>>()?;
    if content_lengths
        .windows(2)
        .any(|values| values[0] != values[1])
    {
        anyhow::bail!("conflicting content-length headers");
    }
    let content_length = content_lengths.first().copied().unwrap_or(0);
    if content_length > MAX_REQUEST_BODY_BYTES {
        anyhow::bail!("request body too large");
    }

    let mut body = buf[header_end + 4..].to_vec();
    if body.len() > MAX_REQUEST_BODY_BYTES {
        anyhow::bail!("request body too large");
    }
    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > MAX_REQUEST_BODY_BYTES {
            anyhow::bail!("request body too large");
        }
    }
    if body.len() != content_length {
        anyhow::bail!("request body length does not match content-length");
    }

    Ok(Some(Request {
        method,
        path,
        headers,
        body,
        peer_is_loopback,
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
        ("POST", "/publishers/register") if request.peer_is_loopback => {
            handle_publisher_register(request, root)
        }
        ("POST", "/publishers/register") => Response::json(
            403,
            serde_json::json!({ "error": "publisher registration requires a local connection" }),
        ),
        ("POST", path) if path.starts_with("/publishers/") && path.ends_with("/rotate") => {
            let user_id = path
                .trim_start_matches("/publishers/")
                .trim_end_matches("/rotate")
                .trim_end_matches('/');
            handle_publisher_rotate(request, root, user_id)
        }
        ("GET", path) if path.starts_with("/publishers/") => {
            let user_id = path.trim_start_matches("/publishers/");
            match read_publisher(root, user_id) {
                Ok(Some(publisher)) => {
                    Response::json(200, serde_json::json!({ "publisher": publisher }))
                }
                Ok(None) => Response::json(
                    404,
                    serde_json::json!({ "error": format!("publisher `{user_id}` not found") }),
                ),
                Err(error) => store_error_response(error),
            }
        }
        ("POST", "/publish") => handle_publish(request, root),
        ("GET", path) if path.starts_with("/arts/") => {
            let file = &path["/arts/".len()..];
            if let Some((id, version_file)) = file.split_once('/') {
                if let Some(version) = version_file.strip_suffix(".zip.sha256") {
                    return match read_art_zip_version_sha256(root, id, version) {
                        Ok(Some(bytes)) => Response::bytes(200, "text/plain; charset=utf-8", bytes),
                        Ok(None) => Response::json(
                            404,
                            serde_json::json!({ "error": format!("art `{id}` version `{version}` not found") }),
                        ),
                        Err(error) => store_error_response(error),
                    };
                }
                let Some(version) = version_file.strip_suffix(".zip") else {
                    return Response::json(
                        404,
                        serde_json::json!({ "error": "versioned art package must end with .zip" }),
                    );
                };
                return match read_art_zip_version(root, id, version) {
                    Ok(Some(bytes)) => Response::bytes(200, "application/zip", bytes),
                    Ok(None) => Response::json(
                        404,
                        serde_json::json!({ "error": format!("art `{id}` version `{version}` not found") }),
                    ),
                    Err(error) => store_error_response(error),
                };
            }
            Response::json(
                404,
                serde_json::json!({ "error": "art package requests require an exact version" }),
            )
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
                    serde_json::json!({ "error": "framework package must end with .zip" }),
                );
            };
            match read_framework_package(root, id) {
                Ok(Some(bytes)) => Response::bytes(200, "application/zip", bytes),
                Ok(None) => Response::json(
                    404,
                    serde_json::json!({ "error": format!("framework package `{id}` not found") }),
                ),
                Err(error) => store_error_response(error),
            }
        }
        ("GET", _) => Response::json(404, serde_json::json!({ "error": "not found" })),
        _ => Response::json(405, serde_json::json!({ "error": "method not allowed" })),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterPublisherRequest {
    #[serde(default)]
    user_id: Option<String>,
    key_id: String,
    public_key: String,
}

fn handle_publisher_register(request: &Request, root: &std::path::Path) -> Response {
    let input = match serde_json::from_slice::<RegisterPublisherRequest>(&request.body) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("art store: invalid publisher registration JSON: {error}");
            return Response::json(
                400,
                serde_json::json!({ "error": "invalid publisher registration request" }),
            );
        }
    };
    match register_publisher_with_id(
        root,
        input.user_id.as_deref(),
        &input.key_id,
        &input.public_key,
    ) {
        Ok(publisher) => Response::json(200, serde_json::json!({ "publisher": publisher })),
        Err(error) => store_error_response(error),
    }
}

fn handle_publisher_rotate(request: &Request, root: &std::path::Path, user_id: &str) -> Response {
    let input = match serde_json::from_slice::<PublisherRotationRequest>(&request.body) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("art store: invalid publisher rotation JSON: {error}");
            return Response::json(
                400,
                serde_json::json!({ "error": "invalid publisher rotation request" }),
            );
        }
    };
    match rotate_publisher_key(root, user_id, &input) {
        Ok(publisher) => Response::json(200, serde_json::json!({ "publisher": publisher })),
        Err(error) => store_error_response(error),
    }
}

fn handle_publish(request: &Request, root: &std::path::Path) -> Response {
    if request.body.is_empty() {
        return Response::json(400, serde_json::json!({ "error": "empty publish body" }));
    }
    let declared = request.header("X-Art-Id");
    match store_verified_published_zip(root, declared, &request.body) {
        Ok(published) => Response::json(
            200,
            serde_json::json!({
                "artId": published.art_id,
                "globalId": published.global_id,
                "published": true
            }),
        ),
        Err(error) => store_error_response(error),
    }
}

fn store_error_response(error: StoreError) -> Response {
    let (status, message) = match &error {
        StoreError::InvalidArtId(_)
        | StoreError::InvalidResourceName(_)
        | StoreError::MissingManifest
        | StoreError::MissingPublisher(_)
        | StoreError::MissingFramework(_)
        | StoreError::ArtIdMismatch { .. }
        | StoreError::InvalidVersion { .. }
        | StoreError::VersionConflict { .. }
        | StoreError::IdentityConflict { .. }
        | StoreError::InvalidPublisherId(_)
        | StoreError::InvalidPublisherKeyId(_)
        | StoreError::InvalidPublisherPublicKey
        | StoreError::PublisherActiveKeyMissing(_)
        | StoreError::PublisherRotationSignature
        | StoreError::PublisherKeyConflict { .. }
        | StoreError::MissingPublisherSignature
        | StoreError::InvalidPublisherSignatureMetadata
        | StoreError::PublisherSignatureVerification
        | StoreError::ArchiveCompressionRatio(_)
        | StoreError::ArchiveSymbolicLink(_) => (400, error.to_string()),
        StoreError::Zip(_) | StoreError::Json(_) => {
            (400, "invalid package or JSON document".to_owned())
        }
        StoreError::PackageTooLarge(_)
        | StoreError::ArchiveEntryCount
        | StoreError::ArchiveEntryTooLarge { .. }
        | StoreError::ArchiveExpandedTooLarge(_)
        | StoreError::StoredResourceTooLarge(_) => {
            (413, "request or stored resource is too large".to_owned())
        }
        StoreError::PublisherNotFound(_) | StoreError::UnsafeStoredPath => {
            (404, "requested resource was not found".to_owned())
        }
        StoreError::GlobalIdExhausted
        | StoreError::PublisherIdExhausted
        | StoreError::UnsupportedOfficialCertificationSchema(_)
        | StoreError::UnsupportedPublisherDirectorySchema(_)
        | StoreError::PersistenceLockTimeout
        | StoreError::Io(_) => (500, "Art Store internal error".to_owned()),
    };
    if matches!(
        error,
        StoreError::Zip(_)
            | StoreError::Json(_)
            | StoreError::UnsafeStoredPath
            | StoreError::PersistenceLockTimeout
            | StoreError::Io(_)
    ) {
        eprintln!("art store: request failed: {error}");
    }
    Response::json(status, serde_json::json!({ "error": message }))
}

fn write_response(stream: &mut TcpStream, response: Response) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        503 => "Service Unavailable",
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

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
