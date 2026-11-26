//! Exact Device LAN routes for native Tile; actions follow existing Wall permission checks without opening admin initiation.
use super::lan_http::Request;
use anyhow::{ensure, Result};
pub(super) fn allowed(method: &str, path: &str) -> bool {
    (method == "GET" && (path == "/v1/walls/state" || media(path)))
        || (method == "POST"
            && matches!(
                path,
                "/v1/walls/endpoints/register"
                    | "/v1/walls/endpoints/identify/report"
                    | "/v1/walls/connect"
                    | "/v1/walls/heartbeat"
                    | "/v1/walls/disconnect"
                    | "/v1/walls/images/read"
                    | "/v1/walls/surfaces/open"
                    | "/v1/walls/surfaces/state"
                    | "/v1/walls/surfaces/image"
                    | "/v1/walls/surfaces/close"
                    | "/v1/walls/surfaces/event"
                    | "/v1/walls/surfaces/confirmation"
                    | "/v1/walls/surfaces/cancel"
            ))
}
pub(super) fn matches(path: &str) -> bool {
    allowed("GET", path) || allowed("POST", path)
}
pub(super) fn validate(request: &Request) -> Result<()> {
    ensure!(
        allowed(&request.method, &request.path),
        "Invalid LAN Wall route"
    );
    if media(&request.path) {
        ensure!(request.method == "GET", "Invalid Wall media method");
        super::lan_http_live::validate_upgrade(request)?;
        ensure!(
            request
                .headers
                .get("sec-websocket-protocol")
                .map(String::as_str)
                == Some("loom.wall.media.v1"),
            "Invalid Wall media protocol"
        );
    }
    ensure!(
        request
            .headers
            .get("authorization")
            .is_some_and(|s| s.starts_with("Device ") && s.len() > 7),
        "Device session required"
    );
    ensure!(
        request
            .headers
            .get("x-loom-device-nonce")
            .is_some_and(|s| !s.is_empty() && s.len() <= 256),
        "Device nonce required"
    );
    // The runtime exclusively owns identity/nonce replay, endpoint ownership, CAS, leases, layout, and resource authorization rather than duplicating them in the adapter.
    Ok(())
}
pub(super) fn media(path: &str) -> bool {
    use super::lan_http_live::{query_values, unsigned};
    let Some(query) = path.strip_prefix("/v1/walls/live/media?") else {
        return false;
    };
    let Some(v) = query_values(
        query,
        &[
            "endpointId",
            "leaseId",
            "revision",
            "sessionId",
            "format",
            "maxWidth",
            "maxHeight",
            "maxFps",
        ],
    ) else {
        return false;
    };
    if !["endpointId", "leaseId", "sessionId"].iter().all(|k| {
        v.get(*k)
            .is_some_and(|s| loom_protocol::validate_live_identifier(s, k).is_ok())
    }) || !v.get("revision").is_some_and(|s| unsigned(s))
    {
        return false;
    }
    let png = match v.get("format").map(String::as_str) {
        Some("png") => true,
        Some("raw_bgra") => false,
        _ => return false,
    };
    for (key, max) in [
        ("maxWidth", 1280),
        ("maxHeight", 720),
        ("maxFps", if png { 10 } else { 60 }),
    ] {
        if let Some(s) = v.get(key) {
            if (!png && key != "maxFps")
                || !unsigned(s)
                || !s.parse::<u64>().is_ok_and(|n| n > 0 && n <= max)
            {
                return false;
            }
        }
    }
    true
}
