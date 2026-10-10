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
pub(super) async fn hook_bridge_websocket_protocols(
    endpoint: String,
) -> Result<Vec<String>, String> {
    run_blocking_command(move || {
        validate_hook_auth_endpoint(&endpoint)?;
        let token =
            daemon_auth_token()?.ok_or_else(|| "Loom authentication is unavailable".to_owned())?;
        let encoded = base64_encode(token.as_bytes())
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_");
        Ok(vec![
            "loom.hook.v1".to_owned(),
            format!("loom.auth.{encoded}"),
        ])
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
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
