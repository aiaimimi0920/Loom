//! Selects the bounded translation backend without exposing provider details to plugins.

use std::env;
use std::time::Duration;

use loom_gateway::{GatewayChatMessage, GatewayChatRequest, GatewayClient, GatewayClientConfig};
use serde_json::{json, Value};

const MODE_ENV: &str = "LOOM_TRANSLATION_MODE";
const LOCAL_ORIGIN_ENV: &str = "LOOM_LOCAL_TRANSLATION_BASE_URL";
const LOCAL_MODEL_ENV: &str = "LOOM_LOCAL_TRANSLATION_MODEL";
const LOCAL_TOKEN_ENV: &str = "LOOM_LOCAL_TRANSLATION_TOKEN";
const DEFAULT_LOCAL_MODEL: &str = "translation-local";
const LOCAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(35);
const LOCAL_ONLY_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderMode {
    Local,
    Gateway,
    Auto,
}

impl ProviderMode {
    fn from_environment() -> Result<Self, String> {
        let configured = env::var(MODE_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let has_local_origin = env::var(LOCAL_ORIGIN_ENV)
            .ok()
            .is_some_and(|value| !value.trim().is_empty());
        Self::parse(configured.as_deref(), has_local_origin)
    }

    fn parse(configured: Option<&str>, has_local_origin: bool) -> Result<Self, String> {
        match configured.as_deref() {
            Some("local") => Ok(Self::Local),
            Some("gateway") => Ok(Self::Gateway),
            Some("auto") | Some("local-first") => Ok(Self::Auto),
            Some(_) => Err("LOOM_TRANSLATION_MODE must be local, gateway, or auto".to_owned()),
            None if has_local_origin => Ok(Self::Auto),
            None => Ok(Self::Gateway),
        }
    }
}

pub(super) fn complete(
    system: String,
    user: String,
    response_schema: Option<Value>,
    requested_mode: Option<String>,
) -> Result<String, String> {
    let mode = from_request_or_environment(requested_mode.as_deref())?;
    if mode == ProviderMode::Auto
        && env::var(LOCAL_ORIGIN_ENV)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    {
        validate_local_configuration()?;
    }
    let local_system = system.clone();
    let local_user = user.clone();
    let local_timeout = if mode == ProviderMode::Local {
        LOCAL_ONLY_REQUEST_TIMEOUT
    } else {
        LOCAL_REQUEST_TIMEOUT
    };
    let gateway_schema = response_schema.clone();
    complete_with(
        mode,
        || {
            complete_local(
                &local_system,
                &local_user,
                response_schema.as_ref(),
                local_timeout,
            )
        },
        || super::model_gateway::complete(system, user, gateway_schema),
    )
}

fn from_request_or_environment(requested_mode: Option<&str>) -> Result<ProviderMode, String> {
    from_request_with_default(requested_mode, ProviderMode::from_environment)
}

fn from_request_with_default(
    requested_mode: Option<&str>,
    default: impl FnOnce() -> Result<ProviderMode, String>,
) -> Result<ProviderMode, String> {
    if let Some(value) = requested_mode {
        let value = value.trim().to_ascii_lowercase();
        if value != "auto" {
            return ProviderMode::parse(Some(&value), false);
        }
    }
    // Hook's default "auto" must honor an operator's strict local-only configuration.
    default()
}

fn complete_with<Local, Gateway>(
    mode: ProviderMode,
    local: Local,
    gateway: Gateway,
) -> Result<String, String>
where
    Local: FnOnce() -> Result<String, String>,
    Gateway: FnOnce() -> Result<String, String>,
{
    match mode {
        ProviderMode::Local => local(),
        ProviderMode::Gateway => gateway(),
        ProviderMode::Auto => local().or_else(|_| gateway()),
    }
}

fn complete_local(
    system: &str,
    user: &str,
    schema: Option<&Value>,
    timeout: Duration,
) -> Result<String, String> {
    let origin = env::var(LOCAL_ORIGIN_ENV)
        .map_err(|_| "Local translation provider is not configured".to_owned())?;
    let model = env::var(LOCAL_MODEL_ENV).unwrap_or_else(|_| DEFAULT_LOCAL_MODEL.to_owned());
    validate_local_configuration_values(&origin, &model)?;
    let mut config = GatewayClientConfig::new_loopback(origin)
        .map_err(|_| "Local translation base URL must be a loopback origin".to_owned())?
        .without_proxy()
        .with_timeout(timeout);
    if let Ok(token) = env::var(LOCAL_TOKEN_ENV) {
        if token.len() > 4096 {
            return Err("Local translation token is too large".to_owned());
        }
        if !token.trim().is_empty() {
            config = config.with_auth_token(token);
        }
    }
    complete_local_with_config(config, &model, system, user, schema)
}

fn complete_local_with_config(
    config: GatewayClientConfig,
    model: &str,
    system: &str,
    user: &str,
    schema: Option<&Value>,
) -> Result<String, String> {
    let client = GatewayClient::new(config)
        .map_err(|_| "Local translation provider configuration is invalid".to_owned())?;
    client
        .chat(GatewayChatRequest {
            model: model.to_owned(),
            messages: vec![
                GatewayChatMessage::system(system),
                GatewayChatMessage::user(user),
            ],
            stream: false,
            temperature: Some(serde_json::Number::from_f64(0.2).expect("finite temperature")),
            response_format: schema.map(|schema| {
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": "response", "strict": true, "schema": schema }
                })
            }),
        })
        .map(|response| response.content)
        .map_err(|_| "Local translation provider request failed".to_owned())
}

