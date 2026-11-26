//! Surface LAN seam required by the native Live viewer host; exposes no instance management, arbitrary events, or admin reads.
use super::lan_http::Request;
use anyhow::{ensure, Result};

pub(super) fn allowed(method: &str, path: &str) -> bool {
    if method == "POST" && path == "/v1/surfaces/attach" {
        return true;
    }
    if method == "POST" {
        let Some(id) = path
            .strip_prefix("/v1/surfaces/instances/")
            .and_then(|p| p.strip_suffix("/lifecycle"))
        else {
            return false;
        };
        // Runtime owner consumes the raw segment; never let reqwest normalize dot segments, escapes, or path aliases.
        return !matches!(id, "" | "." | "..")
            && id
                .bytes()
                .all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b'?' | b'#' | b'%'));
    }
    if method != "GET" {
        return false;
    }
    if path == "/v1/surfaces/stream" {
        return true;
    }
    let Some(query) = path.strip_prefix("/v1/surfaces/stream?") else {
        return false;
    };
    let mut after = false;
    let mut timeout = false;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            return false;
        };
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        match key {
            "after" if !after && value.parse::<usize>().is_ok() => after = true,
            "timeoutMs" if !timeout && value.parse::<u64>().is_ok_and(|n| n <= 5000) => {
                timeout = true
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn matches(path: &str) -> bool {
    allowed("GET", path) || allowed("POST", path)
}

pub(super) fn validate(request: &Request) -> Result<()> {
    ensure!(
        allowed(&request.method, &request.path),
        "Invalid LAN Surface route"
    );
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
    // Identity, nonce replay, device revocation, Surface attachment ownership, and lifecycle versions remain verified by the runtime owner.
    Ok(())
}
