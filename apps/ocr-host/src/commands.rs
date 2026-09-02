use std::fs;
use std::path::PathBuf;

use loom_ocr::{discover_default_model_set, OcrEngine, OcrError};
use loom_protocol::{
    CapabilityErrorCode, CapabilityProtocolError, ExtensionResourceKind, ExtensionResourceRef,
    ExtensionUnitAttachment,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::code_exclusion;
use crate::code_overlay;
use crate::code_scan;
use crate::command_options::{parse_quality_mode, parse_region};
use crate::overlay;

pub const PLUGIN_ID: &str = "neuro.official/ocr";
pub const RESULT_ATTACHMENT_ID: &str = "neuro.official/ocr.result";
pub const RESULT_TYPE_ID: &str = "neuro.official/ocr.result.v1";
pub const RESULT_RENDERER_ID: &str = "neuro.official/ocr.result-overlay";
pub const CODES_ATTACHMENT_ID: &str = "neuro.official/ocr.codes";
pub const CODES_TYPE_ID: &str = "neuro.official/ocr.codes.v1";
pub const CODES_RENDERER_ID: &str = "neuro.official/ocr.codes-overlay";
const RECOGNIZE_COMMAND: &str = "neuro.official/ocr.recognize-selected-unit";
const TOGGLE_COMMAND: &str = "neuro.official/ocr.toggle-overlay";
const COPY_FULL_COMMAND: &str = "neuro.official/ocr.copy-full-text";
const COPY_LAYOUT_COMMAND: &str = "neuro.official/ocr.copy-layout-text";
const COPY_SELECTED_COMMAND: &str = "neuro.official/ocr.copy-selected-text";
const COPY_BLOCK_COMMAND: &str = "neuro.official/ocr.copy-block";
const SCAN_CODES_COMMAND: &str = "neuro.official/ocr.scan-codes";
const COPY_CODE_COMMAND: &str = "neuro.official/ocr.copy-code";
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COPY_BYTES: usize = 1024 * 1024;
const MAX_CODE_EDIT_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub struct CommandFailure {
    code: CapabilityErrorCode,
    message: String,
    retryable: bool,
}

impl CommandFailure {
    pub fn new(code: CapabilityErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn into_protocol_error(self) -> CapabilityProtocolError {
        CapabilityProtocolError {
            code: self.code,
            message: self.message,
            retryable: self.retryable,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandEnvelope {
    command_id: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    unit_attachments: Vec<ExtensionUnitAttachment>,
    #[serde(default)]
    staged_resources: Vec<StagedResource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagedResource {
    resource_ref: ExtensionResourceRef,
    staged_path: PathBuf,
}

pub fn execute(payload: Value, engine: &mut Option<OcrEngine>) -> Result<Value, CommandFailure> {
    let request: CommandEnvelope = serde_json::from_value(payload).map_err(|_| {
        CommandFailure::new(
            CapabilityErrorCode::InvalidInput,
            "OCR command payload is invalid",
            false,
        )
    })?;
    match request.command_id.as_str() {
        RECOGNIZE_COMMAND => recognize(&request, engine),
        TOGGLE_COMMAND => toggle_overlay(&request.unit_attachments),
        COPY_FULL_COMMAND => crate::copy_commands::copy_full_text(&request.unit_attachments),
        COPY_LAYOUT_COMMAND => crate::copy_commands::copy_layout_text(&request.unit_attachments),
        COPY_SELECTED_COMMAND => {
            crate::copy_commands::copy_selected_text(&request.unit_attachments)
        }
        COPY_BLOCK_COMMAND => {
            crate::copy_commands::copy_block(&request.input, &request.unit_attachments)
        }
        SCAN_CODES_COMMAND => scan_codes(&request),
        COPY_CODE_COMMAND => code_action(&request),
        _ => Err(CommandFailure::new(
            CapabilityErrorCode::InvalidInput,
            "OCR command is not declared by this capability",
            false,
        )),
    }
}

fn recognize(
    request: &CommandEnvelope,
    engine: &mut Option<OcrEngine>,
) -> Result<Value, CommandFailure> {
    let image = read_staged_image(&request.staged_resources)?;
    let quality_mode = parse_quality_mode(&request.input)?;
    let region = parse_region(&request.input)?;
    if engine.is_none() {
        *engine = Some(load_engine()?);
    }
    let result = engine
        .as_mut()
        .expect("engine was initialized")
        .detect_image_region_bytes_with_mode(&image, false, quality_mode, region)
        .map_err(map_ocr_error)?;
    let code_scan = code_scan::decode(&image).map_err(|_| {
        CommandFailure::new(
            CapabilityErrorCode::RuntimeFault,
            "QR/barcode decoding failed",
            true,
        )
    })?;
    let result = code_exclusion::suppress_code_text(result, &code_scan);
    let attachment = overlay::build_attachment_payload(&result, true);
    let code_attachment = code_overlay::build_attachment_payload(&code_scan);
    let prior_revision =
        current_ocr_attachment(&request.unit_attachments).map_or(0, |value| value.revision);
    let full_text = attachment.full_text.clone();
    let code_prior = current_code_attachment(&request.unit_attachments);
    let code_count = code_attachment.results.len();
    let mut effects = vec![upsert_ocr_effect(prior_revision, &attachment)];
    if code_count > 0 {
        effects.push(upsert_effect(
            code_prior.map_or(0, |value| value.revision),
            CODES_ATTACHMENT_ID,
            CODES_TYPE_ID,
            CODES_RENDERER_ID,
            &code_attachment,
        ));
    } else if let Some(previous) = code_prior {
        effects.push(remove_effect(previous));
    }
    if !full_text.is_empty() {
        effects.push(json!({ "type": "clipboard.writeText", "payload": { "text": &full_text } }));
        let message = if code_count == 0 {
            "OCR 全文已复制".to_owned()
        } else {
            format!("OCR 全文已复制；已标记 {code_count} 个二维码或条码")
        };
        effects.push(notice_effect(&message));
    } else if code_count > 0 {
        effects.push(notice_effect(&format!(
            "未识别到文本；已标记 {code_count} 个二维码或条码"
        )));
    } else {
        effects.push(notice_effect("OCR 未识别到文本或二维码/条码"));
    }
    Ok(json!({
        "output": {
            "blockCount": attachment.text_blocks.len(),
            "textBytes": full_text.len(),
            "codeCount": code_count,
        },
        "effects": effects,
    }))
}

fn map_ocr_error(error: OcrError) -> CommandFailure {
    match error {
        OcrError::InvalidImage(_) => CommandFailure::new(
            CapabilityErrorCode::InvalidInput,
            "OCR image or requested region is invalid",
            false,
        ),
        _ => CommandFailure::new(
            CapabilityErrorCode::RuntimeFault,
            "OCR inference failed",
            true,
        ),
    }
}

fn toggle_overlay(attachments: &[ExtensionUnitAttachment]) -> Result<Value, CommandFailure> {
    let ocr = current_ocr_attachment(attachments);
    let codes = current_code_attachment(attachments);
    if ocr.is_none() && codes.is_none() {
        return Err(missing_result());
    }
    let currently_visible = [ocr, codes]
        .into_iter()
        .flatten()
        .any(|attachment| attachment.payload.get("visible").and_then(Value::as_bool) == Some(true));
    let visible = !currently_visible;
    let mut effects = Vec::new();
    if let Some(attachment) = ocr {
        effects.push(toggle_attachment_effect(attachment, visible)?);
    }
    if let Some(attachment) = codes {
        effects.push(toggle_attachment_effect(attachment, visible)?);
    }
    Ok(json!({
        "output": { "visible": visible },
        "effects": effects,
    }))
}

fn scan_codes(request: &CommandEnvelope) -> Result<Value, CommandFailure> {
    let image = read_staged_image(&request.staged_resources)?;
    let scan = code_scan::decode(&image).map_err(|_| {
        CommandFailure::new(
            CapabilityErrorCode::RuntimeFault,
            "QR/barcode decoding failed",
            true,
        )
    })?;
    let attachment = code_overlay::build_attachment_payload(&scan);
    let prior_revision =
        current_code_attachment(&request.unit_attachments).map_or(0, |value| value.revision);
    let result_count = attachment.results.len();
    let message = if result_count == 0 {
        "未识别到二维码或条码".to_owned()
    } else {
        format!("已识别 {result_count} 个二维码或条码；点击中心标记可选择操作")
    };
    Ok(json!({
        "output": { "resultCount": result_count },
        "effects": [
            upsert_effect(
                prior_revision,
                CODES_ATTACHMENT_ID,
                CODES_TYPE_ID,
                CODES_RENDERER_ID,
                &attachment,
            ),
            notice_effect_with_title("OCR · 二维码/条码", &message),
        ],
    }))
}

fn code_action(request: &CommandEnvelope) -> Result<Value, CommandFailure> {
    let payload = request
        .input
        .get("surfaceEvent")
        .and_then(|value| value.get("payload"));
    let operation = payload
        .and_then(|value| value.get("operation"))
        .and_then(Value::as_str)
        .unwrap_or("legacy_copy");
    if matches!(operation, "select" | "dismiss") {
        let attachment =
            current_code_attachment(&request.unit_attachments).ok_or_else(missing_code_result)?;
        let selected_id = if operation == "select" {
            Some(code_result_id(payload).ok_or_else(missing_code_result)?)
        } else {
            None
        };
        let mut value = attachment.payload.clone();
        if !code_overlay::set_selection(&mut value, selected_id) {
            return Err(missing_code_result());
        }
        return Ok(json!({
            "output": { "selectedId": selected_id },
            "effects": [upsert_effect(
                attachment.revision,
                CODES_ATTACHMENT_ID,
                CODES_TYPE_ID,
                CODES_RENDERER_ID,
                &value,
            )],
        }));
    }
    if matches!(operation, "copy" | "open") {
        let attachment =
            current_code_attachment(&request.unit_attachments).ok_or_else(missing_code_result)?;
        let state: code_overlay::CodeAttachmentPayload =
            serde_json::from_value(attachment.payload.clone())
                .map_err(|_| missing_code_result())?;
        let result_id = code_result_id(payload).ok_or_else(missing_code_result)?;
        let result = state
            .results
            .iter()
            .find(|result| result.id == result_id)
            .ok_or_else(missing_code_result)?;
        let text = edited_code_text(payload, &result.text)?;
        if operation == "open" {
            if result.url.is_none() {
                return Err(missing_code_result());
            }
            let url = code_scan::classify_https_url(text).ok_or_else(invalid_code_edit)?;
            return Ok(json!({
                "effects": [{ "type": "external.openUrl", "payload": { "url": url } }]
            }));
        }
        return Ok(json!({
            "effects": copy_effects(text, "二维码/条码内容已复制")
        }));
    }
    legacy_copy_code(&request.input)
}

fn legacy_copy_code(input: &Value) -> Result<Value, CommandFailure> {
    let text = input
        .get("surfaceEvent")
        .and_then(|value| value.get("payload"))
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_COPY_BYTES)
        .ok_or_else(|| {
            CommandFailure::new(
                CapabilityErrorCode::InvalidInput,
                "QR/barcode payload is invalid",
                false,
            )
        })?;
    Ok(json!({
        "effects": copy_effects(text, "二维码/条码内容已复制")
    }))
}

fn code_result_id(payload: Option<&Value>) -> Option<&str> {
    payload
        .and_then(|value| value.get("resultId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64)
}

fn edited_code_text<'a>(
    payload: Option<&'a Value>,
    fallback: &'a str,
) -> Result<&'a str, CommandFailure> {
    let text = payload
        .and_then(|value| value.get("surfaceValues"))
        .and_then(|value| value.get(code_overlay::CODE_EDITOR_NODE_ID))
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    if text.is_empty() || text.len() > MAX_CODE_EDIT_BYTES {
        return Err(invalid_code_edit());
    }
    Ok(text)
}

fn read_staged_image(resources: &[StagedResource]) -> Result<Vec<u8>, CommandFailure> {
    let resource = resources
        .iter()
        .find(|value| value.resource_ref.kind == ExtensionResourceKind::SharedImage)
        .ok_or_else(|| {
            CommandFailure::new(
                CapabilityErrorCode::ResourceNotFound,
                "OCR image resource is missing",
                false,
            )
        })?;
    let metadata = fs::symlink_metadata(&resource.staged_path).map_err(|_| missing_image())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES
    {
        return Err(missing_image());
    }
    fs::read(&resource.staged_path).map_err(|_| missing_image())
}

fn load_engine() -> Result<OcrEngine, CommandFailure> {
    let models = discover_default_model_set()
        .map_err(|_| model_unavailable())?
        .ok_or_else(model_unavailable)?;
    OcrEngine::new(models).map_err(|_| model_unavailable())
}

fn current_ocr_attachment(
    attachments: &[ExtensionUnitAttachment],
) -> Option<&ExtensionUnitAttachment> {
    attachments.iter().find(|attachment| {
        attachment.plugin_id == PLUGIN_ID && attachment.attachment_id == RESULT_ATTACHMENT_ID
    })
}

fn current_code_attachment(
    attachments: &[ExtensionUnitAttachment],
) -> Option<&ExtensionUnitAttachment> {
    attachments.iter().find(|attachment| {
        attachment.plugin_id == PLUGIN_ID && attachment.attachment_id == CODES_ATTACHMENT_ID
    })
}

fn upsert_ocr_effect(revision: u64, payload: &impl Serialize) -> Value {
    upsert_effect(
        revision,
        RESULT_ATTACHMENT_ID,
        RESULT_TYPE_ID,
        RESULT_RENDERER_ID,
        payload,
    )
}

fn upsert_effect(
    revision: u64,
    attachment_id: &str,
    type_id: &str,
    renderer_id: &str,
    payload: &impl Serialize,
) -> Value {
    json!({
        "type": "attachment.upsert",
        "payload": {
            "attachmentId": attachment_id,
            "typeId": type_id,
            "schemaVersion": "1",
            "priorRevision": revision,
            "revision": revision + 1,
            "rendererId": renderer_id,
            "payload": payload,
            "resourceRefs": [],
        }
    })
}

fn remove_effect(attachment: &ExtensionUnitAttachment) -> Value {
    json!({
        "type": "attachment.remove",
        "payload": {
            "attachmentId": attachment.attachment_id,
            "priorRevision": attachment.revision,
        }
    })
}

fn toggle_attachment_effect(
    attachment: &ExtensionUnitAttachment,
    visible: bool,
) -> Result<Value, CommandFailure> {
    let mut payload = attachment.payload.clone();
    let scene_visible = payload
        .pointer_mut("/surfaceScene/props/visible")
        .ok_or_else(missing_result)?;
    *scene_visible = json!(visible);
    payload["visible"] = json!(visible);
    let renderer_id = attachment.renderer_id.as_deref().unwrap_or_else(|| {
        if attachment.attachment_id == CODES_ATTACHMENT_ID {
            CODES_RENDERER_ID
        } else {
            RESULT_RENDERER_ID
        }
    });
    Ok(upsert_effect(
        attachment.revision,
        &attachment.attachment_id,
        &attachment.type_id,
        renderer_id,
        &payload,
    ))
}

fn copy_effects(text: &str, notice: &str) -> Vec<Value> {
    vec![
        json!({ "type": "clipboard.writeText", "payload": { "text": text } }),
        notice_effect(notice),
    ]
}

fn notice_effect(message: &str) -> Value {
    notice_effect_with_title("OCR", message)
}

fn notice_effect_with_title(title: &str, message: &str) -> Value {
    json!({ "type": "notice.show", "payload": { "title": title, "message": message } })
}

fn missing_result() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::ResourceNotFound,
        "Run OCR before using the cached result",
        false,
    )
}

fn missing_code_result() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::ResourceNotFound,
        "QR/barcode result is unavailable",
        false,
    )
}

fn invalid_code_edit() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::InvalidInput,
        "Edited QR/barcode content is invalid",
        false,
    )
}

fn missing_image() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::ResourceNotFound,
        "OCR image resource is unavailable",
        false,
    )
}

fn model_unavailable() -> CommandFailure {
    CommandFailure::new(
        CapabilityErrorCode::RuntimeFault,
        "OCR models are unavailable",
        false,
    )
}
