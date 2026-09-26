//! Opens only the pending account authorization owned by the local daemon.
use super::*;

#[tauri::command]
pub(super) async fn open_loom_account_login(
    base_url: String,
    request_id: String,
) -> Result<(), String> {
    run_blocking_command(move || {
        let base_url = resolve_command_base_url(base_url);
        let view = http_post_json(&base_url, "/v1/account/status", &serde_json::json!({}))?;
        let url = authorization_url(&view, &request_id)?;
        diagnostics::open_url_in_default_browser(url.as_str())
    })
    .await
}

fn authorization_url(view: &Value, request_id: &str) -> Result<tauri::Url, String> {
    let invalid = || "登录请求已失效或地址无效，请重新登录。".to_owned();
    if request_id.len() != 64
        || !request_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || view["status"] != "pending"
        || view["requestId"] != request_id
    {
        return Err(invalid());
    }
    let address = view["authorizationUrl"].as_str().ok_or_else(invalid)?;
    if address.len() > 4096 {
        return Err(invalid());
    }
    let url = tauri::Url::parse(address).map_err(|_| invalid())?;
    let origin = view["origin"].as_str().ok_or_else(invalid)?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    let pairs = url.query_pairs().collect::<Vec<_>>();
    let allowed = [
        "requestId",
        "codeChallenge",
        "publicKey",
        "deviceName",
        "expiresAtMs",
    ];
    if url.origin().ascii_serialization() != origin
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/loom/authorize"
        || pairs.len() != allowed.len()
        || allowed
            .iter()
            .any(|key| pairs.iter().filter(|(name, _)| name == key).count() != 1)
        || !pairs
            .iter()
            .any(|(key, value)| key == "requestId" && value == request_id)
    {
        return Err(invalid());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pending(origin: &str) -> Value {
        let id = "a".repeat(64);
        json!({"status":"pending", "requestId":id, "origin":origin,
            "authorizationUrl":format!("{origin}/loom/authorize?requestId={id}&codeChallenge=c&publicKey=k&deviceName=d&expiresAtMs=1")})
    }

    #[test]
    fn account_login_browser_accepts_only_current_daemon_authorization() {
        let id = "a".repeat(64);
        for origin in [
            "https://platform.example",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
        ] {
            assert!(authorization_url(&pending(origin), &id).is_ok());
        }
        let valid = pending("https://platform.example");
        assert!(authorization_url(&valid, &"b".repeat(64)).is_err());
        for replacement in [
            "https://other.example/loom/authorize",
            "https://platform.example/elsewhere",
            "https://user@platform.example/loom/authorize",
            "javascript:alert(1)",
        ] {
            let mut bad = valid.clone();
            bad["authorizationUrl"] = json!(valid["authorizationUrl"]
                .as_str()
                .unwrap()
                .replace("https://platform.example/loom/authorize", replacement));
            assert!(authorization_url(&bad, &id).is_err());
        }
        for suffix in [
            "#fragment",
            "&requestId=other",
            "&redirect=https://other.example",
        ] {
            let mut bad = valid.clone();
            bad["authorizationUrl"] = json!(format!(
                "{}{suffix}",
                valid["authorizationUrl"].as_str().unwrap()
            ));
            assert!(authorization_url(&bad, &id).is_err());
        }
        assert!(authorization_url(&pending("http://platform.example"), &id).is_err());
        let mut completed = valid;
        completed["status"] = json!("signed_in");
        assert!(authorization_url(&completed, &id).is_err());
    }
}
