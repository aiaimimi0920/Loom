//! Shared bounded HTTP response intake for the loopback JSON and image clients.

use super::*;

pub(super) struct ParsedHttpResponse {
    pub status_line: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub body_offset: usize,
}

pub(super) fn read_bounded_http_response<R: Read>(
    stream: &mut R,
    path: &str,
    max_body_bytes: usize,
) -> Result<Vec<u8>, String> {
    let max_total = MAX_DAEMON_RESPONSE_HEADER_BYTES
        .checked_add(4)
        .and_then(|value| value.checked_add(max_body_bytes))
        .ok_or_else(|| "Loom local service response size limit overflowed.".to_owned())?;
    let mut raw = Vec::new();
    stream
        .take(max_total as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|error| format!("Failed to read Loom local service response {path}: {error}"))?;
    if raw.len() > max_total {
        return Err(format!(
            "Loom local service response exceeds the {max_body_bytes}-byte body limit: {path}"
        ));
    }
    Ok(raw)
}

pub(super) fn parse_http_response(
    raw: &[u8],
    path: &str,
    max_body_bytes: usize,
) -> Result<ParsedHttpResponse, String> {
    let separator = b"\r\n\r\n";
    let header_end = raw
        .windows(separator.len())
        .position(|window| window == separator)
        .ok_or_else(|| format!("Malformed Loom local service response: {path}"))?;
    if header_end > MAX_DAEMON_RESPONSE_HEADER_BYTES {
        return Err(format!(
            "Loom local service response headers are too large: {path}"
        ));
    }
    let body_offset = header_end + separator.len();
    let body_len = raw.len().saturating_sub(body_offset);
    if body_len > max_body_bytes {
        return Err(format!(
            "Loom local service response exceeds the {max_body_bytes}-byte body limit: {path}"
        ));
    }

    let headers = std::str::from_utf8(&raw[..header_end]).map_err(|error| {
        format!("Loom local service response headers are not UTF-8: {path}: {error}")
    })?;
    let status_line = headers.lines().next().unwrap_or("unknown status");
    let mut status_parts = status_line.split_whitespace();
    let version = status_parts.next().unwrap_or_default();
    let status = status_parts.next().unwrap_or_default();
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || status.len() != 3
        || !status.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!(
            "Invalid Loom local service response status: {path} returned {status_line}"
        ));
    }
    let status_code = status.parse::<u16>().map_err(|error| {
        format!(
            "Invalid Loom local service response status: {path} returned {status_line}: {error}"
        )
    })?;

    let mut content_length = None;
    let mut content_type = None;
    for line in headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!(
                "Malformed Loom local service response header: {path}"
            ));
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") && !value.eq_ignore_ascii_case("identity")
        {
            return Err(format!(
                "Loom local service response uses an unsupported transfer encoding: {path}"
            ));
        }
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = value.parse::<usize>().map_err(|error| {
                format!("Invalid Loom local service Content-Length: {path}: {error}")
            })?;
            if content_length.replace(parsed).is_some() {
                return Err(format!(
                    "Loom local service response contains duplicate Content-Length headers: {path}"
                ));
            }
        } else if name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.to_owned());
        }
    }
    if content_length.is_some_and(|expected| expected != body_len) {
        return Err(format!(
            "Loom local service response body length mismatch: {path}"
        ));
    }

    Ok(ParsedHttpResponse {
        status_line: status_line.to_owned(),
        status_code,
        content_type,
        body_offset,
    })
}

pub(super) fn is_json_content_type(value: &str) -> bool {
    let essence = value.split(';').next().unwrap_or_default().trim();
    essence.eq_ignore_ascii_case("application/json")
        || essence.eq_ignore_ascii_case("text/json")
        || essence
            .to_ascii_lowercase()
            .strip_prefix("application/")
            .is_some_and(|subtype| subtype.ends_with("+json"))
}
