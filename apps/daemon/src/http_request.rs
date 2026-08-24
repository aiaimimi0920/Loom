//! Bounded HTTP/1 request intake, framing, parsing, and credential access.

use super::*;

pub(super) enum HttpReadOutcome {
    Empty,
    Request(Vec<u8>),
    Rejected { status: u16, body: String },
}

/// How much of a request is read per `read` call.
///
/// A `MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES` install is 96 MiB, which at the previous 512 bytes was
/// about 196 000 syscalls for one upload.
pub(super) const HTTP_READ_CHUNK_BYTES: usize = 64 * 1024;

/// How long one request may take to arrive in full.
///
/// `CONNECTION_READ_TIMEOUT_MILLIS` bounds a single `read`; it does not bound the request. A client
/// that sends one byte just inside every read timeout satisfies the per-read timeout forever, so
/// the whole read needs a wall-clock deadline of its own. Production callers configure that
/// per-read timeout before entering this generic `Read` loop.
pub(super) const MAX_REQUEST_READ_MILLIS: u64 = 30_000;

/// What the reader needs to know about a request head while it is still waiting for the body.
///
/// Parsed once, when the header terminator first appears. The offset of that terminator cannot move
/// afterwards, so neither the size limit nor the completeness check has to re-scan the bytes already
/// received — which, for a large upload, meant re-scanning the whole accumulated buffer per chunk.
struct RequestHead {
    header_end: usize,
    content_length: usize,
    body_limit: usize,
}

impl RequestHead {
    fn body_start(&self) -> usize {
        self.header_end + 4
    }

    fn exceeds_size_limit(&self, received: usize) -> bool {
        self.header_end > MAX_HTTP_HEADER_BYTES
            || self.content_length > self.body_limit
            || received.saturating_sub(self.body_start()) > self.body_limit
    }

    fn has_full_body(&self, received: usize) -> bool {
        received.saturating_sub(self.body_start()) >= self.content_length
    }

    fn has_excess_body(&self, received: usize) -> bool {
        received.saturating_sub(self.body_start()) > self.content_length
    }
}

/// Locates the header terminator and reads the two headers the reader depends on.
///
/// `scan_from` is where the search for the terminator starts; a caller that has already searched a
/// prefix passes the offset three bytes back from the end of it, so a terminator split across two
/// reads is still found.
fn parse_request_head(
    request: &[u8],
    scan_from: usize,
) -> std::result::Result<Option<RequestHead>, &'static str> {
    let Some(tail) = request.get(scan_from..) else {
        return Ok(None);
    };
    let Some(relative_end) = tail.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let header_end = scan_from + relative_end;
    let header_text = std::str::from_utf8(&request[..header_end])
        .map_err(|_| "request headers must be valid UTF-8")?;
    let content_length = validate_request_head(header_text)?;
    Ok(Some(RequestHead {
        header_end,
        content_length,
        body_limit: request_body_size_limit(&header_text),
    }))
}

fn validate_request_head(headers: &str) -> std::result::Result<usize, &'static str> {
    let mut lines = headers.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or("request line is missing")?
        .split_whitespace();
    let method = request_line.next().ok_or("request method is missing")?;
    let path = request_line.next().ok_or("request path is missing")?;
    let version = request_line.next().ok_or("HTTP version is missing")?;
    if request_line.next().is_some()
        || !is_http_token(method)
        || !path.starts_with('/')
        || path.bytes().any(|byte| byte.is_ascii_control())
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
    {
        return Err("request line is invalid");
    }

    let mut content_length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("request header is malformed")?;
        if !is_http_token(name) || value.bytes().any(is_invalid_header_value_byte) {
            return Err("request header is invalid");
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("transfer encoding is not supported");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err("duplicate content length is not allowed");
            }
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("content length is invalid");
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "content length is invalid")?,
            );
        }
    }
    Ok(content_length.unwrap_or(0))
}

fn is_http_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn is_invalid_header_value_byte(byte: u8) -> bool {
    (byte < b' ' && byte != b'\t') || byte == 0x7f
}

/// The deadline-free reader the read tests use: they drive a complete byte source and have no
/// wall-clock behaviour to state.
#[cfg(test)]
pub(super) fn read_http_request(stream: &mut impl Read) -> Result<HttpReadOutcome> {
    let deadline = Instant::now() + Duration::from_millis(MAX_REQUEST_READ_MILLIS);
    read_http_request_until(stream, deadline, &AtomicBool::new(false))
}

