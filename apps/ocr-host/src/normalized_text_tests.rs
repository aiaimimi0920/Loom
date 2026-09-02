use loom_ocr::{EnhancedTextBlock, OcrDetectResult, OcrPoint};

use crate::overlay::build_attachment_payload;

#[test]
fn attachment_keeps_normalized_text_distinct_from_corrected_raw_text() {
    let result = OcrDetectResult {
        text_blocks: vec![fixture_block()],
        scale_factor: 1.0,
        full_text: "OCR 文本".to_owned(),
        width: 100,
        height: 100,
    };
    let payload = build_attachment_payload(&result, true);
    let block = &payload.text_blocks[0];
    assert_eq!(block.text, "OCR 文本");
    assert_eq!(block.normalized_text.as_deref(), Some("OCR 文本"));
    assert_eq!(block.raw_text.as_deref(), Some("0CR 文本"));
    assert_eq!(
        block.confidence_source.as_deref(),
        Some("modelTextScoreMean")
    );
}

fn fixture_block() -> EnhancedTextBlock {
    EnhancedTextBlock {
        box_points: vec![
            OcrPoint { x: 10, y: 20 },
            OcrPoint { x: 40, y: 20 },
            OcrPoint { x: 40, y: 40 },
            OcrPoint { x: 10, y: 40 },
        ],
        box_score: 0.99,
        text: "OCR 文本".to_owned(),
        text_score: 0.99,
        color_hex: "#ffffff".to_owned(),
        bg_color_hex: "#101010".to_owned(),
        raw_text: Some("0CR 文本".to_owned()),
        confidence: None,
        line_geometry: None,
        character_spans: Vec::new(),
        word_spans: Vec::new(),
    }
}
