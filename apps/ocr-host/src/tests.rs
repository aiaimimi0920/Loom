use std::fs;
use std::path::PathBuf;

use loom_ocr::{EnhancedTextBlock, OcrDetectResult, OcrPoint};
use loom_protocol::{parse_capability_manifest, ExtensionUnitAttachment};
use serde_json::{json, Value};

use crate::commands;
use crate::overlay::build_attachment_payload;

#[test]
fn official_manifest_declares_the_complete_ocr_surface() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../capability-packages/ocr/capability.manifest.json");
    let manifest = parse_capability_manifest(&fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest.qualified_id(), "neuro.official/ocr");
    assert_eq!(manifest.contributes.commands.len(), 4);
    assert_eq!(manifest.contributes.shortcuts.len(), 2);
    assert_eq!(manifest.contributes.menus.len(), 1);
    assert_eq!(manifest.contributes.data_types.len(), 1);
    assert_eq!(manifest.contributes.renderers.len(), 1);
    assert!(manifest
        .permissions
        .contains(&"hook.unit.image.read".to_owned()));
}

#[test]
fn attachment_scene_preserves_source_geometry_and_uses_one_opaque_fill() {
    let payload = build_attachment_payload(&fixture_result(), true);
    let children = payload.surface_scene["children"].as_array().unwrap();
    assert_eq!(children[0]["layout"]["left"], "10.00000%");
    assert_eq!(children[0]["layout"]["top"], "20.00000%");
    assert_eq!(children[0]["layout"]["width"], "30.00000%");
    assert_eq!(children[0]["layout"]["height"], "20.00000%");
    assert!(children[0]["children"][0]["style"]["fontSize"]
        .as_str()
        .unwrap()
        .ends_with("cqh"));
    let fill = payload.text_blocks[0].background_color.clone();
    assert!(fill.starts_with('#') && fill.len() == 7);
    assert!(payload
        .text_blocks
        .iter()
        .all(|block| block.background_color == fill));
    assert_ne!(fill, "#ffffff");
    assert_ne!(fill, "#101010");
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
        line_geometry: None,
        character_spans: Vec::new(),
        word_spans: Vec::new(),
    }
}
