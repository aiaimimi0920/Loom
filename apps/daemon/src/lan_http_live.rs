//! Exact LAN Live route and upgrade boundary; authorization remains with the runtime.
use super::lan_http::Request;
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};

pub(super) fn media(path: &str) -> bool {
    let Some(query) = path.strip_prefix("/v1/live/media?") else {
        return false;
    };
    let Some(values) = query_values(query, &["sessionId", "role", "afterEpoch", "afterFrameId"])
    else {
        return false;
    };
    values
        .get("sessionId")
        .is_some_and(|id| loom_protocol::validate_live_identifier(id, "session_id").is_ok())
        && matches!(
            values.get("role").map(String::as_str),
            Some("source" | "viewer")
        )
        && ["afterEpoch", "afterFrameId"]
            .iter()
            .all(|key| values.get(*key).is_none_or(|value| unsigned(value)))
}

pub(super) fn allowed(method: &str, path: &str) -> bool {
    if method == "GET" && media(path) {
        return true;
    }
    let (route, query) = path
        .split_once('?')
        .map_or((path, None), |(p, q)| (p, Some(q)));
    if matches!(
        (method, route),
        ("GET", "/v1/live/status" | "/v1/live/sessions") | ("POST", "/v1/live/sessions")
    ) {
        return query.is_none();
    }
    let Some(tail) = route.strip_prefix("/v1/live/sessions/") else {
        return false;
    };
    let (id, suffix) = tail
        .split_once('/')
        .map_or((tail, None), |(id, s)| (id, Some(s)));
    // A segment is decoded exactly once by the Live route owner, never by reqwest.
    if id.is_empty()
        || id
            .bytes()
            .any(|b| !b.is_ascii_graphic() || matches!(b, b'\\' | b'#'))
    {
        return false;
    }
    let Some(decoded) = crate::percent_decode_component(id) else {
        return false;
    };
    if matches!(decoded.as_str(), "." | "..")
        || loom_protocol::validate_live_identifier(&decoded, "session_id").is_err()
    {
        return false;
    }
    match (method, suffix, query) {
        ("GET", None, None) => true,
        ("GET", Some("events"), Some(query)) => query_values(query, &["after", "timeoutMs"])
            .is_some_and(|values| values.values().all(|value| unsigned(value))),
        ("GET", Some("events"), None) => true,
        (
            "POST",
            Some(
                "viewers" | "direct-viewer" | "resume" | "control" | "input" | "observations"
                | "triggers" | "close",
            ),
            None,
        ) => true,
        _ => false,
    }
}

pub(super) fn unsigned(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) && value.parse::<u64>().is_ok()
}

pub(super) fn query_values(
    query: &str,
    keys: &[&str],
) -> Option<std::collections::HashMap<String, String>> {
    let mut values = std::collections::HashMap::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        // Names are canonical ASCII. Encoded aliases and duplicate names are rejected.
        if !keys.contains(&key) {
            return None;
        }
        let value = crate::percent_decode_component(value)?;
        if values.insert(key.to_owned(), value).is_some() {
            return None;
        }
    }
    Some(values)
}

pub(super) fn validate(request: &Request) -> Result<()> {
    ensure!(
        request.method == "GET" && media(&request.path),
        "Invalid LAN media route"
    );
    validate_upgrade(request)
}
/// Shares only Device/nonce/WS key and empty-body bounds; each protocol still owns its exact path, query, and profile.
pub(super) fn validate_upgrade(request: &Request) -> Result<()> {
    let headers = &request.headers;
    ensure!(request.body.is_empty(), "LAN media body is not allowed");
    ensure!(
        headers
            .get("authorization")
            .is_some_and(|s| s.starts_with("Device ") && s.len() > 7),
        "Device session required"
    );
    ensure!(
        headers
            .get("x-loom-device-nonce")
            .is_some_and(|s| !s.is_empty() && s.len() <= 256),
        "Device nonce required"
    );
    let key = headers
        .get("sec-websocket-key")
        .ok_or_else(|| anyhow::anyhow!("WebSocket key required"))?;
    ensure!(
        STANDARD.decode(key).is_ok_and(|bytes| bytes.len() == 16),
        "Invalid WebSocket key"
    );
    Ok(())
}

pub(super) fn upgrade_request(
    request: Request,
    backend: std::net::SocketAddr,
) -> crate::ParsedHttpRequest {
    let mut headers = vec![("host".into(), backend.to_string())];
    // Host has already been checked against the actual LAN listener. Never inherit
    // caller origin/ingress claims or administrator credentials into the runtime.
    for name in [
        "authorization",
        "x-loom-device-nonce",
        "upgrade",
        "connection",
        "sec-websocket-version",
        "sec-websocket-key",
        "sec-websocket-protocol",
    ] {
        if let Some(value) = request.headers.get(name) {
            headers.push((name.into(), value.clone()));
        }
    }
    crate::ParsedHttpRequest {
        method: request.method,
        path: request.path,
        headers,
        body: String::new(),
    }
}
