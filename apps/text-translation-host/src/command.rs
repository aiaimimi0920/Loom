use loom_protocol::{CapabilityErrorCode, CapabilityProtocolError};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::translation::{self, TranslationInput};

const TRANSLATE_COMMAND: &str = "neuro.official/text-translation.translate";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandEnvelope {
    command_id: String,
    input: TranslationInput,
    #[serde(default)]
    target: Option<Value>,
    #[serde(default)]
    resource_refs: Vec<Value>,
    #[serde(default)]
    unit_attachments: Vec<Value>,
    #[serde(default)]
    staged_resources: Vec<Value>,
    #[serde(default)]
    user_gesture: bool,
}

pub fn execute(payload: Value) -> Result<Value, CapabilityProtocolError> {
    let envelope: CommandEnvelope = serde_json::from_value(payload)
        .map_err(|_| invalid_input("text translation command payload is invalid"))?;
    if envelope.command_id != TRANSLATE_COMMAND {
        return Err(invalid_input("text translation command id is unknown"));
    }
    if envelope.target.is_some()
        || !envelope.resource_refs.is_empty()
        || !envelope.unit_attachments.is_empty()
        || !envelope.staged_resources.is_empty()
        || envelope.user_gesture
    {
        return Err(invalid_input(
            "text translation does not accept resources, attachments, or gesture grants",
        ));
    }
    let output =
        translation::translate(envelope.input).map_err(|error| invalid_input(error.to_string()))?;
    Ok(json!({ "output": output, "effects": [] }))
}

fn invalid_input(message: impl Into<String>) -> CapabilityProtocolError {
    CapabilityProtocolError {
        code: CapabilityErrorCode::InvalidInput,
        message: message.into(),
        retryable: false,
    }
}
