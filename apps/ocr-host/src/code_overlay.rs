//! Converts decoded codes into one bounded, generic attachment scene for Hook.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::code_scan::{CodeResult, CodeScanResult};

const MAX_ATTACHMENT_BYTES: usize = 240 * 1024;
const COPY_CODE_COMMAND: &str = "neuro.official/ocr.copy-code";
const CODE_MARKER_SIZE_PX: u32 = 30;
pub const CODE_EDITOR_NODE_ID: &str = "ocr-code-action-editor";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeAttachmentPayload {
    pub schema_version: String,
    pub visible: bool,
    pub source_width: u32,
    pub source_height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<String>,
    pub results: Vec<CodeResult>,
    pub surface_scene: Value,
}

pub fn build_attachment_payload(scan: &CodeScanResult) -> CodeAttachmentPayload {
    let source_width = scan.width.max(1);
    let source_height = scan.height.max(1);
    let mut results = scan.results.clone();
    loop {
        let payload = CodeAttachmentPayload {
            schema_version: "1".to_owned(),
            visible: true,
            source_width,
            source_height,
            selected_id: None,
            surface_scene: scene(&results, source_width, source_height, true, None),
            results: results.clone(),
        };
        if serde_json::to_vec(&payload).is_ok_and(|bytes| bytes.len() <= MAX_ATTACHMENT_BYTES) {
            return payload;
        }
        if results.pop().is_none() {
            return payload;
        }
    }
}

/// Updates only the scene selection while preserving unknown migration fields.
pub fn set_selection(payload: &mut Value, selected_id: Option<&str>) -> bool {
    let Ok(parsed) = serde_json::from_value::<CodeAttachmentPayload>(payload.clone()) else {
        return false;
    };
    if selected_id.is_some_and(|id| !parsed.results.iter().any(|result| result.id == id)) {
        return false;
    }
    let Some(object) = payload.as_object_mut() else {
        return false;
    };
    if let Some(id) = selected_id {
        object.insert("selectedId".to_owned(), json!(id));
    } else {
        object.remove("selectedId");
    }
    object.insert(
        "surfaceScene".to_owned(),
        scene(
            &parsed.results,
            parsed.source_width,
            parsed.source_height,
            parsed.visible,
            selected_id,
        ),
    );
    true
}

fn scene(
    results: &[CodeResult],
    source_width: u32,
    source_height: u32,
    visible: bool,
    selected_id: Option<&str>,
) -> Value {
    let mut children = Vec::new();
    if selected_id.is_some() {
        children.push(json!({
            "id": "ocr-code-action-backdrop",
            "type": "stack",
            "props": {
                "hostPointerPassthrough": true,
                "eventPayload": { "operation": "dismiss" }
            },
            "layout": {
                "position": "absolute",
                "left": "0",
                "top": "0",
                "width": "100%",
                "height": "100%",
            },
            "accessibility": { "label": "关闭二维码或条码操作面板" },
            "events": { "click": COPY_CODE_COMMAND }
        }));
    }
    children.extend(results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let (center_x, center_y) = result_center(result, source_width, source_height);
            let marker_size = CODE_MARKER_SIZE_PX as f32;
            let left = (center_x - marker_size / 2.0)
                .clamp(0.0, (source_width as f32 - marker_size).max(0.0));
            let top = (center_y - marker_size / 2.0)
                .clamp(0.0, (source_height as f32 - marker_size).max(0.0));
            json!({
                "id": format!("ocr-code-marker-{index}"),
                "type": "button",
                "props": {
                    "label": format!("{}", index + 1),
                    "eventPayload": {
                        "operation": if selected_id == Some(result.id.as_str()) { "dismiss" } else { "select" },
                        "resultId": result.id,
                    }
                },
                "layout": {
                    "position": "absolute",
                    "left": percent(left, source_width),
                    "top": percent(top, source_height),
                    "width": format!("{CODE_MARKER_SIZE_PX}px"),
                    "height": format!("{CODE_MARKER_SIZE_PX}px"),
                    "minHeight": format!("{CODE_MARKER_SIZE_PX}px"),
                    "padding": "0px",
                    "align": "center",
                    "justify": "center",
                },
                "style": {
                    "background": if selected_id == Some(result.id.as_str()) { "#d9ff38" } else { "#06b6d4" },
                    "color": "#06080d",
                    "borderColor": if selected_id == Some(result.id.as_str()) { "#f7f8ef" } else { "#d9ff38" },
                    "borderWidth": "2px",
                    "borderRadius": "999px",
                    "fontSize": "11px",
                    "lineHeight": "16px",
                    "fontWeight": 800,
                    "textAlign": "center",
                },
                "accessibility": {
                    "label": format!("打开第 {} 个二维码或条码操作", index + 1),
                    "description": result.format,
                },
                "events": { "click": COPY_CODE_COMMAND }
            })
        }));
    if let Some(result) = selected_id.and_then(|id| results.iter().find(|result| result.id == id)) {
        children.push(action_popup(result, source_width, source_height));
    }
    json!({
        "id": "ocr-code-overlay-root",
        "type": "stack",
        "props": { "visible": visible },
        "layout": {
            "position": "relative",
            "width": "100%",
            "height": "100%",
            "overflowX": "hidden",
            "overflowY": "hidden",
        },
        "children": children,
    })
}

