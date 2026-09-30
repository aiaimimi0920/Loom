use super::{error, Result};
use loom_security::network::{secure_client, validate_outbound_url, OutboundPolicy};
use serde_json::Value;
use std::{io::Read, time::Duration};

pub(super) fn origin(input: &str) -> Result<String> {
    let url =
        reqwest::Url::parse(input.trim()).map_err(|_| error(400, "account_origin_invalid"))?;
    if input.len() > 256
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.scheme(), "http" | "https")
        || (url.scheme() == "http"
            && !url
                .host_str()
                .is_some_and(loom_security::network::host_is_loopback_literal))
    {
        return Err(error(400, "account_origin_invalid"));
    }
    Ok(url.origin().ascii_serialization())
}

pub(super) fn post(base: &str, action: &str, body: Value) -> Result<Value> {
    let base = origin(base)?;
    let url = reqwest::Url::parse(&format!("{base}/api/loom/account/{action}"))
        .map_err(|_| error(400, "account_origin_invalid"))?;
    let policy = OutboundPolicy {
        allow_http_loopback: true,
        allow_private_networks: true,
        allowed_domains: vec![],
        max_redirects: 0,
    };
    validate_outbound_url(&url, &policy).map_err(|_| error(400, "account_origin_invalid"))?;
    let client = secure_client("Loom-Account/1", Duration::from_secs(8), policy)
        .map_err(|_| error(503, "account_network_unavailable"))?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|_| error(503, "account_network_unavailable"))?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|_| error(503, "account_network_unavailable"))?;
    if bytes.len() > 16_384 {
        return Err(error(502, "account_response_invalid"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| error(502, "account_response_invalid"))?;
    if !(200..300).contains(&status) {
        return Err(match status {
            401 if value["error"]["code"] == "device_session_unavailable" => {
                error(401, "account_session_unavailable")
            }
            401 if value["error"]["code"] == "device_clock_skew" => {
                error(409, "account_clock_skew")
            }
            429 => error(429, "account_rate_limited"),
            _ => error(502, "account_server_rejected"),
        });
    }
    Ok(value)
}
