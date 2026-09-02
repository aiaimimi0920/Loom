use crate::geometry::block_bounds;
use crate::types::{OcrDetectResult, OcrPoint};
use serde::{Deserialize, Serialize};

const MAX_METRIC_TEXT_CHARS: usize = 4_096;
const MAX_EDIT_DISTANCE_CELLS: usize = 4_194_304;

/// A checked-in expected result for one representative OCR image.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenFixture {
    pub fixture: String,
    pub source_width: u32,
    pub source_height: u32,
    pub expected_full_text: String,
    pub expected_blocks: Vec<GoldenBlock>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenBlock {
    pub text: String,
    #[serde(default)]
    pub box_points: Option<Vec<OcrPoint>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrQualityMetrics {
    pub character_error_rate: Option<f32>,
    pub punctuation_recall: Option<f32>,
    pub block_box_iou: Option<f32>,
    pub reading_order_accuracy: Option<f32>,
    pub expected_block_count: usize,
    pub actual_block_count: usize,
    pub dimensions_match: bool,
}

/// Compares a bounded model result with a checked-in golden expectation.
pub fn evaluate_quality(expected: &GoldenFixture, actual: &OcrDetectResult) -> OcrQualityMetrics {
    let expected_texts: Vec<&str> = expected
        .expected_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect();
    let actual_texts: Vec<&str> = actual
        .text_blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect();

    OcrQualityMetrics {
        character_error_rate: bounded_cer(&expected.expected_full_text, &actual.full_text),
        punctuation_recall: punctuation_recall(&expected.expected_full_text, &actual.full_text),
        block_box_iou: block_box_iou(expected, actual),
        reading_order_accuracy: reading_order_accuracy(&expected_texts, &actual_texts),
        expected_block_count: expected.expected_blocks.len(),
        actual_block_count: actual.text_blocks.len(),
        dimensions_match: expected.source_width == actual.width
            && expected.source_height == actual.height,
    }
}

fn bounded_cer(expected: &str, actual: &str) -> Option<f32> {
    let expected = bounded_chars(expected)?;
    let actual = bounded_chars(actual)?;
    if expected.is_empty() {
        return Some(if actual.is_empty() { 0.0 } else { 1.0 });
    }
    Some(edit_distance(&expected, &actual)? as f32 / expected.len() as f32)
}

fn bounded_chars(text: &str) -> Option<Vec<char>> {
    let characters: Vec<char> = text.chars().take(MAX_METRIC_TEXT_CHARS + 1).collect();
    (characters.len() <= MAX_METRIC_TEXT_CHARS).then_some(characters)
}

fn edit_distance(expected: &[char], actual: &[char]) -> Option<usize> {
    if expected.len().checked_mul(actual.len())? > MAX_EDIT_DISTANCE_CELLS {
        return None;
    }
    let mut previous: Vec<usize> = (0..=actual.len()).collect();
    for (row, expected_char) in expected.iter().enumerate() {
        let mut current = vec![row + 1; actual.len() + 1];
        for (column, actual_char) in actual.iter().enumerate() {
            current[column + 1] = (previous[column] + usize::from(expected_char != actual_char))
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        previous = current;
    }
    Some(previous[actual.len()])
}

fn punctuation_recall(expected: &str, actual: &str) -> Option<f32> {
    let expected = punctuation_counts(expected);
    let actual = punctuation_counts(actual);
    let expected_total: usize = expected.values().sum();
    if expected_total == 0 {
        return None;
    }
    let matched: usize = expected
        .iter()
        .map(|(character, count)| (*count).min(actual.get(character).copied().unwrap_or(0)))
        .sum();
    Some(matched as f32 / expected_total as f32)
}

fn punctuation_counts(text: &str) -> std::collections::BTreeMap<char, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for character in text
        .chars()
        .filter(|character| is_ocr_punctuation(*character))
    {
        *counts.entry(character).or_default() += 1;
    }
    counts
}

fn is_ocr_punctuation(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '\u{2018}'
                | '\u{2019}'
                | '\u{201c}'
                | '\u{201d}'
                | '\u{3001}'
                | '\u{3002}'
                | '\u{3008}'
                | '\u{3009}'
                | '\u{300a}'
                | '\u{300b}'
                | '\u{300c}'
                | '\u{300d}'
                | '\u{3010}'
                | '\u{3011}'
                | '\u{ff01}'
                | '\u{ff0c}'
                | '\u{ff1a}'
                | '\u{ff1b}'
                | '\u{ff1f}'
        )
}

fn reading_order_accuracy(expected: &[&str], actual: &[&str]) -> Option<f32> {
    if expected.is_empty() {
        return None;
    }
    let mut used = vec![false; actual.len()];
    let mut matched_positions = Vec::new();
    for expected_text in expected {
        let position = actual
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .filter(|(_, text)| plausible_text_match(expected_text, text))
            .min_by_key(|(_, text)| text_distance(expected_text, text))
            .map(|(index, _)| index);
        if let Some(position) = position {
            used[position] = true;
            matched_positions.push(position);
        }
    }
    if matched_positions.len() < 2 {
        return Some(matched_positions.len() as f32 / expected.len() as f32);
    }
    let ordered_pairs = matched_positions
        .windows(2)
        .filter(|pair| pair[0] < pair[1])
        .count();
    Some(ordered_pairs as f32 / (expected.len() - 1) as f32)
}

fn plausible_text_match(expected: &str, actual: &str) -> bool {
    let distance = text_distance(expected, actual);
    if distance == usize::MAX {
        return false;
    }
    let expected_len = expected.chars().count().max(1);
    distance <= (expected_len / 4).max(1)
}

