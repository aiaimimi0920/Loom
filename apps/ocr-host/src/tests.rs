use loom_ocr::{EnhancedTextBlock, OcrDetectResult, OcrPoint};
use loom_protocol::ExtensionUnitAttachment;
use serde_json::{json, Value};

use crate::code_overlay;
use crate::code_scan::{CodeBounds, CodePoint, CodeResult, CodeScanResult};
use crate::commands;
use crate::overlay::build_attachment_payload;

#[test]
fn attachment_scene_preserves_source_geometry_and_uses_one_opaque_fill() {
    let payload = build_attachment_payload(&fixture_result(), true);
    let children = payload.surface_scene["children"].as_array().unwrap();
    assert_eq!(children[0]["layout"]["left"], "10.00000%");
    assert_eq!(children[0]["layout"]["top"], "20.00000%");
    assert_eq!(children[0]["layout"]["width"], "30.00000%");
    assert_eq!(children[0]["layout"]["height"], "20.00000%");
    let font_size = children[0]["children"][0]["style"]["fontSize"]
        .as_str()
        .unwrap();
    assert!(font_size.starts_with("min(") && font_size.ends_with("cqw)"));
    assert_eq!(children[0]["children"][0]["props"]["selectable"], true);
    assert!(children[0]["layout"].get("overflowX").is_none());
    assert!(children[0]["layout"].get("overflowY").is_none());
    let fill = payload.text_blocks[0].background_color.clone();
    assert!(fill.starts_with('#') && fill.len() == 7);
    assert!(payload
        .text_blocks
        .iter()
        .all(|block| block.background_color == fill));
    assert_ne!(fill, "#ffffff");
    assert_ne!(fill, "#101010");
    assert!(!is_red_like(&fill));
}

#[test]
fn attachment_payload_remains_inside_the_host_budget() {
    let block = fixture_block("中".repeat(4096));
    let result = OcrDetectResult {
        text_blocks: vec![block; 256],
        scale_factor: 1.0,
        full_text: "文".repeat(200_000),
        width: 100,
        height: 100,
    };
    let payload = build_attachment_payload(&result, true);
    assert!(payload.text_blocks.len() <= 128);
    assert!(serde_json::to_vec(&payload).unwrap().len() <= 240 * 1024);
}

#[test]
fn attachment_geometry_accounts_for_ocr_preprocessing_scale() {
    let mut result = fixture_result();
    result.scale_factor = 2.0;
    let payload = build_attachment_payload(&result, true);
    let first = &payload.text_blocks[0];

    assert_eq!(payload.coordinate_scale, 2.0);
    assert_eq!(first.left, 5.0);
    assert_eq!(first.top, 10.0);
    assert_eq!(
        payload.surface_scene["children"][0]["layout"]["left"],
        "5.00000%"
    );
    assert_eq!(first.box_points, fixture_result().text_blocks[0].box_points);
}

#[test]
fn cached_and_clicked_text_commands_return_only_brokered_effects() {
    let payload = build_attachment_payload(&fixture_result(), true);
    let mut payload_value = serde_json::to_value(&payload).unwrap();
    payload_value["migration"] = json!({ "source": "hook.unitData.ocrResult", "version": 1 });
    payload_value["textBlocks"][0]["rawText"] = json!("OCR raw text");
    let attachment = ExtensionUnitAttachment {
        attachment_id: commands::RESULT_ATTACHMENT_ID.to_owned(),
        type_id: commands::RESULT_TYPE_ID.to_owned(),
        schema_version: "1".to_owned(),
        revision: 4,
        plugin_id: commands::PLUGIN_ID.to_owned(),
        plugin_version: "1.0.0".to_owned(),
        renderer_id: Some(commands::RESULT_RENDERER_ID.to_owned()),
        payload: payload_value,
        resource_refs: Vec::new(),
    };
    let copy = execute(
        "neuro.official/ocr.copy-full-text",
        json!({}),
        vec![attachment.clone()],
    );
    assert_eq!(effect_types(&copy), ["clipboard.writeText", "notice.show"]);
    assert_eq!(copy["effects"][0]["payload"]["text"], payload.full_text);

    let block = execute(
        "neuro.official/ocr.copy-block",
        json!({ "surfaceEvent": { "payload": { "text": "单块" } } }),
        Vec::new(),
    );
    assert_eq!(block["effects"][0]["payload"]["text"], "单块");

    let toggle = execute(
        "neuro.official/ocr.toggle-overlay",
        json!({}),
        vec![attachment],
    );
    assert_eq!(toggle["output"]["visible"], false);
    assert_eq!(toggle["effects"][0]["payload"]["priorRevision"], 4);
    assert_eq!(toggle["effects"][0]["payload"]["revision"], 5);
    assert_eq!(
        toggle["effects"][0]["payload"]["payload"]["surfaceScene"]["props"]["visible"],
        false
    );
    assert_eq!(
        toggle["effects"][0]["payload"]["payload"]["migration"]["source"],
        "hook.unitData.ocrResult"
    );
    assert_eq!(
        toggle["effects"][0]["payload"]["payload"]["textBlocks"][0]["rawText"],
        "OCR raw text"
    );
}

