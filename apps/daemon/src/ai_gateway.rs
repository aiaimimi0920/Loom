//! Global AI Gateway relay service ("configure once on Loom, share across all managed apps & devices").
//!
//! Supports loopback (`127.0.0.1` / `::1`), private home LAN HTTP (`10/8`, `172.16/12`, `192.168/16`, ULA),
//! and remote HTTPS Gateways. Persists non-secret configuration in `<control-plane>/settings/ai-gateway.json`
//! and isolates bearer credentials inside `CredentialStore` so tokens never appear in status/config responses.

use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use loom_gateway::{
    classify_gateway_origin, GatewayClient, GatewayClientConfig, GatewayError, GatewayOriginKind,
};
use loom_tool_registry::credentials::{
    CredentialInput, CredentialScope, CredentialStore, CredentialValueType,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const DEFAULT_GATEWAY_BASE_URL: &str = "http://127.0.0.1:4200";
const DEFAULT_GATEWAY_TIMEOUT_SECS: u64 = 60;
const MIN_GATEWAY_TIMEOUT_SECS: u64 = 1;
const MAX_GATEWAY_TIMEOUT_SECS: u64 = 300;
const AI_GATEWAY_SETTINGS_FILE: &str = "ai-gateway.json";
const AI_GATEWAY_TOKEN_CREDENTIAL_KEY: &str = "bearer-token";
const AI_GATEWAY_CREDENTIAL_FRAMEWORK_SCOPE: &str = "neuro.official/ai-gateway";

fn ai_gateway_credential_scope() -> CredentialScope {
    CredentialScope {
        framework_id: Some(AI_GATEWAY_CREDENTIAL_FRAMEWORK_SCOPE.to_owned()),
        art_id: None,
        mcp_server_id: None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AiGatewayConfig {
    Unconfigured,
    Gateway(AiGatewayEndpointConfig),
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct AiGatewayEndpointConfig {
    pub base_url: String,
    pub auth_token: Option<String>,
    pub default_model: Option<String>,
    pub timeout: Duration,
}

impl fmt::Debug for AiGatewayEndpointConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AiGatewayEndpointConfig")
            .field("base_url", &"[CONFIGURED]")
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("default_model", &self.default_model)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum AiGatewayConfigError {
    #[error("LOOM_GATEWAY_TIMEOUT_SECS must be an integer, got `{0}`")]
    InvalidTimeout(String),
    #[error(
        "LOOM_GATEWAY_TIMEOUT_SECS must be between {MIN_GATEWAY_TIMEOUT_SECS} and {MAX_GATEWAY_TIMEOUT_SECS}, got {0}"
    )]
    TimeoutOutOfRange(u64),
}

impl AiGatewayConfig {
    pub(crate) fn from_env() -> Result<Self, AiGatewayConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub(crate) fn from_lookup<F>(lookup: F) -> Result<Self, AiGatewayConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let explicit_url = lookup("LOOM_GATEWAY_BASE_URL")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let default_model = lookup("LOOM_GATEWAY_MODEL")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if explicit_url.is_none() && default_model.is_none() {
            return Ok(Self::Unconfigured);
        }

        let base_url = explicit_url.unwrap_or_else(|| DEFAULT_GATEWAY_BASE_URL.to_owned());
        let auth_token = lookup("LOOM_GATEWAY_TOKEN")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let timeout_seconds = match lookup("LOOM_GATEWAY_TIMEOUT_SECS")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(value) => value
                .parse::<u64>()
                .map_err(|_| AiGatewayConfigError::InvalidTimeout(value))?,
            None => DEFAULT_GATEWAY_TIMEOUT_SECS,
        };
        if !(MIN_GATEWAY_TIMEOUT_SECS..=MAX_GATEWAY_TIMEOUT_SECS).contains(&timeout_seconds) {
            return Err(AiGatewayConfigError::TimeoutOutOfRange(timeout_seconds));
        }

        Ok(Self::Gateway(AiGatewayEndpointConfig {
            base_url,
            auth_token,
            default_model,
            timeout: Duration::from_secs(timeout_seconds),
        }))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedAiGatewaySettings {
    pub enabled: bool,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub timeout_seconds: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AiGatewayConfigUpdate {
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub clear_auth_token: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AiGatewayStatus {
    pub mode: &'static str,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    pub has_token: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AiChatCompletionOutcome {
    pub response: Value,
    pub model: String,
    pub content: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Error)]
pub(crate) enum AiGatewayRelayError {
    #[error("AI Gateway is not configured on this Loom hub")]
    NotConfigured,
    #[error("invalid AI Gateway request: {0}")]
    InvalidRequest(String),
    #[error("AI Gateway configuration error: {0}")]
    InvalidConfig(String),
    #[error("AI Gateway persistence failed: {0}")]
    Storage(String),
    #[error("AI Gateway upstream error: {0}")]
    Upstream(#[from] GatewayError),
}

pub(crate) trait AiGatewayService: Send + Sync {
    fn status(&self) -> AiGatewayStatus;
    fn chat_completion(
        &self,
        request: Value,
    ) -> Result<AiChatCompletionOutcome, AiGatewayRelayError>;
    fn list_models(&self) -> Result<Value, AiGatewayRelayError>;
    fn update_config(
        &self,
        update: AiGatewayConfigUpdate,
        control_plane_root: &Path,
    ) -> Result<AiGatewayStatus, AiGatewayRelayError>;
}

pub(crate) type SharedAiGateway = Arc<dyn AiGatewayService>;

struct ActiveGatewayEndpoint {
    client: GatewayClient,
    base_url: String,
    origin_kind: GatewayOriginKind,
    default_model: Option<String>,
    timeout: Duration,
    has_token: bool,
}

pub(crate) struct ManagedAiGateway {
    inner: RwLock<Option<ActiveGatewayEndpoint>>,
}

impl ManagedAiGateway {
    fn from_endpoint(
        config: AiGatewayEndpointConfig,
    ) -> Result<ActiveGatewayEndpoint, GatewayError> {
        let has_token = config
            .auth_token
            .as_ref()
            .is_some_and(|token| !token.trim().is_empty());
        let mut client_config =
            GatewayClientConfig::new(config.base_url.clone()).with_timeout(config.timeout);
        if let Some(token) = config.auth_token {
            client_config = client_config.with_auth_token(token);
        }
        let client = GatewayClient::new(client_config)?;
        let origin_kind = client.origin_kind();
        Ok(ActiveGatewayEndpoint {
            client,
            base_url: config.base_url,
            origin_kind,
            default_model: config.default_model,
            timeout: config.timeout,
            has_token,
        })
    }

    pub(crate) fn new(
        initial: AiGatewayConfig,
        control_plane_root: Option<&Path>,
    ) -> Result<Self, AiGatewayRelayError> {
        let resolved = match initial {
            AiGatewayConfig::Gateway(endpoint) => Some(Self::from_endpoint(endpoint)?),
            AiGatewayConfig::Unconfigured => match control_plane_root {
                Some(root) => load_persisted_endpoint(root)?,
                None => None,
            },
        };
        Ok(Self {
            inner: RwLock::new(resolved),
        })
    }
}

fn settings_file_path(control_plane_root: &Path) -> std::path::PathBuf {
    control_plane_root
        .join("settings")
        .join(AI_GATEWAY_SETTINGS_FILE)
}

fn load_persisted_token(control_plane_root: &Path) -> Option<String> {
    let store = CredentialStore::new(control_plane_root);
    let scope = ai_gateway_credential_scope();
    store
        .reveal(AI_GATEWAY_TOKEN_CREDENTIAL_KEY, &scope)
        .ok()
        .flatten()
        .map(|details| details.value)
        .filter(|value| !value.trim().is_empty())
}

fn load_persisted_endpoint(
    control_plane_root: &Path,
) -> Result<Option<ActiveGatewayEndpoint>, AiGatewayRelayError> {
    let path = settings_file_path(control_plane_root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AiGatewayRelayError::Storage(error.to_string())),
    };
    let persisted: PersistedAiGatewaySettings = serde_json::from_slice(&bytes)
        .map_err(|error| AiGatewayRelayError::Storage(error.to_string()))?;
    if !persisted.enabled || persisted.base_url.trim().is_empty() {
        return Ok(None);
    }
    let timeout_secs = persisted
        .timeout_seconds
        .clamp(MIN_GATEWAY_TIMEOUT_SECS, MAX_GATEWAY_TIMEOUT_SECS);
    let endpoint = AiGatewayEndpointConfig {
        base_url: persisted.base_url.trim().to_owned(),
        auth_token: load_persisted_token(control_plane_root),
        default_model: persisted
            .default_model
            .map(|model| model.trim().to_owned())
            .filter(|model| !model.is_empty()),
        timeout: Duration::from_secs(timeout_secs),
    };
    ManagedAiGateway::from_endpoint(endpoint)
        .map(Some)
        .map_err(AiGatewayRelayError::Upstream)
}

impl AiGatewayService for ManagedAiGateway {
    fn status(&self) -> AiGatewayStatus {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match guard.as_ref() {
            None => AiGatewayStatus {
                mode: "unconfigured",
                configured: false,
                origin_kind: None,
                base_url: None,
                model: None,
                timeout_seconds: None,
                has_token: false,
            },
            Some(active) => AiGatewayStatus {
                mode: "gateway",
                configured: true,
                origin_kind: Some(active.origin_kind.as_str()),
                base_url: Some(active.base_url.clone()),
                model: active.default_model.clone(),
                timeout_seconds: Some(active.timeout.as_secs()),
                has_token: active.has_token,
            },
        }
    }

    fn chat_completion(
        &self,
        request: Value,
    ) -> Result<AiChatCompletionOutcome, AiGatewayRelayError> {
        let (client, default_model) = {
            let guard = self
                .inner
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(active) = guard.as_ref() else {
                return Err(AiGatewayRelayError::NotConfigured);
            };
            (active.client.clone(), active.default_model.clone())
        };

        let mut payload = request
            .as_object()
            .cloned()
            .ok_or_else(|| AiGatewayRelayError::InvalidRequest("JSON object required".into()))?;
        let model = payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or(default_model)
            .ok_or_else(|| {
                AiGatewayRelayError::InvalidRequest(
                    "model is required when no default Gateway model is configured".into(),
                )
            })?;
        payload.insert("model".to_owned(), json!(model));
        let messages_valid = payload
            .get("messages")
            .and_then(Value::as_array)
            .is_some_and(|messages| !messages.is_empty());
        if !messages_valid {
            return Err(AiGatewayRelayError::InvalidRequest(
                "messages must be a non-empty array".into(),
            ));
        }

        let (response, request_id) = client.chat_completion(&Value::Object(payload))?;
        let resolved_model = response
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&model)
            .to_owned();
        let content = response
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok(AiChatCompletionOutcome {
            response,
            model: resolved_model,
            content,
            request_id,
        })
    }

    fn list_models(&self) -> Result<Value, AiGatewayRelayError> {
        let client = {
            let guard = self
                .inner
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(active) = guard.as_ref() else {
                return Err(AiGatewayRelayError::NotConfigured);
            };
            active.client.clone()
        };
        Ok(client.list_models()?)
    }

    fn update_config(
        &self,
        update: AiGatewayConfigUpdate,
        control_plane_root: &Path,
    ) -> Result<AiGatewayStatus, AiGatewayRelayError> {
        let timeout_seconds = update
            .timeout_seconds
            .unwrap_or(DEFAULT_GATEWAY_TIMEOUT_SECS);
        if !(MIN_GATEWAY_TIMEOUT_SECS..=MAX_GATEWAY_TIMEOUT_SECS).contains(&timeout_seconds) {
            return Err(AiGatewayRelayError::InvalidConfig(format!(
                "timeoutSeconds must be between {MIN_GATEWAY_TIMEOUT_SECS} and {MAX_GATEWAY_TIMEOUT_SECS}"
            )));
        }
        let base_url = update.base_url.trim().to_owned();
        if update.enabled {
            if base_url.is_empty() {
                return Err(AiGatewayRelayError::InvalidConfig(
                    "baseUrl is required when enabling AI Gateway".into(),
                ));
            }
            classify_gateway_origin(&base_url)
                .map_err(|error| AiGatewayRelayError::InvalidConfig(error.to_string()))?;
        } else if !base_url.is_empty() {
            classify_gateway_origin(&base_url)
                .map_err(|error| AiGatewayRelayError::InvalidConfig(error.to_string()))?;
        }

        let credential_store = CredentialStore::new(control_plane_root);
        let credential_scope = ai_gateway_credential_scope();
        if update.clear_auth_token {
            let _ = credential_store.delete(AI_GATEWAY_TOKEN_CREDENTIAL_KEY, &credential_scope);
        } else if let Some(token) = update
            .auth_token
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            credential_store
                .upsert(CredentialInput {
                    name: AI_GATEWAY_TOKEN_CREDENTIAL_KEY.to_owned(),
                    value: token.to_owned(),
                    value_type: CredentialValueType::String,
                    scope: credential_scope,
                    expires_at: None,
                })
                .map_err(|error| AiGatewayRelayError::Storage(error.to_string()))?;
        }

        let default_model = update
            .default_model
            .map(|model| model.trim().to_owned())
            .filter(|model| !model.is_empty());
        let persisted = PersistedAiGatewaySettings {
            enabled: update.enabled,
            base_url: base_url.clone(),
            default_model: default_model.clone(),
            timeout_seconds,
        };
        let path = settings_file_path(control_plane_root);
        crate::write_json_atomically(&path, &persisted)
            .map_err(|error| AiGatewayRelayError::Storage(error.to_string()))?;

        let next_active = if update.enabled {
            Some(ManagedAiGateway::from_endpoint(AiGatewayEndpointConfig {
                base_url,
                auth_token: load_persisted_token(control_plane_root),
                default_model,
                timeout: Duration::from_secs(timeout_seconds),
            })?)
        } else {
            None
        };
        *self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next_active;
        Ok(self.status())
    }
}

pub(crate) fn build_ai_gateway(
    config: AiGatewayConfig,
    control_plane_root: Option<&Path>,
) -> Result<SharedAiGateway, AiGatewayRelayError> {
    Ok(Arc::new(ManagedAiGateway::new(config, control_plane_root)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn env_config_supports_unconfigured_loopback_and_lan_urls() {
        let empty = HashMap::<String, String>::new();
        assert_eq!(
            AiGatewayConfig::from_lookup(|name| empty.get(name).cloned()).expect("unconfigured"),
            AiGatewayConfig::Unconfigured
        );

        let mut values = HashMap::new();
        values.insert(
            "LOOM_GATEWAY_BASE_URL".to_owned(),
            "http://192.168.1.40:4200".to_owned(),
        );
        values.insert("LOOM_GATEWAY_MODEL".to_owned(), "qwen-home".to_owned());
        values.insert("LOOM_GATEWAY_TOKEN".to_owned(), "secret-token".to_owned());
        values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "15".to_owned());
        let config =
            AiGatewayConfig::from_lookup(|name| values.get(name).cloned()).expect("lan config");
        assert!(matches!(config, AiGatewayConfig::Gateway(_)));
        assert!(!format!("{config:?}").contains("secret-token"));

        values.insert("LOOM_GATEWAY_TIMEOUT_SECS".to_owned(), "0".to_owned());
        assert_eq!(
            AiGatewayConfig::from_lookup(|name| values.get(name).cloned())
                .expect_err("zero timeout"),
            AiGatewayConfigError::TimeoutOutOfRange(0)
        );
    }
}
