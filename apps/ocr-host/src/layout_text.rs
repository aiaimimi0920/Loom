use std::cmp::Ordering;

use crate::overlay::OcrAttachmentBlock;

const MAX_HORIZONTAL_SPACES: usize = 64;
const MAX_LINE_BREAKS: usize = 4;
const MAX_LAYOUT_BLOCKS: usize = 128;
const MAX_BLOCK_TEXT_BYTES: usize = 1024;
const MAX_LAYOUT_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
struct LayoutBlock {
    text: String,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

#[derive(Debug)]
struct LayoutRow {
    top: f32,
    bottom: f32,
    blocks: Vec<LayoutBlock>,
}

pub(crate) fn compose(blocks: &[OcrAttachmentBlock], translated: bool) -> Option<String> {
    if blocks.len() > MAX_LAYOUT_BLOCKS {
        return None;
    }
    let mut layout_blocks = Vec::with_capacity(blocks.len());
    for block in blocks {
        let text = if translated {
            block.translated_text.as_deref().unwrap_or(&block.text)
        } else {
            &block.text
        };
        if text.len() > MAX_BLOCK_TEXT_BYTES {
            return None;
        }
        if let Some(block) = valid_block(text, block.left, block.top, block.width, block.height) {
            layout_blocks.push(block);
        }
    }
    let output = compose_blocks(layout_blocks);
    (output.len() <= MAX_LAYOUT_TEXT_BYTES).then_some(output)
}

fn compose_blocks(mut blocks: Vec<LayoutBlock>) -> String {
    if blocks.is_empty() {
        return String::new();
    }
    blocks.sort_by(visual_order);
    let glyph_width = median(blocks.iter().filter_map(block_glyph_width)).unwrap_or(8.0);
    let row_height = median(blocks.iter().map(|block| block.height)).unwrap_or(16.0);
    let minimum_left = blocks
        .iter()
        .map(|block| block.left)
        .fold(f32::INFINITY, f32::min);
    let mut rows: Vec<LayoutRow> = Vec::new();
    for block in blocks {
        match rows
            .iter_mut()
            .rev()
            .find(|row| belongs_to_row(row, &block))
        {
            Some(row) => {
                row.top = row.top.min(block.top);
                row.bottom = row.bottom.max(block.top + block.height);
                row.blocks.push(block);
            }
            None => rows.push(LayoutRow {
                top: block.top,
                bottom: block.top + block.height,
                blocks: vec![block],
            }),
        }
    }
    rows.sort_by(|left, right| float_order(left.top, right.top));

    let mut output = String::new();
    let mut previous_center = None;
    for row in &mut rows {
        row.blocks
            .sort_by(|left, right| float_order(left.left, right.left));
        let center = (row.top + row.bottom) / 2.0;
        if let Some(previous) = previous_center {
            output.push_str(&"\n".repeat(line_breaks(center - previous, row_height)));
        }
        previous_center = Some(center);
        append_row(&mut output, row, minimum_left, glyph_width);
    }
    output.trim_end().to_owned()
}

fn append_row(output: &mut String, row: &LayoutRow, minimum_left: f32, glyph_width: f32) {
    let first = &row.blocks[0];
    output.push_str(&" ".repeat(space_count(first.left - minimum_left, glyph_width, 0)));
    let mut previous_right = first.left;
    for (index, block) in row.blocks.iter().enumerate() {
        if index > 0
            && !output.ends_with(char::is_whitespace)
            && !block.text.starts_with(char::is_whitespace)
        {
            output.push_str(&" ".repeat(space_count(block.left - previous_right, glyph_width, 1)));
        }
        output.push_str(&block.text);
        previous_right = previous_right.max(block.left + block.width);
    }
}

fn valid_block(text: &str, left: f32, top: f32, width: f32, height: f32) -> Option<LayoutBlock> {
    if text.trim().is_empty()
        || ![left, top, width, height]
            .iter()
            .all(|value| value.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }
    Some(LayoutBlock {
        text: text.to_owned(),
        left,
        top,
        width,
        height,
    })
}

fn belongs_to_row(row: &LayoutRow, block: &LayoutBlock) -> bool {
    let overlap = row.bottom.min(block.top + block.height) - row.top.max(block.top);
    let smaller_height = (row.bottom - row.top).min(block.height);
    overlap >= smaller_height * 0.35
}

fn block_glyph_width(block: &LayoutBlock) -> Option<f32> {
    let count = block
        .text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    (count > 0).then(|| block.width / count as f32)
}

fn space_count(distance: f32, glyph_width: f32, minimum: usize) -> usize {
    if !distance.is_finite() || distance <= 0.0 {
        return minimum;
    }
    ((distance / glyph_width.max(1.0)).round() as usize)
        .max(minimum)
        .min(MAX_HORIZONTAL_SPACES)
}

fn line_breaks(distance: f32, row_height: f32) -> usize {
    ((distance / row_height.max(1.0)).round() as usize).clamp(1, MAX_LINE_BREAKS)
}

fn median(values: impl Iterator<Item = f32>) -> Option<f32> {
    let mut values = values
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    values.sort_by(|left, right| float_order(*left, *right));
    values.get(values.len() / 2).copied()
}

fn visual_order(left: &LayoutBlock, right: &LayoutBlock) -> Ordering {
    float_order(left.top, right.top).then_with(|| float_order(left.left, right.left))
}

fn float_order(left: f32, right: f32) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_relative_indentation_gaps_and_paragraph_spacing() {
        let blocks = vec![
            valid_block("alpha", 10.0, 0.0, 50.0, 10.0).unwrap(),
            valid_block("beta", 80.0, 0.0, 40.0, 10.0).unwrap(),
            valid_block("nested", 30.0, 12.0, 60.0, 10.0).unwrap(),
            valid_block("paragraph", 10.0, 32.0, 90.0, 10.0).unwrap(),
        ];
        assert_eq!(compose_blocks(blocks), "alpha  beta\n  nested\n\nparagraph");
    }

    #[test]
    fn ignores_invalid_blocks_and_caps_extreme_whitespace() {
        let blocks = vec![
            valid_block("left", 0.0, 0.0, 4.0, 10.0).unwrap(),
            valid_block("right", 100_000.0, 0.0, 5.0, 10.0).unwrap(),
        ];
        let result = compose_blocks(blocks);
        assert!(result.starts_with("left"));
        assert!(result.ends_with("right"));
        assert!(result.len() <= "left".len() + MAX_HORIZONTAL_SPACES + "right".len());
    }
}
