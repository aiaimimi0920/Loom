//! 对接当前 Workers API；不复用历史 Art-store 协议或自动执行远端载荷。
mod aria2;
mod aria2_native;
mod download;
mod download_control;
pub(crate) fn shutdown_download_engine() {
    aria2_native::shutdown();
}
pub(crate) fn cancel_account_downloads(root: &Path) {
    download_control::cancel_root(root);
}
pub(crate) mod application;
mod aria2_relay;
mod install;
use crate::naccount::{access_token, error, Result};
#[cfg(test)]
pub(crate) use install::prepare_trial_install;
pub(crate) use install::{prepare_install, InstallPackage};
use loom_security::network::{secure_client, OutboundPolicy};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, path::Path, time::Duration};

const ORIGIN: &str = "https://assetlibrary.aiaimimi.com";
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    #[serde(default)]
    operation_id: String,
    #[serde(default)]
    after: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    resource_id: String,
}
fn route(
    action: &str,
    input: &Input,
) -> Result<(reqwest::Method, reqwest::Url, Option<Value>, bool)> {
    let (method, path, body, private) = match action {
        "catalog" => (reqwest::Method::GET, "/v1/catalog", None, false),
        "resources" => (reqwest::Method::GET, "/v1/me/resources", None, true),
        "library" => (reqwest::Method::GET, "/v1/me/library", None, true),
        "create" => {
            if uuid::Uuid::parse_str(&input.operation_id).is_err() {
                return Err(error(400, "asset_library_operation_required"));
            }
            if !matches!(
                input.kind.as_str(),
                "art" | "capability" | "application" | "mcp"
            ) || input.title.trim().is_empty()
                || input.title.chars().count() > 200
                || input.title.chars().any(char::is_control)
            {
                return Err(error(400, "asset_library_resource_invalid"));
            }
            (
                reqwest::Method::POST,
                "/v1/resources",
                Some(json!({"kind":input.kind, "title":input.title.trim()})),
                true,
            )
        }
        _ => return Err(error(404, "asset_library_route_unknown")),
    };
    let mut url = reqwest::Url::parse(&format!("{ORIGIN}{path}")).expect("fixed service");
    if !input.resource_id.is_empty() {
        let id = uuid::Uuid::parse_str(&input.resource_id)
            .map_err(|_| error(400, "asset_library_resource_invalid"))?;
        if !matches!(action, "catalog" | "library")
            || id.get_version_num() != 4
            || id.to_string() != input.resource_id
        {
            return Err(error(400, "asset_library_resource_invalid"));
        }
        url.query_pairs_mut()
            .append_pair("resourceId", &input.resource_id);
    }
    if method == reqwest::Method::GET {
        url.query_pairs_mut().append_pair("limit", "20");
        if !input.after.is_empty() {
            if input.after.len() > 512 {
                return Err(error(400, "asset_library_cursor_invalid"));
            }
            url.query_pairs_mut().append_pair("after", &input.after);
        }
    }
    Ok((method, url, body, private))
}
pub(crate) fn handle(root: &Path, action: &str, body: &str) -> Result<Value> {
    if body.len() > 4096 {
        return Err(error(413, "asset_library_request_budget"));
    }
    if action == "download" {
        return download::handle(root, body);
    }
    if action == "download-control" {
        return download_control::handle(root, body);
    }
    let input: Input =
        serde_json::from_str(body).map_err(|_| error(400, "asset_library_request_invalid"))?;
    let (method, url, body, private) = route(action, &input)?;
    let client = client()?;
    let mut request = client.request(method, url);
    if private {
        request = request.bearer_auth(access_token(root)?);
    }
    if let Some(body) = body {
        request = request
            .json(&body)
            .header("Idempotency-Key", &input.operation_id);
    }
    let value = json_response(request)?;
    if action != "create" && !value["items"].is_array() {
        return Err(error(502, "asset_library_response_invalid"));
    }
    Ok(value)
}
fn client() -> Result<reqwest::blocking::Client> {
    secure_client(
        "Loom-AssetLibrary/1",
        Duration::from_secs(15),
        OutboundPolicy {
            allow_http_loopback: false,
            allow_private_networks: false,
            allowed_domains: vec![],
            max_redirects: 0,
        },
    )
    .map_err(|_| error(503, "asset_library_unavailable"))
}
fn json_response(request: reqwest::blocking::RequestBuilder) -> Result<Value> {
    let response = request
        .send()
        .map_err(|_| error(503, "asset_library_unavailable"))?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    response
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| error(503, "asset_library_unavailable"))?;
    if bytes.len() > 262144 {
        return Err(error(502, "asset_library_response_invalid"));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| error(502, "asset_library_response_invalid"))?;
    if !(200..300).contains(&status) {
        let code = match value["error"].as_str() {
            Some("SCANNER_CLOUD_NOT_VALIDATED") => "asset_library_distribution_unavailable",
            Some("NATIVE_CLIENT_NOT_CONFIGURED") => "asset_library_native_client_not_configured",
            _ if status == 401 => "naccount_login_required",
            _ => "asset_library_request_rejected",
        };
        return Err(error(status, code));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_library_uses_current_contract_and_preserves_cursor() {
        let input = Input {
            after: "opaque+cursor".into(),
            ..Default::default()
        };
        let (_, url, _, private) = route("resources", &input).unwrap();
        assert_eq!(url.path(), "/v1/me/resources");
        assert!(private);
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "after" && v == "opaque+cursor"));
        for kind in ["art", "capability", "application", "mcp"] {
            assert!(route(
                "create",
                &Input {
                    operation_id: uuid::Uuid::new_v4().to_string(),
                    kind: kind.into(),
                    title: "测试资源".into(),
                    ..Default::default()
                }
            )
            .is_ok());
        }
        assert!(route("create", &Input::default()).is_err());
        assert!(route("install", &Input::default()).is_err());
    }
    #[test]
    fn asset_library_update_filter_is_bound_to_resource_identity() {
        let mut input = Input {
            resource_id: uuid::Uuid::new_v4().to_string(),
            ..Default::default()
        };
        for action in ["catalog", "library"] {
            let (_, url, _, _) = route(action, &input).unwrap();
            assert!(url
                .query_pairs()
                .any(|(key, value)| key == "resourceId" && value == input.resource_id));
        }
        assert!(route("resources", &input).is_err());
        input.resource_id = "not-an-id".into();
        assert!(route("library", &input).is_err());
    }
}
