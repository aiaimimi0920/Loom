use crate::geometry::{block_bounds, Bounds};
use crate::types::EnhancedTextBlock;

const SAME_LINE_CENTER_RATIO: f32 = 0.55;

/// Applies a stable visual order after detector fragments have been merged.
/// Blocks on one evidenced row are ordered left-to-right; distinct rows use
/// their top edge, preserving the detector's stable order for exact ties.
pub(crate) fn sort_blocks(blocks: &mut [EnhancedTextBlock], width: u32, height: u32) {
    blocks.sort_by(|left, right| compare_blocks(left, right, width, height));
}

fn compare_blocks(
    left: &EnhancedTextBlock,
    right: &EnhancedTextBlock,
    width: u32,
    height: u32,
) -> std::cmp::Ordering {
    let Some(left_bounds) = block_bounds(&left.box_points, width, height) else {
        return std::cmp::Ordering::Greater;
    };
    let Some(right_bounds) = block_bounds(&right.box_points, width, height) else {
        return std::cmp::Ordering::Less;
    };
    if same_visual_line(left_bounds, right_bounds) {
        return left_bounds.min_x.cmp(&right_bounds.min_x);
    }
    left_bounds
        .min_y
        .cmp(&right_bounds.min_y)
        .then_with(|| left_bounds.min_x.cmp(&right_bounds.min_x))
}

fn same_visual_line(left: Bounds, right: Bounds) -> bool {
    let minimum_height = left.height().min(right.height()) as f32;
    let center_distance = center_y(left).abs_diff(center_y(right)) as f32;
    center_distance <= minimum_height * SAME_LINE_CENTER_RATIO
}

const fn center_y(bounds: Bounds) -> u32 {
    bounds.min_y + bounds.max_y.saturating_sub(bounds.min_y) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OcrPoint, OcrTextSpan};

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
            character_spans: Vec::<OcrTextSpan>::new(),
            word_spans: Vec::new(),
        }
    }

    #[test]
    fn orders_rows_top_to_bottom_and_fragments_left_to_right() {
        let mut blocks = vec![
            block("row2", 0, 40, 30, 50),
            block("row1-right", 40, 10, 70, 20),
            block("row1-left", 0, 11, 30, 21),
        ];
        sort_blocks(&mut blocks, 100, 100);
        assert_eq!(
            blocks
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            ["row1-left", "row1-right", "row2"]
        );
    }

    #[test]
    fn keeps_narrow_punctuation_on_the_same_visual_row() {
        let mut blocks = vec![block("title", 10, 10, 50, 20), block("#", 0, 12, 1, 18)];
        sort_blocks(&mut blocks, 100, 100);
        assert_eq!(blocks[0].text, "#");
        assert_eq!(blocks[1].text, "title");
    }
}