/// Reads one request, answering 408 at `deadline` and giving up once `abort` is set.
pub(super) fn read_http_request_until(
    stream: &mut impl Read,
    mut deadline: Instant,
    abort: &AtomicBool,
) -> Result<HttpReadOutcome> {
    let mut request = Vec::new();
    let mut buffer = vec![0_u8; HTTP_READ_CHUNK_BYTES];
    let mut head: Option<RequestHead> = None;
    let mut shutdown_grace_applied = false;
    loop {
        // A request that has already begun arriving is worth a bounded grace period at shutdown:
        // dropping the socket with its bytes still unread makes Windows answer the client with an
        // RST, so a request that was about to be answerable becomes a reset instead of a 503. One
        // that has sent nothing has nothing to salvage and goes immediately.
        if abort.load(Ordering::SeqCst) {
            if request.is_empty() {
                return Ok(HttpReadOutcome::Empty);
            }
            if !shutdown_grace_applied {
                shutdown_grace_applied = true;
                let grace = Instant::now() + Duration::from_millis(SHUTDOWN_READ_GRACE_MILLIS);
                if grace < deadline {
                    deadline = grace;
                }
            }
        }
        if Instant::now() >= deadline {
            return Ok(request_timeout_response());
        }
        match stream.read(&mut buffer) {
            Ok(0) if request.is_empty() => return Ok(HttpReadOutcome::Empty),
            Ok(0) => break,
            Ok(bytes) => {
                let scan_from = request.len().saturating_sub(3);
                request.extend_from_slice(&buffer[..bytes]);
                if head.is_none() {
                    head = match parse_request_head(&request, scan_from) {
                        Ok(head) => head,
                        Err(_) => return Ok(invalid_request_response()),
                    };
                    if head.is_none() && request.len() > MAX_HTTP_HEADER_BYTES {
                        return Ok(payload_too_large_response());
                    }
                }
                if let Some(head) = head.as_ref() {
                    if head.exceeds_size_limit(request.len()) {
                        return Ok(payload_too_large_response());
                    }
                    let expected = head.body_start().saturating_add(head.content_length);
                    if request.capacity() < expected
                        && request
                            .try_reserve_exact(expected.saturating_sub(request.len()))
                            .is_err()
                    {
                        return Ok(payload_too_large_response());
                    }
                    if head.has_excess_body(request.len()) {
                        return Ok(invalid_request_response());
                    }
                    if head.has_full_body(request.len()) {
                        break;
                    }
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                    && request.is_empty() =>
            {
                return Ok(HttpReadOutcome::Empty);
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Ok(request_timeout_response());
            }
            Err(error) => return Err(error).context("read daemon request"),
        }
    }

    let Some(head) = head else {
        return Ok(invalid_request_response());
    };
    let body = &request[head.body_start()..];
    if body.len() != head.content_length || std::str::from_utf8(body).is_err() {
        return Ok(invalid_request_response());
    }

    // The accumulated buffer is handed over rather than copied into a `String`: for a package install
    // that copy, plus the one the parser used to make of the body, was two extra copies of the whole
    // upload resident at once.
    Ok(HttpReadOutcome::Request(request))
}

/// Converts owned bytes to text the way `String::from_utf8_lossy` would, without copying when the
/// bytes are already valid UTF-8 — which, for a request body, is the case that matters.
fn into_lossy_string(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
    }
}

fn payload_too_large_response() -> HttpReadOutcome {
    HttpReadOutcome::Rejected {
        status: 413,
        body: json!({
            "error": {
                "code": "payload_too_large",
                "message": "request body is too large"
            },
            "status": "failed"
        })
        .to_string(),
    }
}

fn invalid_request_response() -> HttpReadOutcome {
    HttpReadOutcome::Rejected {
        status: 400,
        body: json!({
            "error": {
                "code": "invalid_request",
                "message": "request framing or encoding is invalid"
            },
            "status": "failed"
        })
        .to_string(),
    }
}

/// Answer for a request that never finished arriving.
///
/// 408 rather than 413: nothing about the request was too large, it simply never turned up in full.
fn request_timeout_response() -> HttpReadOutcome {
    HttpReadOutcome::Rejected {
        status: 408,
        body: json!({
            "error": {
                "code": "request_timeout",
                "message": "request was not received in full before the read deadline"
            },
            "status": "failed"
        })
        .to_string(),
    }
}

/// The whole-buffer form of the check the reader now makes incrementally, kept for the tests that
/// state the limit rules against a complete request. It delegates rather than reimplementing, so it
/// cannot drift from what the reader does.
#[cfg(test)]
pub(super) fn request_exceeds_size_limit(request: &[u8]) -> bool {
    match parse_request_head(request, 0) {
        Ok(Some(head)) => head.exceeds_size_limit(request.len()),
        Ok(None) => request.len() > MAX_HTTP_HEADER_BYTES,
        Err(_) => false,
    }
}

fn request_body_size_limit(headers: &str) -> usize {
    let mut request_line = headers
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = request_line.next().unwrap_or_default();
    let path = request_line.next().unwrap_or_default();
    let is_package_install = matches!(path, "/v1/frameworks/install" | "/v1/arts/install");
    let is_mcp_server_package_install = path == "/v1/mcp/servers/install";
    let is_framework_upgrade = path.starts_with("/v1/frameworks/") && path.ends_with("/upgrade");
    let is_surface_resource = path == "/v1/surfaces/resources";
    if method.eq_ignore_ascii_case("POST") && is_mcp_server_package_install {
        MAX_MCP_SERVER_PACKAGE_HTTP_BODY_BYTES
    } else if method.eq_ignore_ascii_case("POST")
        && (is_package_install || is_framework_upgrade || is_surface_resource)
    {
        MAX_PACKAGE_HTTP_BODY_BYTES
    } else {
        MAX_HTTP_BODY_BYTES
    }
}

#[derive(Debug)]
pub(super) struct ParsedHttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: String,
}

