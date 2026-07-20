//! Client contracts for the external Neuro Gateway.

use std::io::Read;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Version of the Gateway integration crate.
pub const LOOM_GATEWAY_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_GATEWAY_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_GATEWAY_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_ERROR_MESSAGE_CHARS: usize = 512;

/// Gateway client errors.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid Gateway base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("Gateway URL scheme must be http or https")]
    UnsupportedScheme,
    #[error("Gateway URL must not contain credentials")]
    CredentialsInUrl,
    #[error("Gateway HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Gateway I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Gateway JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Gateway returned HTTP status {status}: {message}")]
    HttpStatus {
        status: u16,
        code: Option<String>,
        message: String,
    },
    #[error("Gateway response exceeded {0} bytes")]
    ResponseTooLarge(usize),
    #[error("Gateway response is malformed: {0}")]
    MalformedResponse(String),
}

/// Result alias for Gateway client operations.
pub type GatewayResult<T> = Result<T, GatewayError>;

/// Configuration for a local or hosted Gateway HTTP client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayClientConfig {
    base_url: String,
    auth_token: Option<String>,
    timeout: Duration,
}

impl GatewayClientConfig {
    /// Create a client configuration with the bounded default timeout.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: None,
            timeout: DEFAULT_GATEWAY_TIMEOUT,
        }
    }

    /// Attach an optional serving API bearer token.
    #[must_use]
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        let token = token.into();
        self.auth_token = (!token.trim().is_empty()).then_some(token);
        self
    }

    /// Override the request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// OpenAI-compatible chat message forwarded to Gateway.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayChatMessage {
    pub role: String,
    pub content: String,
}

impl GatewayChatMessage {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: content.into(),
        }
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: content.into(),
        }
    }
}

/// Non-streaming OpenAI-compatible chat request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayChatRequest {
    pub model: String,
    pub messages: Vec<GatewayChatMessage>,
    pub stream: bool,
}

/// Normalized first assistant message returned by Gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayChatResponse {
    pub model: String,
    pub content: String,
    pub request_id: Option<String>,
}

/// Minimal HTTP client for the external Gateway.
#[derive(Clone, Debug)]
pub struct GatewayClient {
    http: Client,
    chat_url: Url,
    auth_token: Option<String>,
}

impl GatewayClient {
    /// Build a validated client from an origin URL.
    pub fn new(config: GatewayClientConfig) -> GatewayResult<Self> {
        let mut base_url = Url::parse(config.base_url.trim())
            .map_err(|error| GatewayError::InvalidBaseUrl(error.to_string()))?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(GatewayError::UnsupportedScheme);
        }
        if !base_url.username().is_empty() || base_url.password().is_some() {
            return Err(GatewayError::CredentialsInUrl);
        }
        if base_url.path() != "" && base_url.path() != "/" {
            return Err(GatewayError::InvalidBaseUrl(
                "base URL must be an origin without a path".to_owned(),
            ));
        }
        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err(GatewayError::InvalidBaseUrl(
                "base URL must not contain a query or fragment".to_owned(),
            ));
        }

        base_url.set_path("/v1/chat/completions");
        let http = Client::builder().timeout(config.timeout).build()?;
        Ok(Self {
            http,
            chat_url: base_url,
            auth_token: config.auth_token,
        })
    }

    /// Send one bounded, non-streaming chat request.
    pub fn chat(&self, request: GatewayChatRequest) -> GatewayResult<GatewayChatResponse> {
        let mut builder = self.http.post(self.chat_url.clone()).json(&request);
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }

        let mut response = builder.send()?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        response
            .by_ref()
            .take((MAX_GATEWAY_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut body)?;
        if body.len() > MAX_GATEWAY_RESPONSE_BYTES {
            return Err(GatewayError::ResponseTooLarge(body.len()));
        }
        if !status.is_success() {
            return Err(parse_gateway_http_error(status.as_u16(), &body));
        }

        parse_chat_response(&body, &request.model, request_id)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    model: Option<String>,
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

fn parse_chat_response(
    body: &[u8],
    requested_model: &str,
    request_id: Option<String>,
) -> GatewayResult<GatewayChatResponse> {
    let parsed: OpenAiChatResponse = serde_json::from_slice(body)
        .map_err(|error| GatewayError::MalformedResponse(error.to_string()))?;
    let choice = parsed
        .choices
        .first()
        .ok_or_else(|| GatewayError::MalformedResponse("choices is empty".to_owned()))?;
    if choice.message.content.trim().is_empty() {
        return Err(GatewayError::MalformedResponse(
            "assistant content is empty".to_owned(),
        ));
    }

    Ok(GatewayChatResponse {
        model: parsed
            .model
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| requested_model.to_owned()),
        content: choice.message.content.clone(),
        request_id,
    })
}