fn validate_local_configuration() -> Result<(), String> {
    let origin = env::var(LOCAL_ORIGIN_ENV)
        .map_err(|_| "Local translation provider is not configured".to_owned())?;
    let model = env::var(LOCAL_MODEL_ENV).unwrap_or_else(|_| DEFAULT_LOCAL_MODEL.to_owned());
    validate_local_configuration_values(&origin, &model)
}

fn validate_local_configuration_values(origin: &str, model: &str) -> Result<(), String> {
    if model.trim().is_empty() || model.len() > 256 {
        return Err("Local translation model name is invalid".to_owned());
    }
    GatewayClientConfig::new_loopback(origin)
        .map_err(|_| "Local translation base URL must be a loopback origin".to_owned())?;
    if env::var(LOCAL_TOKEN_ENV)
        .ok()
        .is_some_and(|token| token.len() > 4096)
    {
        return Err("Local translation token is too large".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn local_mode_never_calls_gateway() {
        let gateway_calls = Cell::new(0);
        let result = complete_with(
            ProviderMode::Local,
            || Ok("local".to_owned()),
            || {
                gateway_calls.set(gateway_calls.get() + 1);
                Ok("gateway".to_owned())
            },
        )
        .expect("local result");
        assert_eq!(result, "local");
        assert_eq!(gateway_calls.get(), 0);
    }

    #[test]
    fn mode_parser_defaults_to_gateway_and_rejects_unknown_values() {
        assert_eq!(
            ProviderMode::parse(None, false).unwrap(),
            ProviderMode::Gateway
        );
        assert_eq!(ProviderMode::parse(None, true).unwrap(), ProviderMode::Auto);
        assert_eq!(
            ProviderMode::parse(Some("local-first"), false).unwrap(),
            ProviderMode::Auto
        );
        assert!(ProviderMode::parse(Some("remote"), false).is_err());
    }

    #[test]
    fn request_mode_overrides_environment_selection() {
        assert_eq!(
            from_request_or_environment(Some("local")).unwrap(),
            ProviderMode::Local
        );
        assert_eq!(
            from_request_or_environment(Some("gateway")).unwrap(),
            ProviderMode::Gateway
        );
        assert!(from_request_or_environment(Some("unknown")).is_err());
    }

    #[test]
    fn automatic_request_preserves_operator_local_only_mode() {
        let mode = from_request_with_default(Some("auto"), || Ok(ProviderMode::Local)).unwrap();
        assert_eq!(mode, ProviderMode::Local);
        let result = complete_with(
            mode,
            || Err("local failed".to_owned()),
            || panic!("offline OCR leaked to Gateway"),
        );
        assert_eq!(result.unwrap_err(), "local failed");
        assert_eq!(
            from_request_with_default(Some("gateway"), || panic!(
                "explicit mode must not consult defaults"
            ))
            .unwrap(),
            ProviderMode::Gateway
        );
    }

    #[test]
    fn auto_falls_back_once_when_local_fails() {
        let local_calls = Cell::new(0);
        let gateway_calls = Cell::new(0);
        let result = complete_with(
            ProviderMode::Auto,
            || {
                local_calls.set(local_calls.get() + 1);
                Err("local unavailable".to_owned())
            },
            || {
                gateway_calls.set(gateway_calls.get() + 1);
                Ok("gateway".to_owned())
            },
        )
        .expect("Gateway fallback");
        assert_eq!(result, "gateway");
        assert_eq!(local_calls.get(), 1);
        assert_eq!(gateway_calls.get(), 1);
    }

    #[test]
    fn local_origin_requires_loopback_ip_and_no_path() {
        assert!(GatewayClientConfig::new_loopback("http://127.0.0.1:11434").is_ok());
        assert!(GatewayClientConfig::new_loopback("http://[::1]:11434").is_ok());
        assert!(GatewayClientConfig::new_loopback("https://10.0.0.2:11434").is_err());
        assert!(GatewayClientConfig::new_loopback("http://127.0.0.1:11434/api").is_err());
        assert!(GatewayClientConfig::new_loopback("http://user:pass@127.0.0.1:11434").is_err());
    }

    #[test]
    fn local_provider_reads_openai_compatible_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local provider");
        let address = listener.local_addr().expect("local provider address");
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept local request");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            let body_end;
            loop {
                let bytes = socket.read(&mut chunk).expect("read local request");
                assert!(bytes > 0, "local request ended before body");
                request.extend_from_slice(&chunk[..bytes]);
                let Some(header_end) = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|index| index + 4)
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().expect("content length"))
                    })
                    .expect("content-length header");
                if request.len() >= header_end + content_length {
                    body_end = header_end + content_length;
                    break;
                }
            }
            let request = String::from_utf8(request[..body_end].to_vec()).expect("UTF-8 request");
            assert!(request.contains("\"temperature\":0.2"));
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(
                body["response_format"]["json_schema"]["schema"],
                json!({ "type": "string" })
            );
            let body = r#"{"choices":[{"message":{"content":"{\"translations\":[\"你好\"]}"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket
                .write_all(response.as_bytes())
                .expect("write local response");
        });
        let config = GatewayClientConfig::new_loopback(format!("http://{address}"))
            .expect("loopback config")
            .without_proxy();
        let result = complete_local_with_config(
            config,
            "local-model",
            "system",
            "user",
            Some(&json!({ "type": "string" })),
        )
        .expect("local response");
        assert_eq!(result, r#"{"translations":["你好"]}"#);
        server.join().expect("local provider server");
    }
}
