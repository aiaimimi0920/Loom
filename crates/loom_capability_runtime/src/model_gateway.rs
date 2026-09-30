//! The host selects the Gateway origin/model; plugins never receive its credential.
use std::time::Duration;

use loom_gateway::{GatewayChatMessage, GatewayChatRequest, GatewayClient, GatewayClientConfig};
use serde_json::{json, Value};

pub(super) fn complete(
    system: String,
    user: String,
    response_schema: Option<Value>,
) -> Result<String, String> {
    let model = std::env::var("LOOM_GATEWAY_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .ok_or("Configure a Loom Gateway model before translating")?;
    let origin = std::env::var("LOOM_GATEWAY_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:4200".to_owned());
    let mut config = GatewayClientConfig::new(origin).with_timeout(Duration::from_secs(25));
    if let Ok(token) = std::env::var("LOOM_GATEWAY_TOKEN") {
        config = config.with_auth_token(token);
    }
    let client = GatewayClient::new(config).map_err(|_| "Loom Gateway configuration is invalid")?;
    client
        .chat(GatewayChatRequest {
            model,
            messages: vec![
                GatewayChatMessage::system(system),
                GatewayChatMessage::user(user),
            ],
            stream: false,
            temperature: None,
            response_format: response_schema.map(|schema| {
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": "response", "strict": true, "schema": schema }
                })
            }),
        })
        .map(|response| response.content)
        .map_err(|_| "Loom Gateway translation request failed".to_owned())
}