fn action_popup(result: &CodeResult, source_width: u32, source_height: u32) -> Value {
    let (center_x, center_y) = result_center(result, source_width, source_height);
    let width = source_width.max(1) as f32;
    let height = source_height.max(1) as f32;
    let marker_size = CODE_MARKER_SIZE_PX as f32;
    let marker_left = (center_x - marker_size / 2.0).clamp(0.0, (width - marker_size).max(0.0));
    let marker_top = (center_y - marker_size / 2.0).clamp(0.0, (height - marker_size).max(0.0));
    // The marker's rendered bottom-right corner is the preferred panel anchor. Shift only
    // an overflowing axis using the rendered CSS size, not source-image pixels.
    let left = bounded_position_after_marker(marker_left, source_width, 45, 280);
    let top = bounded_position_after_marker(marker_top, source_height, 70, 90);
    let mut actions = vec![action_button(result, "copy", "⧉", "复制编辑后的内容")];
    if result.url.is_some() {
        actions.push(action_button(
            result,
            "open",
            "↗",
            "在浏览器中打开编辑后的链接",
        ));
    }
    json!({
        "id": "ocr-code-action-popup",
        "type": "row",
        "layout": {
            "position": "absolute",
            "left": left,
            "top": top,
            "width": "45%",
            "minWidth": "0",
            "maxWidth": "280px",
            "height": "70%",
            "maxHeight": "90px",
            "padding": "8px",
            "gap": "6px",
            "align": "stretch",
        },
        "style": {
            "background": "#0e1218",
            "color": "#f7f8ef",
            "borderColor": "#d9ff38",
            "borderWidth": "1px",
            "borderRadius": "7px",
        },
        "children": [{
            "id": CODE_EDITOR_NODE_ID,
            "type": "textarea",
            "props": {
                "value": result.text,
                "rows": 3,
                "maxLength": 16384,
                "placeholder": "二维码或条码内容"
            },
            "layout": {
                "width": "auto",
                "minWidth": "0",
                "height": "auto",
                "grow": 1,
                "overflowY": "auto"
            },
            "style": {
                "background": "#111720",
                "color": "#f7f8ef",
                "borderColor": "#06b6d4",
                "borderWidth": "1px",
                "borderRadius": "6px",
                "fontSize": "12px",
                "lineHeight": "16px",
                "whiteSpace": "pre-wrap"
            },
            "accessibility": { "label": "可编辑的二维码或条码内容" }
        }, {
            "id": "ocr-code-action-buttons",
            "type": "column",
            "layout": {
                "width": "34px",
                "minWidth": "34px",
                "gap": "6px",
                "justify": "flex-start"
            },
            "children": actions,
        }]
    })
}

fn action_button(result: &CodeResult, operation: &str, glyph: &str, label: &str) -> Value {
    json!({
        "id": format!("ocr-code-action-{operation}"),
        "type": "button",
        "props": {
            "label": glyph,
            "includeSurfaceValues": true,
            "eventPayload": { "operation": operation, "resultId": result.id }
        },
        "layout": {
            "width": "34px",
            "height": "34px",
            "minHeight": "34px",
            "padding": "0px",
            "align": "center",
            "justify": "center"
        },
        "style": {
            "background": if operation == "open" { "#d9ff38" } else { "#111720" },
            "color": if operation == "open" { "#06080d" } else { "#f7f8ef" },
            "borderColor": if operation == "open" { "#d9ff38" } else { "#313943" },
            "borderWidth": "1px",
            "borderRadius": "6px",
            "fontSize": "18px",
            "lineHeight": "20px",
            "fontWeight": 700,
        },
        "accessibility": { "label": label },
        "events": { "click": COPY_CODE_COMMAND },
    })
}

fn result_center(result: &CodeResult, source_width: u32, source_height: u32) -> (f32, f32) {
    let width = source_width.max(1) as f32;
    let height = source_height.max(1) as f32;
    let finite = result
        .points
        .iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .collect::<Vec<_>>();
    if !finite.is_empty() {
        let count = finite.len() as f32;
        let x = finite.iter().map(|point| point.x).sum::<f32>() / count;
        let y = finite.iter().map(|point| point.y).sum::<f32>() / count;
        return (x.clamp(0.0, width), y.clamp(0.0, height));
    }
    result
        .bounds
        .as_ref()
        .map(|bounds| {
            (
                ((bounds.left + bounds.right) / 2.0).clamp(0.0, width),
                ((bounds.top + bounds.bottom) / 2.0).clamp(0.0, height),
            )
        })
        .unwrap_or((width / 2.0, height / 2.0))
}

fn percent(value: f32, maximum: u32) -> String {
    format!(
        "{:.5}%",
        (value / maximum.max(1) as f32 * 100.0).clamp(0.0, 100.0)
    )
}

fn bounded_position_after_marker(
    value: f32,
    maximum: u32,
    relative_size: u32,
    pixel_cap: u32,
) -> String {
    let anchor = (value / maximum.max(1) as f32 * 100.0).clamp(0.0, 100.0);
    format!(
        "min(calc({anchor:.5}% + {CODE_MARKER_SIZE_PX}px), calc(100% - min({relative_size}%, {pixel_cap}px)))"
    )
}
