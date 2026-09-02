//! Removes OCR text that belongs to an independently decoded code region.

use loom_ocr::{EnhancedTextBlock, OcrDetectResult};

use crate::code_scan::{CodeBounds, CodeScanResult};

const CODE_PADDING_RATIO: f32 = 0.22;

pub fn suppress_code_text(mut ocr: OcrDetectResult, codes: &CodeScanResult) -> OcrDetectResult {
    let exclusions = exclusions(codes, ocr.width, ocr.height);
    if exclusions.is_empty() {
        return ocr;
    }

    let original_count = ocr.text_blocks.len();
    let coordinate_scale = normalized_scale(ocr.scale_factor);
    ocr.text_blocks
        .retain(|block| !belongs_to_code(block, coordinate_scale, &exclusions));
    if ocr.text_blocks.len() != original_count {
        ocr.full_text = ocr
            .text_blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    ocr
}

fn exclusions(codes: &CodeScanResult, target_width: u32, target_height: u32) -> Vec<CodeBounds> {
    let scale_x = target_width.max(1) as f32 / codes.width.max(1) as f32;
    let scale_y = target_height.max(1) as f32 / codes.height.max(1) as f32;
    codes
        .results
        .iter()
        .filter_map(|result| result.bounds.as_ref())
        .filter_map(|bounds| expanded_bounds(bounds, scale_x, scale_y, target_width, target_height))
        .collect()
}

fn expanded_bounds(
    bounds: &CodeBounds,
    scale_x: f32,
    scale_y: f32,
    target_width: u32,
    target_height: u32,
) -> Option<CodeBounds> {
    let left = bounds.left * scale_x;
    let top = bounds.top * scale_y;
    let right = bounds.right * scale_x;
    let bottom = bounds.bottom * scale_y;
    if ![left, top, right, bottom]
        .iter()
        .all(|value| value.is_finite())
        || right <= left
        || bottom <= top
    {
        return None;
    }
    // Decoder points often sit at QR finder-pattern centers, not the outer edge.
    let padding = ((right - left).max(bottom - top) * CODE_PADDING_RATIO).max(2.0);
    Some(CodeBounds {
        left: (left - padding).clamp(0.0, target_width.max(1) as f32),
        top: (top - padding).clamp(0.0, target_height.max(1) as f32),
        right: (right + padding).clamp(0.0, target_width.max(1) as f32),
        bottom: (bottom + padding).clamp(0.0, target_height.max(1) as f32),
    })
}

fn belongs_to_code(block: &EnhancedTextBlock, coordinate_scale: f32, codes: &[CodeBounds]) -> bool {
    let Some(bounds) = block_bounds(block, coordinate_scale) else {
        return false;
    };
    let center_x = (bounds.left + bounds.right) / 2.0;
    let center_y = (bounds.top + bounds.bottom) / 2.0;
    codes.iter().any(|code| {
        center_x >= code.left
            && center_x <= code.right
            && center_y >= code.top
            && center_y <= code.bottom
    })
}

fn block_bounds(block: &EnhancedTextBlock, scale: f32) -> Option<CodeBounds> {
    let first = block.box_points.first()?;
    let mut bounds = CodeBounds {
        left: first.x as f32 / scale,
        top: first.y as f32 / scale,
        right: first.x as f32 / scale,
        bottom: first.y as f32 / scale,
    };
    for point in block.box_points.iter().skip(1) {
        let x = point.x as f32 / scale;
        let y = point.y as f32 / scale;
        bounds.left = bounds.left.min(x);
        bounds.top = bounds.top.min(y);
        bounds.right = bounds.right.max(x);
        bounds.bottom = bounds.bottom.max(y);
    }
    (bounds.right > bounds.left && bounds.bottom > bounds.top).then_some(bounds)
}

fn normalized_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 && scale <= 1000.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use loom_ocr::{EnhancedTextBlock, OcrDetectResult, OcrPoint};

    use super::suppress_code_text;
    use crate::code_scan::{CodeBounds, CodeResult, CodeScanResult};

    #[test]
    fn removes_only_text_centered_in_a_decoded_code() {
        let ocr = result(vec![
            block("QR noise", 28, 16, 72, 44),
            block("ordinary text", 110, 20, 190, 40),
        ]);

        let filtered = suppress_code_text(ocr, &scan());

        assert_eq!(filtered.full_text, "ordinary text");
        assert_eq!(filtered.text_blocks.len(), 1);
        assert_eq!(filtered.text_blocks[0].text, "ordinary text");
    }

    #[test]
    fn preserves_the_original_text_when_no_code_was_decoded() {
        let mut ocr = result(vec![block("ordinary text", 10, 20, 90, 40)]);
        ocr.full_text = "  ordinary text  ".to_owned();
        let empty = CodeScanResult {
            width: 200,
            height: 100,
            results: Vec::new(),
        };

        let filtered = suppress_code_text(ocr, &empty);

        assert_eq!(filtered.full_text, "  ordinary text  ");
    }

    fn result(text_blocks: Vec<EnhancedTextBlock>) -> OcrDetectResult {
        let full_text = text_blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        OcrDetectResult {
            text_blocks,
            scale_factor: 1.0,
            full_text,
            width: 200,
            height: 100,
        }
    }

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
            box_score: 0.99,
            text: text.to_owned(),
            text_score: 0.99,
            color_hex: "#ffffff".to_owned(),
            bg_color_hex: "#000000".to_owned(),
            raw_text: None,
            line_geometry: None,
            character_spans: Vec::new(),
            word_spans: Vec::new(),
        }
    }

    fn scan() -> CodeScanResult {
        CodeScanResult {
            width: 200,
            height: 100,
            results: vec![CodeResult {
                id: "code-1".to_owned(),
                format: "QR_CODE".to_owned(),
                text: "https://example.com".to_owned(),
                url: Some("https://example.com".to_owned()),
                points: Vec::new(),
                bounds: Some(CodeBounds {
                    left: 20.0,
                    top: 10.0,
                    right: 80.0,
                    bottom: 50.0,
                }),
            }],
        }
    }
}