impl ParsedHttpRequest {
    /// Takes ownership of the bytes the reader accumulated and moves the body out of them.
    ///
    /// Only the bounded header is copied. Draining its prefix shifts the body within the existing
    /// allocation, avoiding a second package-sized allocation before UTF-8 conversion.
    pub(super) fn from_raw(mut raw: Vec<u8>) -> Self {
        let head = match raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|header_end| header_end + 4)
        {
            Some(body_start) => {
                let head = raw[..body_start - 4].to_vec();
                raw.drain(..body_start);
                head
            }
            None => Vec::new(),
        };
        let head = String::from_utf8_lossy(&head);
        let mut lines = head.lines();
        let mut request_line = lines.next().unwrap_or("").split_whitespace();
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_string(), value.trim().to_string()))
            })
            .collect();
        Self {
            method: request_line.next().unwrap_or("GET").to_string(),
            path: request_line.next().unwrap_or("/").to_string(),
            headers,
            body: into_lossy_string(raw),
        }
    }

    pub(super) fn has_bearer(&self, token: &str) -> bool {
        self.authorization_credential("bearer") == Some(token)
    }

    pub(super) fn has_admin_credential(&self, token: &str) -> bool {
        self.has_bearer(token)
            || self.header("cookie").is_some_and(|cookies| {
                cookies.split(';').any(|cookie| {
                    let Some((name, value)) = cookie.trim().split_once('=') else {
                        return false;
                    };
                    name == ADMIN_AUTH_COOKIE_NAME
                        && percent_decode_component(value).as_deref() == Some(token)
                })
            })
    }

    pub(super) fn query_parameter(&self, expected_name: &str) -> Option<String> {
        let (_, query) = self.path.split_once('?')?;
        query.split('&').find_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode_component(name).as_deref() == Some(expected_name))
                .then(|| percent_decode_component(value))
                .flatten()
        })
    }

    pub(super) fn authorization_credential(&self, expected_scheme: &str) -> Option<&str> {
        let mut credentials = self.headers.iter().filter_map(|(name, value)| {
            if !name.eq_ignore_ascii_case("authorization") {
                return None;
            }
            let mut parts = value.split_whitespace();
            let scheme = parts.next()?;
            let credential = parts.next()?;
            (scheme.eq_ignore_ascii_case(expected_scheme) && parts.next().is_none())
                .then_some(credential)
        });
        let credential = credentials.next()?;
        credentials.next().is_none().then_some(credential)
    }

    pub(super) fn header(&self, expected_name: &str) -> Option<&str> {
        let mut values = self
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(expected_name))
            .map(|(_, value)| value.as_str());
        let value = values.next()?;
        values.next().is_none().then_some(value)
    }

    pub(super) fn header_count(&self, expected_name: &str) -> usize {
        self.headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(expected_name))
            .count()
    }
}

fn percent_decode_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let high = *bytes.get(index + 1)?;
                let low = *bytes.get(index + 2)?;
                let high = (high as char).to_digit(16)? as u8;
                let low = (low as char).to_digit(16)? as u8;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).ok()
}
