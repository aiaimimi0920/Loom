//! Local WebSocket credentials cross only the trusted desktop command boundary.
use super::*;

fn validate_hook_auth_endpoint(endpoint: &str) -> Result<(), String> {
    let url = tauri::Url::parse(endpoint).map_err(|_| "Invalid Hook endpoint".to_owned())?;
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() != "ws"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Hook authentication requires a credential-free loopback endpoint".to_owned());
    }
    Ok(())
}

#[tauri::command]
pub(super) async fn hook_bridge_websocket_protocols(endpoint: String) -> Result<Value, String> {
    run_blocking_command(move || {
        validate_hook_auth_endpoint(&endpoint)?;
        let credential = http_post_json(
            &configured_loom_daemon_url(),
            "/v1/hook-bridge/credentials",
            &serde_json::json!({}),
        )?;
        scoped_hook_protocols(&endpoint, &credential)
    })
    .await
}

fn scoped_hook_protocols(endpoint: &str, credential: &Value) -> Result<Value, String> {
    validate_hook_auth_endpoint(endpoint)?;
    let bound_url = credential
        .get("url")
        .and_then(Value::as_str)
        .ok_or("Hook bridge is unavailable")?;
    validate_hook_auth_endpoint(bound_url)?;
    let bound = tauri::Url::parse(bound_url).map_err(|_| "Invalid bound Hook endpoint")?;
    let requested = tauri::Url::parse(endpoint).map_err(|_| "Invalid Hook endpoint")?;
    if bound.host_str() != Some("127.0.0.1")
        || bound.port_or_known_default() != requested.port_or_known_default()
    {
        return Err("Hook endpoint does not match the daemon-owned listener".to_owned());
    }
    let token = credential
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| token.starts_with("hook-v1.") && token.len() <= 4096)
        .ok_or("Hook bridge credential is unavailable")?;
    let encoded = base64_encode(token.as_bytes())
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_");
    Ok(
        serde_json::json!({"url": bound_url, "protocols": ["loom.hook.v1", format!("loom.auth.{encoded}")]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_hook_credentials_require_the_broker_owned_listener() {
        let endpoint = "ws://[::1]:19820";
        let credential =
            serde_json::json!({"url":"ws://127.0.0.1:19820", "token":"hook-v1.fixture"});
        let connection = scoped_hook_protocols(endpoint, &credential).unwrap();
        assert_eq!(connection["url"], "ws://127.0.0.1:19820");
        assert_eq!(connection["protocols"][0], "loom.hook.v1");
        for url in [
            "ws://127.0.0.1:19821",
            "ws://[::1]:19820",
            "ws://localhost:19820",
            "ws://evil.example:19820",
        ] {
            assert!(scoped_hook_protocols(
                endpoint,
                &serde_json::json!({"url":url,"token":"hook-v1.fixture"})
            )
            .is_err());
        }
        assert!(scoped_hook_protocols(
            endpoint,
            &serde_json::json!({"url":"ws://127.0.0.1:19820","token":"admin"})
        )
        .is_err());
        assert!(scoped_hook_protocols(endpoint, &serde_json::json!({})).is_err());
    }
    #[test]
    fn hook_credentials_are_limited_to_local_origin_endpoints() {
        for url in [
            "ws://127.0.0.1:19820",
            "ws://localhost:19820",
            "ws://[::1]:19820",
        ] {
            assert!(validate_hook_auth_endpoint(url).is_ok());
        }
        for url in [
            "ws://evil.example",
            "ws://user@127.0.0.1",
            "ws://127.0.0.1/?token=x",
            "ws://localhost/path",
        ] {
            assert!(validate_hook_auth_endpoint(url).is_err());
        }
    }
}
