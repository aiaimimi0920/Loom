use loom_protocol::{
    CapabilityErrorCode, CapabilityProtocolError, ExtensionTarget, ExtensionUnitAttachment,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::translation;
use crate::translation_input::{TranslationInput, TranslationProviderMode};

const TRANSLATE_COMMAND: &str = "neuro.official/text-translation.translate";
const TOGGLE_COMMAND: &str = "neuro.official/text-translation.toggle";
const TOGGLE_OVERLAY_COMMAND: &str = "neuro.official/text-translation.toggle-overlay";
const PLUGIN: &str = "neuro.official/text-translation";
const ATTACHMENT: &str = "neuro.official/text-translation.result";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandEnvelope {
    command_id: String,
    #[serde(default)]
    input: Option<Value>,
    #[serde(default)]
    target: Option<ExtensionTarget>,
    #[serde(default)]
    resource_refs: Vec<Value>,
    #[serde(default)]
    unit_attachments: Vec<ExtensionUnitAttachment>,
    #[serde(default)]
    staged_resources: Vec<Value>,
    #[serde(default)]
    #[serde(rename = "userGesture")]
    _user_gesture: bool,
}

pub fn execute(payload: Value) -> Result<Value, CapabilityProtocolError> {
    execute_with(payload, crate::model_client::complete)
}

pub(crate) fn execute_with(
    payload: Value,
    complete: impl FnMut(&str, &str, &Value, TranslationProviderMode) -> anyhow::Result<String>,
) -> Result<Value, CapabilityProtocolError> {
    let envelope: CommandEnvelope = serde_json::from_value(payload)
        .map_err(|_| invalid_input("text translation command payload is invalid"))?;
    if envelope.command_id != TRANSLATE_COMMAND
        && envelope.command_id != TOGGLE_COMMAND
        && envelope.command_id != TOGGLE_OVERLAY_COMMAND
    {
        return Err(invalid_input("text translation command id is unknown"));
    }
    if !envelope.resource_refs.is_empty()
        || !envelope.staged_resources.is_empty()
        || envelope
            .unit_attachments
            .iter()
            .any(|item| item.plugin_id != PLUGIN || !item.resource_refs.is_empty())
    {
        return Err(invalid_input(
            "text translation accepts only its own resource-free attachments",
        ));
    }
    let existing = envelope
        .unit_attachments
        .iter()
        .find(|item| item.attachment_id == ATTACHMENT);
    if envelope.command_id == TOGGLE_OVERLAY_COMMAND {
        let attachment = existing.ok_or_else(|| invalid_input("translation result is missing"))?;
        // Visibility commands carry Hook's empty input and never require OCR or a model.
        let mut payload = attachment
            .payload
            .as_object()
            .cloned()
            .ok_or_else(|| invalid_input("translation result payload is invalid"))?;
        let revision = next_attachment_revision(attachment.revision)?;
        let visible = payload
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let next_visible = !visible;
        payload.insert("visible".to_owned(), Value::Bool(next_visible));
        if let Some(scene_props) = payload
            .get_mut("surfaceScene")
            .and_then(Value::as_object_mut)
            .and_then(|scene| scene.get_mut("props"))
            .and_then(Value::as_object_mut)
        {
            scene_props.insert("visible".to_owned(), Value::Bool(next_visible));
        }
        return Ok(json!({
            "output": { "visible": next_visible },
            "effects": [{ "type": "attachment.upsert", "payload": {
                "attachmentId": ATTACHMENT,
                "typeId": "neuro.official/text-translation.result.v1",
                "schemaVersion": "1",
                "priorRevision": attachment.revision,
                "revision": revision,
                "rendererId": "neuro.official/text-translation.renderer",
                "payload": payload,
                "resourceRefs": []
            }}]
        }));
    }
    let input: TranslationInput = serde_json::from_value(
        envelope
            .input
            .ok_or_else(|| invalid_input("translation input is missing"))?,
    )
    .map_err(|_| invalid_input("translation input is invalid"))?;
    if envelope
        .target
        .as_ref()
        .is_some_and(|target| input.source_revision != Some(target.revision))
    {
        return Err(invalid_input(
            "translation source revision does not match the target",
        ));
    }
    let prior_revision = existing.map_or(0, |attachment| attachment.revision);
    let revision = next_attachment_revision(prior_revision)?;
    let output = translation::translate_with(input, complete)
        .map_err(|error| invalid_input(error.to_string()))?;
    let output =
        serde_json::to_value(output).map_err(|_| invalid_input("translation output is invalid"))?;
    let translated_attachment = json!({
        "attachmentId": "neuro.official/text-translation.result",
        "typeId": "neuro.official/text-translation.result.v1",
        "schemaVersion": "1",
        "priorRevision": prior_revision,
        "revision": revision,
        "rendererId": "neuro.official/text-translation.renderer",
        "payload": output.clone(),
        "resourceRefs": []
    });
    Ok(json!({
        "output": output,
        "effects": [{ "type": "attachment.upsert", "payload": translated_attachment }]
    }))
}

fn next_attachment_revision(prior_revision: u64) -> Result<u64, CapabilityProtocolError> {
    prior_revision
        .checked_add(1)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid_input("translation attachment revision is exhausted"))
}

fn invalid_input(message: impl Into<String>) -> CapabilityProtocolError {
    CapabilityProtocolError {
        code: CapabilityErrorCode::InvalidInput,
        message: message.into(),
        retryable: false,
    }
}
