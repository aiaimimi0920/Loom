use loom_protocol::{CapabilityErrorCode, ExtensionUnitAttachment};
use serde_json::{json, Value};

use crate::commands::{
    CommandFailure, PLUGIN_ID, RESULT_ATTACHMENT_ID, RESULT_RENDERER_ID, RESULT_TYPE_ID,
};
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

pub(crate) fn copy_selected_text(
    attachments: &[ExtensionUnitAttachment],
) -> Result<Value, CommandFailure> {
    let text = current_payload(attachments)?.selected_text();
    if text.is_empty() || text.len() > MAX_COPY_BYTES {
        return Err(invalid_input(
            "Select one or more OCR blocks with Shift+click first",
        ));
    }
    Ok(json!({ "effects": copy_effects(&text, "已选 OCR 文本已复制") }))
}

pub(crate) fn copy_block(
    input: &Value,
    attachments: &[ExtensionUnitAttachment],
) -> Result<Value, CommandFailure> {
    if shift_pressed(input) {
        return toggle_block_selection(input, attachments);
    }
    let text = input
        .get("surfaceEvent")
        .and_then(|value| value.get("payload"))
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_COPY_BYTES)
        .ok_or_else(|| invalid_input("OCR text block is invalid"))?;
    Ok(json!({ "effects": copy_effects(text, "OCR 文本已复制") }))
}

fn toggle_block_selection(
    input: &Value,
    attachments: &[ExtensionUnitAttachment],
) -> Result<Value, CommandFailure> {
    let index = event_payload(input)
        .and_then(|payload| payload.get("blockIndex"))
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| invalid_input("OCR block index is invalid"))?;
    let attachment = current_attachment(attachments)?;
    let mut payload = parse_payload(attachment)?;
    let (selected, selected_count) = payload
        .toggle_block_selection(index)
        .ok_or_else(|| invalid_input("OCR block index is outside the cached result"))?;
    let message = if selected {
        format!("已选择 {selected_count} 个 OCR 文本块")
    } else {
        format!("已取消选择；当前选择 {selected_count} 个 OCR 文本块")
    };
    Ok(json!({
        "output": { "selected": selected, "selectedCount": selected_count },
        "effects": [upsert_effect(attachment, &payload)?, notice_effect(&message)],
    }))
}

fn current_payload(
    attachments: &[ExtensionUnitAttachment],
) -> Result<OcrAttachmentPayload, CommandFailure> {
    parse_payload(current_attachment(attachments)?)
}

fn current_attachment(
    attachments: &[ExtensionUnitAttachment],
) -> Result<&ExtensionUnitAttachment, CommandFailure> {
    attachments
        .iter()
        .find(|attachment| {
            attachment.plugin_id == PLUGIN_ID
                && attachment.attachment_id == RESULT_ATTACHMENT_ID
                && attachment.type_id == RESULT_TYPE_ID
                && attachment.schema_version == "1"
                && attachment
                    .renderer_id
                    .as_deref()
                    .is_none_or(|renderer| renderer == RESULT_RENDERER_ID)
        })
        .ok_or_else(missing_result)
}

fn parse_payload(
    attachment: &ExtensionUnitAttachment,
) -> Result<OcrAttachmentPayload, CommandFailure> {
    serde_json::from_value(attachment.payload.clone()).map_err(|_| missing_result())
}

fn event_payload(input: &Value) -> Option<&Value> {
    input.get("surfaceEvent")?.get("payload")
}

fn shift_pressed(input: &Value) -> bool {
    input
        .pointer("/surfaceEvent/modifiers/shiftKey")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn upsert_effect(
    attachment: &ExtensionUnitAttachment,
    payload: &OcrAttachmentPayload,
) -> Result<Value, CommandFailure> {
    let revision = attachment
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid_input("OCR attachment revision is exhausted"))?;
    Ok(json!({
        "type": "attachment.upsert",
        "payload": {
            "attachmentId": RESULT_ATTACHMENT_ID,
            "typeId": RESULT_TYPE_ID,
            "schemaVersion": "1",
            "priorRevision": attachment.revision,
            "revision": revision,
            "rendererId": RESULT_RENDERER_ID,
            "payload": payload,
            "resourceRefs": [],
        }
    }))
}

fn copy_effects(text: &str, notice: &str) -> Vec<Value> {
    vec![
        json!({ "type": "clipboard.writeText", "payload": { "text": text } }),
        json!({ "type": "notice.show", "payload": { "title": "OCR", "message": notice } }),
    ]
}

fn notice_effect(message: &str) -> Value {
    json!({ "type": "notice.show", "payload": { "title": "OCR", "message": message } })
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

        let mut wrong_type = attachment.clone();
        wrong_type.type_id = "untrusted/result".to_owned();
        assert!(copy_full_text(&[wrong_type]).is_err());

        let invalid_selection = copy_block(
            &json!({
                "surfaceEvent": {
                    "modifiers": { "shiftKey": true },
                    "payload": { "text": "missing", "blockIndex": 128 }
                }
            }),
            &[attachment.clone()],
        );
        assert!(invalid_selection.is_err());

        let selection = copy_block(
            &json!({
                "surfaceEvent": {
                    "modifiers": { "shiftKey": true },
                    "payload": { "text": "right", "blockIndex": 1 }
                }
            }),
            &[attachment.clone()],
        )
        .expect("select block");
        assert_eq!(selection["output"]["selectedCount"], 1);
        assert_eq!(
            selection["effects"][0]["payload"]["payload"]["selectedBlockIndices"],
            json!([1])
        );
        assert_eq!(
            selection["effects"][0]["payload"]["payload"]["surfaceScene"]["children"][1]["style"]
                ["borderColor"],
            "#b7f34a"
        );
        let mut selected_attachment = attachment.clone();
        selected_attachment.revision = 2;
        selected_attachment.payload = selection["effects"][0]["payload"]["payload"].clone();
        let copied = copy_selected_text(&[selected_attachment]).expect("copy selection");
        assert_eq!(copied["effects"][0]["payload"]["text"], "right");

        let response = copy_layout_text(&[attachment]).expect("layout copy");
        assert_eq!(response["effects"][0]["type"], "clipboard.writeText");
        assert_eq!(
            response["effects"][0]["payload"]["text"],
            "left        right"
        );
        assert_eq!(response["effects"][1]["type"], "notice.show");
    }
}
