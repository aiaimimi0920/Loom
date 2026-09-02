use crate::geometry::estimate_line_geometry;
use crate::line_fragment_merge::merge_line_fragments;
use crate::types::{EnhancedTextBlock, OcrPoint};

#[test]
fn merges_same_row_fragments_in_visual_order() {
    let blocks = vec![
        block("我执行的运行时操作", 36, 8, 255, 33),
        block("##", 22, 11, 62, 31),
    ];
    let merged = merge_line_fragments(blocks, 881, 320);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].text, "## 我执行的运行时操作");
    assert_eq!(minimum_point(&merged[0].box_points), (22, 8));
    assert_eq!(maximum_point(&merged[0].box_points), (255, 33));
}

#[test]
fn preserves_separate_rows() {
    let blocks = vec![
        block("第一行", 20, 10, 120, 30),
        block("第二行", 20, 38, 120, 58),
    ];
    assert_eq!(merge_line_fragments(blocks, 200, 100).len(), 2);
}

fn block(text: &str, left: u32, top: u32, right: u32, bottom: u32) -> EnhancedTextBlock {
    let points = vec![
        OcrPoint { x: left, y: top },
        OcrPoint { x: right, y: top },
        OcrPoint {
            x: right,
            y: bottom,
        },
        OcrPoint { x: left, y: bottom },
    ];
    EnhancedTextBlock {
        line_geometry: estimate_line_geometry(&points),
        box_points: points,
        box_score: 0.99,
        text: text.to_owned(),
        text_score: 0.98,
        color_hex: "#ffffff".to_owned(),
        bg_color_hex: "#101010".to_owned(),
        raw_text: None,
        confidence: None,
        character_spans: Vec::new(),
        word_spans: Vec::new(),
    }
}

fn minimum_point(points: &[OcrPoint]) -> (u32, u32) {
    (
        points.iter().map(|point| point.x).min().unwrap(),
        points.iter().map(|point| point.y).min().unwrap(),
    )
}

fn maximum_point(points: &[OcrPoint]) -> (u32, u32) {
    (
        points.iter().map(|point| point.x).max().unwrap(),
        points.iter().map(|point| point.y).max().unwrap(),
    )
}
