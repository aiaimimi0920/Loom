use loom_protocol::{CapabilityErrorCode, ExtensionUnitAttachment};
use serde_json::{json, Value};

use crate::commands::{CommandFailure, PLUGIN_ID, RESULT_ATTACHMENT_ID};
use crate::overlay::OcrAttachmentPayload;

const MAX_COPY_BYTES: usize = 1024 * 1024;

pub(crate) fn copy_full_text(
    attachments: &[ExtensionUnitAttachment],
) -> Result<Value, CommandFailure> {
    let payload = current_payload(attachments)?;
    let text = payload.copy_text();
    if text.is_empty() || text.len() > MAX_COPY_BYTES {
        return Err(missing_result());
    }
    Ok(json!({ "effects": copy_effects(&text, "OCR 全文已复制") }))
}

pub(crate) fn copy_layout_text(
    attachments: &[ExtensionUnitAttachment],
) -> Result<Value, CommandFailure> {
    let payload = current_payload(attachments)?;
    let text = crate::layout_text::compose(&payload.text_blocks, payload.show_translated)
        .ok_or_else(missing_result)?;
    if text.is_empty() || text.len() > MAX_COPY_BYTES {
        return Err(missing_result());
    }
    Ok(json!({ "effects": copy_effects(&text, "OCR 版式文本已复制") }))
}

pub(crate) fn copy_block(input: &Value) -> Result<Value, CommandFailure> {
    let text = input
        .get("surfaceEvent")
        .and_then(|value| value.get("payload"))
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_COPY_BYTES)
        .ok_or_else(|| invalid_input("OCR text block is invalid"))?;
    Ok(json!({ "effects": copy_effects(text, "OCR 文本已复制") }))
}

fn current_payload(
    attachments: &[ExtensionUnitAttachment],
) -> Result<OcrAttachmentPayload, CommandFailure> {
    let attachment = attachments
        .iter()
        .find(|attachment| {
            attachment.plugin_id == PLUGIN_ID && attachment.attachment_id == RESULT_ATTACHMENT_ID
        })
        .ok_or_else(missing_result)?;
    serde_json::from_value(attachment.payload.clone()).map_err(|_| missing_result())
}

fn copy_effects(text: &str, notice: &str) -> Vec<Value> {
    vec![
        json!({ "type": "clipboard.writeText", "payload": { "text": text } }),
        json!({ "type": "notice.show", "payload": { "title": "OCR", "message": notice } }),
    ]
}

fn missing_result() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::ResourceNotFound,
        "Run OCR before using the cached result",
        false,
    )
}

fn invalid_input(message: &str) -> CommandFailure {
    CommandFailure::new(CapabilityErrorCode::InvalidInput, message, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_command_uses_cached_geometry_and_brokered_effects() {
        let block = |text: &str, left: u32| {
            json!({
                "text": text,
                "left": left,
                "top": 0,
                "width": 10,
                "height": 10,
                "textColor": "#ffffff",
                "backgroundColor": "#000000",
                "boxPoints": [],
                "boxScore": 1.0,
                "textScore": 1.0,
                "colorHex": "#ffffff",
                "bgColorHex": "#000000"
            })
        };
        let attachment = ExtensionUnitAttachment {
            attachment_id: RESULT_ATTACHMENT_ID.to_owned(),
            type_id: "neuro.official/ocr.result.v1".to_owned(),
            schema_version: "1".to_owned(),
            revision: 1,
            plugin_id: PLUGIN_ID.to_owned(),
            plugin_version: "1.0.0".to_owned(),
            renderer_id: None,
            payload: json!({
                "schemaVersion": "1",
                "visible": true,
                "sourceWidth": 100,
                "sourceHeight": 100,
                "coordinateScale": 1.0,
                "fullText": "left\nright",
                "textBlocks": [block("left", 0), block("right", 30)],
                "surfaceScene": {}
            }),
            resource_refs: Vec::new(),
        };

        let mut oversized = attachment.clone();
        oversized.payload["fullText"] = json!("x".repeat(MAX_COPY_BYTES + 1));
        assert!(copy_full_text(&[oversized]).is_err());

        let mut excessive_blocks = attachment.clone();
        let block = excessive_blocks.payload["textBlocks"][0].clone();
        excessive_blocks.payload["textBlocks"] = Value::Array(vec![block; 129]);
        assert!(copy_layout_text(&[excessive_blocks]).is_err());

        let response = copy_layout_text(&[attachment]).expect("layout copy");
        assert_eq!(response["effects"][0]["type"], "clipboard.writeText");
        assert_eq!(
            response["effects"][0]["payload"]["text"],
            "left        right"
        );
        assert_eq!(response["effects"][1]["type"], "notice.show");
    }
}
