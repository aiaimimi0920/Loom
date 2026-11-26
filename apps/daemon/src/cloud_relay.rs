//! Reserved Cloud Relay configuration and readiness; no cloud transport is implemented.
//! Send/inbox fail explicitly rather than claiming an in-process queue is remote delivery.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{invalid_request, structured_error, ParsedHttpRequest};

const CLOUD_RELAY_FILE_REL: &str = "settings/cloud-relay.json";
const MAX_ENVELOPE_PAYLOAD_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudRelayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub expose_mcp: bool,
    #[serde(default = "default_expose_projection")]
    pub expose_projection_relay: bool,
}

fn default_expose_projection() -> bool {
    true
}

fn config_path(control_plane_root: &Path) -> PathBuf {
    control_plane_root.join(CLOUD_RELAY_FILE_REL)
}

pub fn load_config(control_plane_root: &Path) -> CloudRelayConfig {
    fs::read(config_path(control_plane_root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_config(control_plane_root: &Path, config: &CloudRelayConfig) -> Result<()> {
    let path = config_path(control_plane_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut encoded = serde_json::to_vec_pretty(config)?;
    encoded.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;
    file.write_all(&encoded)?;
    file.sync_all()?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn validate_relay_url(raw: &str) -> std::result::Result<String, String> {
    let parsed = reqwest::Url::parse(raw.trim()).map_err(|e| format!("invalid relayUrl: {e}"))?;
    let host = parsed.host_str().unwrap_or_default();
    let is_loopback =
        host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1";
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && is_loopback) {
        return Err("Cloud Relay URL must use https:// (or loopback http:// for tests)".to_owned());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Cloud Relay URL must not embed credentials".to_owned());
    }
    Ok(parsed.to_string().trim_end_matches('/').to_owned())
}

fn resolve_signed_in_account_id(control_plane_root: &Path) -> Option<String> {
    if let Ok(Some(user)) = crate::naccount::user_summary(control_plane_root) {
        return Some(user.account_id);
    }
    #[cfg(test)]
    if let Ok(bytes) = fs::read(control_plane_root.join("settings/test-naccount-session.json")) {
        if let Ok(val) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(id) = val.get("accountId").and_then(Value::as_str) {
                return Some(id.to_owned());
            }
        }
    }
    None
}

pub fn status_value(control_plane_root: &Path) -> Value {
    let config = load_config(control_plane_root);
    let account_id = resolve_signed_in_account_id(control_plane_root);
    let signed_in = account_id.is_some();
    json!({
        "enabled": config.enabled,
        "signedIn": signed_in,
        "accountId": account_id,
        "relayUrl": config.relay_url,
        "exposeMcp": config.expose_mcp,
        "exposeProjectionRelay": config.expose_projection_relay,
        "active": false,
        "transportAvailable": false,
        "projectionTopology": "otter_to_loom1_to_cloud_relay_to_loom2_to_otter",
        "queuedEnvelopes": 0,
    })
}

pub fn route(
    request: &ParsedHttpRequest,
    route_path: &str,
    authenticated_device_id: Option<&str>,
    admin_authenticated: bool,
    control_plane_root: &Path,
) -> Option<Result<(u16, String)>> {
    if !route_path.starts_with("/v1/cloud-relay/") {
        return None;
    }
    match (request.method.as_str(), route_path) {
        ("GET", "/v1/cloud-relay/status") => {
            if !admin_authenticated && authenticated_device_id.is_none() {
                return Some(structured_error(401, json!({ "code": "unauthorized" })));
            }
            Some(
                serde_json::to_string(&status_value(control_plane_root))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("PUT", "/v1/cloud-relay/config") => {
            if !admin_authenticated {
                return Some(structured_error(
                    403,
                    json!({ "code": "cloud_relay_admin_required" }),
                ));
            }
            let mut input = match serde_json::from_str::<CloudRelayConfig>(&request.body) {
                Ok(cfg) => cfg,
                Err(error) => return Some(invalid_request(error.to_string())),
            };
            if let Some(raw_url) = input.relay_url.as_deref().filter(|s| !s.trim().is_empty()) {
                match validate_relay_url(raw_url) {
                    Ok(normalized) => input.relay_url = Some(normalized),
                    Err(msg) => return Some(invalid_request(msg)),
                }
            } else {
                input.relay_url = None;
            }
            if let Err(error) = save_config(control_plane_root, &input) {
                return Some(Err(error));
            }
            Some(
                serde_json::to_string(&status_value(control_plane_root))
                    .map(|s| (200, s))
                    .map_err(Into::into),
            )
        }
        ("POST", "/v1/cloud-relay/projection/send") => {
            if !admin_authenticated && authenticated_device_id.is_none() {
                return Some(structured_error(401, json!({ "code": "unauthorized" })));
            }
            if request.body.len() > MAX_ENVELOPE_PAYLOAD_BYTES {
                return Some(structured_error(
                    413,
                    json!({ "code": "cloud_relay_payload_too_large" }),
                ));
            }
            let cfg = load_config(control_plane_root);
            if !cfg.enabled || !cfg.expose_projection_relay {
                return Some(structured_error(
                    409,
                    json!({
                        "code": "cloud_relay_disabled",
                        "message": "Cloud Relay projection forwarding is disabled"
                    }),
                ));
            }
            if resolve_signed_in_account_id(control_plane_root).is_none() {
                return Some(structured_error(
                    401,
                    json!({
                        "code": "cloud_relay_naccount_required",
                        "message": "3-tier Internet Loom relay requires signed-in NAccount state"
                    }),
                ));
            }
            // A local queue is not delivery to another Loom. Keep configuration but
            // fail explicitly until an authenticated remote transport is implemented.
            Some(transport_unavailable())
        }
        ("GET", "/v1/cloud-relay/projection/inbox") => {
            if !admin_authenticated && authenticated_device_id.is_none() {
                return Some(structured_error(401, json!({ "code": "unauthorized" })));
            }
            Some(transport_unavailable())
        }
        _ => None,
    }
}

fn transport_unavailable() -> Result<(u16, String)> {
    structured_error(
        501,
        json!({
            "code": "cloud_relay_transport_unavailable",
            "message": "Cross-Loom cloud delivery is not implemented; no payload was sent or queued",
            "relayed": false,
        }),
    )
}