fn text_distance(expected: &str, actual: &str) -> usize {
    let Some(expected) = bounded_chars(expected) else {
        return usize::MAX;
    };
    let Some(actual) = bounded_chars(actual) else {
        return usize::MAX;
    };
    edit_distance(&expected, &actual).unwrap_or(usize::MAX)
}

fn block_box_iou(expected: &GoldenFixture, actual: &OcrDetectResult) -> Option<f32> {
    let expected_boxes: Vec<_> = expected
        .expected_blocks
        .iter()
        .filter_map(|block| {
            block.box_points.as_deref().and_then(|points| {
                block_bounds(points, expected.source_width, expected.source_height)
            })
        })
        .collect();
    if expected_boxes.len() != expected.expected_blocks.len() || actual.text_blocks.is_empty() {
        return None;
    }
    let actual_boxes: Vec<_> = actual
        .text_blocks
        .iter()
        .filter_map(|block| block_bounds(&block.box_points, actual.width, actual.height))
        .collect();
    if actual_boxes.len() != actual.text_blocks.len() {
        return None;
    }

    let mut used = vec![false; actual_boxes.len()];
    let mut total = 0.0;
    for expected_box in expected_boxes {
        let Some((index, score)) = actual_boxes
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .map(|(index, actual_box)| (index, bounds_iou(expected_box, *actual_box)))
            .max_by(|left, right| left.1.total_cmp(&right.1))
        else {
            continue;
        };
        used[index] = true;
        total += score;
    }
    Some(total / expected.expected_blocks.len() as f32)
}

fn bounds_iou(left: crate::geometry::Bounds, right: crate::geometry::Bounds) -> f32 {
    let intersection_left = left.min_x.max(right.min_x);
    let intersection_right = left.max_x.min(right.max_x);
    let intersection_top = left.min_y.max(right.min_y);
    let intersection_bottom = left.max_y.min(right.max_y);
    if intersection_left > intersection_right || intersection_top > intersection_bottom {
        return 0.0;
    }
    let intersection_width = intersection_right - intersection_left + 1;
    let intersection_height = intersection_bottom - intersection_top + 1;
    let intersection = intersection_width as f32 * intersection_height as f32;
    let union = left.width() as f32 * left.height() as f32
        + right.width() as f32 * right.height() as f32
        - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EnhancedTextBlock;

    fn block(text: &str, left: u32, top: u32, right: u32, bottom: u32) -> EnhancedTextBlock {
        EnhancedTextBlock {
            box_points: vec![
                OcrPoint { x: left, y: top },
                OcrPoint { x: right, y: top },
                OcrPoint {
                    x: right,
                    y: bottom,
                },
                OcrPoint { x: left, y: bottom },
            ],
            box_score: 1.0,
            text: text.to_owned(),
            text_score: 1.0,
            color_hex: "#ffffff".to_owned(),
            bg_color_hex: "#101010".to_owned(),
            raw_text: None,
            confidence: None,
            line_geometry: None,
            character_spans: Vec::new(),
            word_spans: Vec::new(),
        }
    }

    #[test]
    fn cer_uses_unicode_scalars_and_explicit_empty_reference_behavior() {
        assert_eq!(bounded_cer("你好", "你号"), Some(0.5));
        assert_eq!(bounded_cer("", ""), Some(0.0));
        assert_eq!(bounded_cer("", "x"), Some(1.0));
        assert_eq!(
            bounded_cer(&"x".repeat(MAX_METRIC_TEXT_CHARS + 1), "x"),
            None
        );
    }

    #[test]
    fn punctuation_recall_counts_repeated_symbols() {
        assert_eq!(punctuation_recall("#--。", "#-。"), Some(0.75));
        assert_eq!(punctuation_recall("plain", "plain"), None);
    }

    #[test]
    fn box_iou_is_one_for_identical_boxes() {
        let expected = GoldenFixture {
            fixture: "synthetic".to_owned(),
            source_width: 100,
            source_height: 100,
            expected_full_text: "one".to_owned(),
            expected_blocks: vec![GoldenBlock {
                text: "one".to_owned(),
                box_points: Some(vec![
                    OcrPoint { x: 10, y: 10 },
                    OcrPoint { x: 20, y: 10 },
                    OcrPoint { x: 20, y: 20 },
                    OcrPoint { x: 10, y: 20 },
                ]),
            }],
        };
        let actual = OcrDetectResult {
            text_blocks: vec![block("one", 10, 10, 20, 20)],
            scale_factor: 1.0,
            full_text: "one".to_owned(),
            width: 100,
            height: 100,
        };
        let metrics = evaluate_quality(&expected, &actual);
        assert_eq!(metrics.block_box_iou, Some(1.0));
        assert_eq!(metrics.reading_order_accuracy, Some(1.0));
    }

    #[test]
    fn box_iou_is_zero_for_disjoint_boxes() {
        let left = block_bounds(&[OcrPoint { x: 1, y: 1 }], 20, 20).expect("left");
        let right = block_bounds(&[OcrPoint { x: 10, y: 10 }], 20, 20).expect("right");
        assert_eq!(bounds_iou(left, right), 0.0);
    }

    #[test]
    fn reading_order_detects_reversed_blocks() {
        let expected = ["first", "second"];
        let actual = ["second", "first"];
        assert_eq!(reading_order_accuracy(&expected, &actual), Some(0.0));
    }

    #[test]
    fn checked_in_fixture_has_stable_shape() {
        let fixture: GoldenFixture = serde_json::from_str(include_str!(
            "../../../resources/ocr/fixtures/golden/test_1.json"
        ))
        .expect("golden fixture JSON");
        assert_eq!((fixture.source_width, fixture.source_height), (678, 108));
        assert_eq!(fixture.expected_blocks.len(), 2);
        assert!(fixture.expected_full_text.contains("paddle-ocr-rs"));
    }
}