fn parse_gateway_http_error(status: u16, body: &[u8]) -> GatewayError {
    let parsed = serde_json::from_slice::<Value>(body).ok();
    let error = parsed.as_ref().and_then(|value| value.get("error"));
    let code = error
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let message = error
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| parsed.as_ref().and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_owned());
    let message = truncate_message(if message.is_empty() {
        "Gateway returned an error"
    } else {
        &message
    });

    GatewayError::HttpStatus {
        status,
        code,
        message,
    }
}

fn truncate_message(message: &str) -> String {
    message.chars().take(MAX_ERROR_MESSAGE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn request(model: &str) -> GatewayChatRequest {
        GatewayChatRequest {
            model: model.to_owned(),
            messages: vec![
                GatewayChatMessage::system("return JSON"),
                GatewayChatMessage::user("hello"),
            ],
            stream: false,
        }
    }

    fn read_request(socket: &mut std::net::TcpStream) -> String {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set mock read timeout");
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end;
        loop {
            let bytes = socket.read(&mut chunk).expect("read request");
            assert!(bytes > 0, "mock request ended before headers");
            buffer.extend_from_slice(&chunk[..bytes]);
            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                header_end = index + 4;
                break;
            }
        }

        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
            .expect("content-length header");
        while buffer.len() < header_end + content_length {
            let bytes = socket.read(&mut chunk).expect("read request body");
            assert!(bytes > 0, "mock request ended before body");
            buffer.extend_from_slice(&chunk[..bytes]);
        }
        String::from_utf8(buffer).expect("request is UTF-8")
    }

    fn write_response(mut socket: std::net::TcpStream, status: &str, body: &str) {
        socket
            .write_all(
                format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Request-Id: request-test\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .expect("write mock response");
    }

    #[test]
    fn gateway_client_posts_openai_chat_request_with_auth_header() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept request");
            let request = read_request(&mut socket);
            let request_lower = request.to_ascii_lowercase();

            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(request_lower.contains("authorization: bearer test-token"));
            assert!(request.contains("\"model\":\"gpt-test\""));
            assert!(request.contains("\"role\":\"system\""));
            assert!(request.contains("\"role\":\"user\""));
            assert!(request.contains("\"stream\":false"));

            let body = r#"{
                "id":"chatcmpl-test",
                "model":"gpt-test-resolved",
                "choices":[{
                    "index":0,
                    "message":{
                        "role":"assistant",
                        "content":"{\"summary\":\"mock response\",\"steps\":[\"one\"]}"
                    },
                    "finish_reason":"stop"
                }]
            }"#;
            write_response(socket, "200 OK", body);
        });

        let client = GatewayClient::new(
            GatewayClientConfig::new(format!("http://{address}")).with_auth_token("test-token"),
        )
        .expect("create client");

        let response = client.chat(request("gpt-test")).expect("chat response");

        assert_eq!(
            response.content,
            r#"{"summary":"mock response","steps":["one"]}"#
        );
        assert_eq!(response.model, "gpt-test-resolved");
        assert_eq!(response.request_id.as_deref(), Some("request-test"));
        server.join().expect("mock server");
    }

    #[test]
    fn gateway_client_maps_non_success_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept request");
            let _ = read_request(&mut socket);
            write_response(
                socket,
                "503 Service Unavailable",
                r#"{"error":{"code":"provider_unavailable","message":"no route"}}"#,
            );
        });

        let client = GatewayClient::new(GatewayClientConfig::new(format!("http://{address}")))
            .expect("create client");
        let error = client
            .chat(request("gpt-test"))
            .expect_err("expected Gateway error");

        assert!(matches!(
            error,
            GatewayError::HttpStatus {
                status: 503,
                code: Some(ref code),
                ..
            } if code == "provider_unavailable"
        ));
        server.join().expect("mock server");
    }

    #[test]
    fn gateway_client_rejects_empty_choices() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
        let address = listener.local_addr().expect("mock address");
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept request");
            let _ = read_request(&mut socket);
            write_response(socket, "200 OK", r#"{"model":"gpt-test","choices":[]}"#);
        });

        let client = GatewayClient::new(GatewayClientConfig::new(format!("http://{address}")))
            .expect("create client");
        let error = client
            .chat(request("gpt-test"))
            .expect_err("expected malformed response");

        assert!(
            matches!(error, GatewayError::MalformedResponse(message) if message == "choices is empty")
        );
        server.join().expect("mock server");
    }

    #[test]
    fn gateway_client_rejects_urls_with_credentials_or_paths() {
        assert!(matches!(
            GatewayClient::new(GatewayClientConfig::new("http://user:pass@127.0.0.1:4200")),
            Err(GatewayError::CredentialsInUrl)
        ));
        assert!(matches!(
            GatewayClient::new(GatewayClientConfig::new("http://127.0.0.1:4200/api")),
            Err(GatewayError::InvalidBaseUrl(_))
        ));
    }
}