#[test]
fn code_attachment_uses_generic_scene_and_brokered_copy() {
    let payload = code_fixture_payload();
    let node = &payload.surface_scene["children"][0];
    assert_eq!(node["layout"]["left"], "17.50000%");
    assert_eq!(node["layout"]["top"], "15.00000%");
    assert_eq!(node["events"]["click"], "neuro.official/ocr.copy-code");
    assert_eq!(node["props"]["eventPayload"]["operation"], "select");
    assert_eq!(node["type"], "button");
    assert_eq!(node["style"]["background"], "#06b6d4");
    assert_eq!(node["style"]["borderRadius"], "999px");
    assert!(payload.selected_id.is_none());

    let selected = execute(
        "neuro.official/ocr.copy-code",
        json!({ "surfaceEvent": { "payload": {
            "operation": "select",
            "resultId": "code-1",
        } } }),
        vec![code_attachment(&payload, 2)],
    );
    assert_eq!(effect_types(&selected), ["attachment.upsert"]);
    let selected_payload = &selected["effects"][0]["payload"]["payload"];
    assert_eq!(selected_payload["selectedId"], "code-1");
    let popup = selected_payload["surfaceScene"]["children"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(popup["id"], "ocr-code-action-popup");
    assert_eq!(popup["type"], "row");
    assert_eq!(popup["layout"]["width"], "45%");
    assert_eq!(popup["layout"]["minWidth"], "0");
    assert_eq!(
        popup["layout"]["left"],
        "min(calc(17.50000% + 30px), calc(100% - min(45%, 280px)))"
    );
    assert_eq!(
        popup["layout"]["top"],
        "min(calc(15.00000% + 30px), calc(100% - min(70%, 90px)))"
    );
    assert_eq!(popup["layout"]["height"], "70%");
    assert_eq!(popup["layout"]["maxHeight"], "90px");
    assert_eq!(popup["children"][0]["type"], "textarea");
    assert_eq!(popup["children"][0]["props"]["rows"], 3);
    assert_eq!(popup["children"][0]["layout"]["overflowY"], "auto");
    assert_eq!(popup["children"][0]["layout"]["grow"], 1);
    assert_eq!(popup["children"][0]["layout"]["height"], "auto");
    assert_eq!(popup["children"][1]["type"], "column");
    assert_eq!(popup["children"][1]["layout"]["width"], "34px");
    let action_operations = popup["children"][1]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|button| {
            button["props"]["eventPayload"]["operation"]
                .as_str()
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(action_operations, ["copy", "open"]);
    assert_eq!(
        selected_payload["surfaceScene"]["children"][0]["props"]["eventPayload"]["operation"],
        "dismiss"
    );
    assert_eq!(
        selected_payload["surfaceScene"]["children"][0]["props"]["hostPointerPassthrough"],
        true
    );
    assert_eq!(
        selected_payload["surfaceScene"]["children"][1]["props"]["eventPayload"]["operation"],
        "dismiss"
    );

    let selected_attachment = ExtensionUnitAttachment {
        revision: 3,
        payload: selected_payload.clone(),
        ..code_attachment(&payload, 2)
    };

    let copy = execute(
        "neuro.official/ocr.copy-code",
        json!({ "surfaceEvent": { "payload": {
            "operation": "copy",
            "resultId": "code-1",
            "surfaceValues": {
                "ocr-code-action-editor": "https://example.com/edited-copy"
            }
        } } }),
        vec![selected_attachment.clone()],
    );
    assert_eq!(effect_types(&copy), ["clipboard.writeText", "notice.show"]);
    assert_eq!(
        copy["effects"][0]["payload"]["text"],
        "https://example.com/edited-copy"
    );

    let open = execute(
        "neuro.official/ocr.copy-code",
        json!({ "surfaceEvent": { "payload": {
            "operation": "open",
            "resultId": "code-1",
            "surfaceValues": {
                "ocr-code-action-editor": " https://example.com/edited-open "
            }
        } } }),
        vec![selected_attachment.clone()],
    );
    assert_eq!(effect_types(&open), ["external.openUrl"]);
    assert_eq!(
        open["effects"][0]["payload"]["url"],
        "https://example.com/edited-open"
    );

    let unsafe_open = commands::execute(
        json!({
            "commandId": "neuro.official/ocr.copy-code",
            "input": { "surfaceEvent": { "payload": {
                "operation": "open",
                "resultId": "code-1",
                "surfaceValues": { "ocr-code-action-editor": "javascript:alert(1)" }
            } } },
            "unitAttachments": [selected_attachment],
            "stagedResources": [],
        }),
        &mut None,
    );
    assert!(unsafe_open.is_err());
}

#[test]
fn code_popup_clamps_inside_the_bottom_right_surface_edge() {
    let payload = code_overlay::build_attachment_payload(&CodeScanResult {
        width: 1000,
        height: 500,
        results: vec![CodeResult {
            id: "edge-code".to_owned(),
            format: "QR_CODE".to_owned(),
            text: "https://example.com/edge".to_owned(),
            url: Some("https://example.com/edge".to_owned()),
            points: vec![CodePoint { x: 900.0, y: 450.0 }],
            bounds: None,
        }],
    });
    let selected = execute(
        "neuro.official/ocr.copy-code",
        json!({ "surfaceEvent": { "payload": {
            "operation": "select",
            "resultId": "edge-code",
        } } }),
        vec![code_attachment(&payload, 1)],
    );
    let popup = selected["effects"][0]["payload"]["payload"]["surfaceScene"]["children"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(
        popup["layout"]["left"],
        "min(calc(88.50000% + 30px), calc(100% - min(45%, 280px)))"
    );
    assert_eq!(
        popup["layout"]["top"],
        "min(calc(87.00000% + 30px), calc(100% - min(70%, 90px)))"
    );
    assert_eq!(popup["layout"]["width"], "45%");
    assert_eq!(popup["layout"]["height"], "70%");
}

#[test]
fn alt_two_toggles_text_and_code_scenes_as_one_overlay() {
    let text_payload = build_attachment_payload(&fixture_result(), true);
    let text = ExtensionUnitAttachment {
        attachment_id: commands::RESULT_ATTACHMENT_ID.to_owned(),
        type_id: commands::RESULT_TYPE_ID.to_owned(),
        schema_version: "1".to_owned(),
        revision: 4,
        plugin_id: commands::PLUGIN_ID.to_owned(),
        plugin_version: "1.2.8".to_owned(),
        renderer_id: Some(commands::RESULT_RENDERER_ID.to_owned()),
        payload: serde_json::to_value(text_payload).unwrap(),
        resource_refs: Vec::new(),
    };
    let codes = code_attachment(&code_fixture_payload(), 7);

    let hidden = execute(
        "neuro.official/ocr.toggle-overlay",
        json!({}),
        vec![text.clone(), codes.clone()],
    );
    assert_eq!(hidden["output"]["visible"], false);
    assert_eq!(
        effect_types(&hidden),
        ["attachment.upsert", "attachment.upsert"]
    );
    assert!(hidden["effects"].as_array().unwrap().iter().all(|effect| {
        effect["payload"]["payload"]["visible"] == false
            && effect["payload"]["payload"]["surfaceScene"]["props"]["visible"] == false
    }));

    let restored = execute(
        "neuro.official/ocr.toggle-overlay",
        json!({}),
        vec![
            ExtensionUnitAttachment {
                revision: 5,
                payload: hidden["effects"][0]["payload"]["payload"].clone(),
                ..text
            },
            ExtensionUnitAttachment {
                revision: 8,
                payload: hidden["effects"][1]["payload"]["payload"].clone(),
                ..codes
            },
        ],
    );
    assert_eq!(restored["output"]["visible"], true);
    assert!(restored["effects"]
        .as_array()
        .unwrap()
        .iter()
        .all(|effect| {
            effect["payload"]["payload"]["visible"] == true
                && effect["payload"]["payload"]["surfaceScene"]["props"]["visible"] == true
        }));
}

#[test]
fn migrated_translation_fields_are_preserved_and_drive_copy_when_selected() {
    let mut payload = build_attachment_payload(&fixture_result(), true);
    payload.show_translated = true;
    payload.text_blocks[0].translated_text = Some("first line".to_owned());
    payload.text_blocks[1].translated_text = Some("second line".to_owned());
    let attachment = ExtensionUnitAttachment {
        attachment_id: commands::RESULT_ATTACHMENT_ID.to_owned(),
        type_id: commands::RESULT_TYPE_ID.to_owned(),
        schema_version: "1".to_owned(),
        revision: 1,
        plugin_id: commands::PLUGIN_ID.to_owned(),
        plugin_version: "1.0.0".to_owned(),
        renderer_id: Some(commands::RESULT_RENDERER_ID.to_owned()),
        payload: serde_json::to_value(payload).unwrap(),
        resource_refs: Vec::new(),
    };

    let copy = execute(
        "neuro.official/ocr.copy-full-text",
        json!({}),
        vec![attachment],
    );
    assert_eq!(
        copy["effects"][0]["payload"]["text"],
        "first line\nsecond line"
    );
}

fn execute(command_id: &str, input: Value, attachments: Vec<ExtensionUnitAttachment>) -> Value {
    commands::execute(
        json!({
            "commandId": command_id,
            "input": input,
            "unitAttachments": attachments,
            "stagedResources": [],
        }),
        &mut None,
    )
    .unwrap()
}

fn effect_types(value: &Value) -> Vec<&str> {
    value["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| effect["type"].as_str().unwrap())
        .collect()
}

fn code_fixture_payload() -> code_overlay::CodeAttachmentPayload {
    code_overlay::build_attachment_payload(&CodeScanResult {
        width: 200,
        height: 100,
        results: vec![CodeResult {
            id: "code-1".to_owned(),
            format: "QR_CODE".to_owned(),
            text: "https://example.com/hook".to_owned(),
            url: Some("https://example.com/hook".to_owned()),
            points: vec![
                CodePoint { x: 20.0, y: 10.0 },
                CodePoint { x: 80.0, y: 10.0 },
                CodePoint { x: 80.0, y: 50.0 },
                CodePoint { x: 20.0, y: 50.0 },
            ],
            bounds: Some(CodeBounds {
                left: 20.0,
                top: 10.0,
                right: 80.0,
                bottom: 50.0,
            }),
        }],
    })
}

fn code_attachment(
    payload: &code_overlay::CodeAttachmentPayload,
    revision: u64,
) -> ExtensionUnitAttachment {
    ExtensionUnitAttachment {
        attachment_id: commands::CODES_ATTACHMENT_ID.to_owned(),
        type_id: commands::CODES_TYPE_ID.to_owned(),
        schema_version: "1".to_owned(),
        revision,
        plugin_id: commands::PLUGIN_ID.to_owned(),
        plugin_version: "1.2.8".to_owned(),
        renderer_id: Some(commands::CODES_RENDERER_ID.to_owned()),
        payload: serde_json::to_value(payload).unwrap(),
        resource_refs: Vec::new(),
    }
}

fn is_red_like(hex: &str) -> bool {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
    let red = ((value >> 16) & 0xff) as f32 / 255.0;
    let green = ((value >> 8) & 0xff) as f32 / 255.0;
    let blue = (value & 0xff) as f32 / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let delta = maximum - minimum;
    if delta <= f32::EPSILON {
        return false;
    }
    let hue = if maximum == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    hue <= 30.0 || hue >= 300.0
}

fn fixture_result() -> OcrDetectResult {
    OcrDetectResult {
        text_blocks: vec![
            fixture_block("OCR 文本".to_owned()),
            fixture_block("第二行".to_owned()),
        ],
        scale_factor: 1.0,
        full_text: "OCR 文本\n第二行".to_owned(),
        width: 100,
        height: 100,
    }
}

fn fixture_block(text: String) -> EnhancedTextBlock {
    EnhancedTextBlock {
        box_points: vec![
            OcrPoint { x: 10, y: 20 },
            OcrPoint { x: 40, y: 20 },
            OcrPoint { x: 40, y: 40 },
            OcrPoint { x: 10, y: 40 },
        ],
        box_score: 0.99,
        text,
        text_score: 0.99,
        color_hex: "#ffffff".to_owned(),
        bg_color_hex: "#101010".to_owned(),
        raw_text: None,
        confidence: None,
        line_geometry: None,
        character_spans: Vec::new(),
        word_spans: Vec::new(),
    }
}
