//! Client contracts for the external Neuro Gateway (local loopback, LAN, and remote HTTPS).

mod origin;
#[cfg(test)]
mod tests;

use std::io::Read;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::{Proxy, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

pub use origin::{classify_gateway_origin, configure_runtime_proxy, GatewayOriginKind};
use origin::{parse_validated_origin, runtime_proxy_store, RuntimeProxy};

/// Version of the Gateway integration crate.
pub const LOOM_GATEWAY_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_GATEWAY_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_GATEWAY_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
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
    #[error("Gateway requires HTTPS except for direct loopback or private LAN IP origins")]
    InsecureTransport,
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

/// Configuration for a local, LAN, or hosted Gateway HTTP client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayClientConfig {
    base_url: String,
    auth_token: Option<String>,
    timeout: Duration,
    disable_proxy: bool,
}

impl GatewayClientConfig {
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: None,
            timeout: DEFAULT_GATEWAY_TIMEOUT,
            disable_proxy: false,
        }
    }

    /// Create a client configuration restricted to an IP loopback origin.
    pub fn new_loopback(base_url: impl Into<String>) -> GatewayResult<Self> {
        let raw = base_url.into();
        let (_, kind) = parse_validated_origin(&raw)?;
        if kind != GatewayOriginKind::Loopback {
            return Err(GatewayError::InvalidBaseUrl(
                "base URL host must be an IP loopback address".to_owned(),
            ));
        }
        Ok(Self::new(raw))
    }

    #[must_use]
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        let token = token.into();
        self.auth_token = (!token.trim().is_empty()).then_some(token);
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn without_proxy(mut self) -> Self {
        self.disable_proxy = true;
        self
    }
}

/// OpenAI-compatible chat message forwarded to Gateway.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<Value>,
}

/// Normalized first assistant message returned by Gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayChatResponse {
    pub model: String,
    pub content: String,
    pub request_id: Option<String>,
}

/// Validated HTTP client for the external AI Gateway.
#[derive(Clone, Debug)]
pub struct GatewayClient {
    http: Client,
    chat_url: Url,
    models_url: Url,
    origin_kind: GatewayOriginKind,
    auth_token: Option<String>,
}

impl GatewayClient {
    /// Build a validated client from an origin URL.
    pub fn new(config: GatewayClientConfig) -> GatewayResult<Self> {
        let (base_url, origin_kind) = parse_validated_origin(&config.base_url)?;
        let mut chat_url = base_url.clone();
        chat_url.set_path("/v1/chat/completions");
        let mut models_url = base_url;
        models_url.set_path("/v1/models");

        // Loopback and private LAN traffic must bypass system/custom Internet proxies.
        let bypass_proxy = config.disable_proxy
            || matches!(
                origin_kind,
                GatewayOriginKind::Loopback | GatewayOriginKind::PrivateLan
            );
        let builder = Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none());
        let builder = if bypass_proxy {
            builder.no_proxy()
        } else {
            match runtime_proxy_store()
                .read()
                .map(|proxy| proxy.clone())
                .unwrap_or_default()
            {
                RuntimeProxy::System => builder,
                RuntimeProxy::Disabled => builder.no_proxy(),
                RuntimeProxy::Custom(url) => builder.proxy(Proxy::all(url)?),
            }
        };
        let http = builder.build()?;
        Ok(Self {
            http,
            chat_url,
            models_url,
            origin_kind,
            auth_token: config.auth_token,
        })
    }

    #[must_use]
    pub fn origin_kind(&self) -> GatewayOriginKind {
        self.origin_kind
    }

    /// Query `GET /v1/models` from the configured Gateway.
    pub fn list_models(&self) -> GatewayResult<Value> {
        let mut builder = self.http.get(self.models_url.clone());
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        let (body, _) = send_bounded(builder)?;
        serde_json::from_slice(&body)
            .map_err(|error| GatewayError::MalformedResponse(error.to_string()))
    }

    /// Forward an OpenAI-compatible non-streaming JSON chat payload to `/v1/chat/completions`.
    pub fn chat_completion(&self, payload: &Value) -> GatewayResult<(Value, Option<String>)> {
        let mut object = payload.as_object().cloned().ok_or_else(|| {
            GatewayError::MalformedResponse("chat request must be an object".into())
        })?;
        object.insert("stream".to_owned(), json!(false));
        let mut builder = self
            .http
            .post(self.chat_url.clone())
            .json(&Value::Object(object));
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        let (body, request_id) = send_bounded(builder)?;
        let parsed: Value = serde_json::from_slice(&body)
            .map_err(|error| GatewayError::MalformedResponse(error.to_string()))?;
        Ok((parsed, request_id))
    }

    /// Send one bounded, non-streaming typed chat request.
    pub fn chat(&self, request: GatewayChatRequest) -> GatewayResult<GatewayChatResponse> {
        let mut builder = self.http.post(self.chat_url.clone()).json(&request);
        if let Some(token) = self.auth_token.as_deref() {
            builder = builder.bearer_auth(token);
        }
        let (body, request_id) = send_bounded(builder)?;
        parse_chat_response(&body, &request.model, request_id)
    }
}

fn send_bounded(
    builder: reqwest::blocking::RequestBuilder,
) -> GatewayResult<(Vec<u8>, Option<String>)> {
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
    Ok((body, request_id))
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
