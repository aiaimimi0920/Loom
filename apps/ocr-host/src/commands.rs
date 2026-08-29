use std::fs;
use std::path::{Path, PathBuf};

use loom_ocr::{OcrEngine, OcrModelSet};
use loom_protocol::{
    CapabilityErrorCode, CapabilityProtocolError, ExtensionResourceKind, ExtensionResourceRef,
    ExtensionUnitAttachment,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::overlay::{self, OcrAttachmentPayload};

pub const PLUGIN_ID: &str = "neuro.official/ocr";
pub const RESULT_ATTACHMENT_ID: &str = "neuro.official/ocr.result";
pub const RESULT_TYPE_ID: &str = "neuro.official/ocr.result.v1";
pub const RESULT_RENDERER_ID: &str = "neuro.official/ocr.result-overlay";
const RECOGNIZE_COMMAND: &str = "neuro.official/ocr.recognize-selected-unit";
const TOGGLE_COMMAND: &str = "neuro.official/ocr.toggle-overlay";
const COPY_FULL_COMMAND: &str = "neuro.official/ocr.copy-full-text";
const COPY_BLOCK_COMMAND: &str = "neuro.official/ocr.copy-block";
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COPY_BYTES: usize = 1024 * 1024;

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
        COPY_FULL_COMMAND => copy_full_text(&request.unit_attachments),
        COPY_BLOCK_COMMAND => copy_block(&request.input),
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
    if engine.is_none() {
        *engine = Some(load_engine()?);
    }
    let result = engine
        .as_mut()
        .expect("engine was initialized")
        .detect_image_bytes(&image, false)
        .map_err(|_| {
            CommandFailure::new(
                CapabilityErrorCode::RuntimeFault,
                "OCR inference failed",
                true,
            )
        })?;
    let attachment = overlay::build_attachment_payload(&result, true);
    let prior_revision =
        current_attachment(&request.unit_attachments).map_or(0, |value| value.revision);
    let full_text = attachment.full_text.clone();
    let mut effects = vec![upsert_effect(prior_revision, &attachment)];
    if !full_text.is_empty() {
        effects.extend(copy_effects(&full_text, "OCR 全文已复制"));
    } else {
        effects.push(notice_effect("OCR 未识别到文本"));
    }
    Ok(json!({
        "output": { "blockCount": attachment.text_blocks.len(), "textBytes": full_text.len() },
        "effects": effects,
    }))
}

fn toggle_overlay(attachments: &[ExtensionUnitAttachment]) -> Result<Value, CommandFailure> {
    let attachment = current_attachment(attachments).ok_or_else(missing_result)?;
    let mut payload: OcrAttachmentPayload =
        serde_json::from_value(attachment.payload.clone()).map_err(|_| missing_result())?;
    payload.visible = !payload.visible;
    payload.surface_scene["props"]["visible"] = json!(payload.visible);
    Ok(json!({
        "output": { "visible": payload.visible },
        "effects": [upsert_effect(attachment.revision, &payload)],
    }))
}

fn copy_full_text(attachments: &[ExtensionUnitAttachment]) -> Result<Value, CommandFailure> {
    let attachment = current_attachment(attachments).ok_or_else(missing_result)?;
    let payload: OcrAttachmentPayload =
        serde_json::from_value(attachment.payload.clone()).map_err(|_| missing_result())?;
    if payload.full_text.is_empty() {
        return Err(missing_result());
    }
    Ok(json!({ "effects": copy_effects(&payload.full_text, "OCR 全文已复制") }))
}

fn copy_block(input: &Value) -> Result<Value, CommandFailure> {
    let text = input
        .get("surfaceEvent")
        .and_then(|value| value.get("payload"))
        .and_then(|value| value.get("text"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_COPY_BYTES)
        .ok_or_else(|| {
            CommandFailure::new(
                CapabilityErrorCode::InvalidInput,
                "OCR text block is invalid",
                false,
            )
        })?;
    Ok(json!({ "effects": copy_effects(text, "OCR 文本已复制") }))
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
    let executable = std::env::current_exe().map_err(|_| model_unavailable())?;
    let model_dir = executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("resources/ocr");
    let models = OcrModelSet::from_dir(&model_dir).map_err(|_| model_unavailable())?;
    OcrEngine::new(models).map_err(|_| model_unavailable())
}

fn current_attachment(attachments: &[ExtensionUnitAttachment]) -> Option<&ExtensionUnitAttachment> {
    attachments.iter().find(|attachment| {
        attachment.plugin_id == PLUGIN_ID && attachment.attachment_id == RESULT_ATTACHMENT_ID
    })
}

fn upsert_effect(revision: u64, payload: &OcrAttachmentPayload) -> Value {
    json!({
        "type": "attachment.upsert",
        "payload": {
            "attachmentId": RESULT_ATTACHMENT_ID,
            "typeId": RESULT_TYPE_ID,
            "schemaVersion": "1",
            "priorRevision": revision,
            "revision": revision + 1,
            "rendererId": RESULT_RENDERER_ID,
            "payload": payload,
            "resourceRefs": [],
        }
    })
}

fn copy_effects(text: &str, notice: &str) -> Vec<Value> {
    vec![
        json!({ "type": "clipboard.writeText", "payload": { "text": text } }),
        notice_effect(notice),
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
