use loom_ocr::{EnhancedTextBlock, OcrDetectResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::overlay_color::shared_fill_color;

const MAX_BLOCKS: usize = 128;
const MAX_BLOCK_TEXT_BYTES: usize = 1024;
const MAX_FULL_TEXT_BYTES: usize = 96 * 1024;
const MAX_ATTACHMENT_BYTES: usize = 240 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrAttachmentPayload {
    pub schema_version: String,
    pub visible: bool,
    pub source_width: u32,
    pub source_height: u32,
    pub full_text: String,
    pub text_blocks: Vec<OcrAttachmentBlock>,
    pub surface_scene: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrAttachmentBlock {
    pub text: String,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub text_color: String,
    pub background_color: String,
}

pub fn build_attachment_payload(result: &OcrDetectResult, visible: bool) -> OcrAttachmentPayload {
    let source_width = result.width.max(1);
    let source_height = result.height.max(1);
    let fill_color = shared_fill_color(&result.text_blocks);
    let mut text_blocks = result
        .text_blocks
        .iter()
        .filter_map(|block| normalize_block(block, source_width, source_height, &fill_color))
        .take(MAX_BLOCKS)
        .collect::<Vec<_>>();
    let mut full_text = truncate_utf8(&result.full_text, MAX_FULL_TEXT_BYTES);
    loop {
        let payload = OcrAttachmentPayload {
            schema_version: "1".to_owned(),
            visible,
            source_width,
            source_height,
            full_text: full_text.clone(),
            surface_scene: scene(&text_blocks, source_width, source_height, visible),
            text_blocks: text_blocks.clone(),
        };
        if serde_json::to_vec(&payload).is_ok_and(|bytes| bytes.len() <= MAX_ATTACHMENT_BYTES) {
            return payload;
        }
        if text_blocks.pop().is_none() {
            let next_limit = full_text.len().saturating_sub(4096);
            full_text = truncate_utf8(&full_text, next_limit);
        }
    }
}

fn normalize_block(
    block: &EnhancedTextBlock,
    source_width: u32,
    source_height: u32,
    fill_color: &str,
) -> Option<OcrAttachmentBlock> {
    if block.text.trim().is_empty() || block.box_points.is_empty() {
        return None;
    }
    let min_x = block.box_points.iter().map(|point| point.x).min()?;
    let max_x = block.box_points.iter().map(|point| point.x).max()?;
    let min_y = block.box_points.iter().map(|point| point.y).min()?;
    let max_y = block.box_points.iter().map(|point| point.y).max()?;
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    let left = min_x.min(source_width) as f32;
    let top = min_y.min(source_height) as f32;
    let right = max_x.min(source_width) as f32;
    let bottom = max_y.min(source_height) as f32;
    if right <= left || bottom <= top {
        return None;
    }
    Some(OcrAttachmentBlock {
        text: truncate_utf8(&block.text, MAX_BLOCK_TEXT_BYTES),
        left,
        top,
        width: right - left,
        height: bottom - top,
        text_color: safe_color(&block.color_hex, "#f8fafc"),
        background_color: fill_color.to_owned(),
    })
}

fn scene(
    blocks: &[OcrAttachmentBlock],
    source_width: u32,
    source_height: u32,
    visible: bool,
) -> Value {
    let children = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let font_size = (block.height / source_height as f32 * 82.0).clamp(0.2, 100.0);
            let line_height = (block.height / source_height as f32 * 100.0).clamp(0.2, 100.0);
            json!({
                "id": format!("ocr-block-{index}"),
                "type": "stack",
                "props": { "eventPayload": { "text": block.text } },
                "layout": {
                    "position": "absolute",
                    "left": percent(block.left, source_width),
                    "top": percent(block.top, source_height),
                    "width": percent(block.width, source_width),
                    "height": percent(block.height, source_height),
                    "overflowX": "hidden",
                    "overflowY": "hidden"
                },
                "style": { "background": block.background_color },
                "events": { "click": "neuro.official/ocr.copy-block" },
                "children": [{
                    "id": format!("ocr-text-{index}"),
                    "type": "text",
                    "props": { "text": block.text },
                    "layout": { "width": "100%", "height": "100%" },
                    "style": {
                        "color": block.text_color,
                        "fontSize": format!("{font_size:.4}cqh"),
                        "lineHeight": format!("{line_height:.4}cqh"),
                        "whiteSpace": "nowrap"
                    }
                }]
            })
        })
        .collect::<Vec<_>>();
    json!({
        "id": "ocr-overlay-root",
        "type": "stack",
        "props": { "visible": visible },
        "layout": {
            "position": "relative",
            "width": "100%",
            "height": "100%",
            "overflowX": "hidden",
            "overflowY": "hidden"
        },
        "children": children
    })
}

fn percent(value: f32, maximum: u32) -> String {
    format!(
        "{:.5}%",
        (value / maximum.max(1) as f32 * 100.0).clamp(0.0, 100.0)
    )
}

fn safe_color(value: &str, fallback: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        value.to_owned()
    } else {
        fallback.to_owned()
    }
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let mut end = maximum_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_owned()
}
