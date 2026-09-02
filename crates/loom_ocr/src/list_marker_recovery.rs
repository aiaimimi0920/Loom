// Recovers a visually evidenced list dash that the line detector left outside its box.

use image::RgbImage;

use crate::geometry::{block_bounds, estimate_line_geometry, Bounds};
use crate::types::EnhancedTextBlock;

const MAX_RECOVERY_BLOCKS: usize = 512;
const MAX_LINE_HEIGHT: u32 = 256;
const MAX_SEARCH_WIDTH: u32 = 128;
const MIN_LUMINANCE_DELTA: i16 = 48;

pub(crate) fn recover_leading_list_markers(image: &RgbImage, blocks: &mut [EnhancedTextBlock]) {
    if image.width() == 0 || image.height() == 0 {
        return;
    }
    let detected_bounds = blocks
        .iter()
        .take(MAX_RECOVERY_BLOCKS)
        .map(|block| block_bounds(&block.box_points, image.width(), image.height()))
        .collect::<Vec<_>>();
    let explicit_markers = blocks
        .iter()
        .take(MAX_RECOVERY_BLOCKS)
        .filter(|block| is_marker_only(&block.text))
        .filter_map(|block| block_bounds(&block.box_points, image.width(), image.height()))
        .collect::<Vec<_>>();
    for (index, block) in blocks.iter_mut().take(MAX_RECOVERY_BLOCKS).enumerate() {
        if starts_with_marker(&block.text) {
            continue;
        }
        let Some(bounds) = block_bounds(&block.box_points, image.width(), image.height()) else {
            continue;
        };
        if bounds.height() < 6
            || bounds.height() > MAX_LINE_HEIGHT
            || block
                .line_geometry
                .as_ref()
                .is_some_and(|line| line.angle_degrees.abs() > 10.0)
            || explicit_markers
                .iter()
                .any(|marker| marker_precedes_line(*marker, bounds))
            || detected_bounds
                .iter()
                .enumerate()
                .any(|(other_index, other)| {
                    other_index != index
                        && other.is_some_and(|other| detected_fragment_precedes_line(other, bounds))
                })
        {
            continue;
        }
        let Some(marker_left) = find_horizontal_marker(image, bounds, &block.bg_color_hex) else {
            continue;
        };
        let recognized = block.text.trim_start().to_owned();
        if block.raw_text.is_none() {
            block.raw_text = Some(recognized.clone());
        }
        block.text = format!("- {recognized}");
        extend_quad_left(&mut block.box_points, marker_left);
        block.line_geometry = estimate_line_geometry(&block.box_points);
        // Existing CTC spans do not describe the pixel-evidenced marker. Omit
        // them rather than falsely labelling the new prefix as model-aligned.
        block.character_spans.clear();
        block.word_spans.clear();
    }
}

fn detected_fragment_precedes_line(fragment: Bounds, line: Bounds) -> bool {
    if fragment.min_x >= line.min_x {
        return false;
    }
    let fragment_center = (fragment.min_y + fragment.max_y) / 2;
    let line_center = (line.min_y + line.max_y) / 2;
    let center_limit = fragment.height().min(line.height()) / 2;
    fragment_center.abs_diff(line_center) <= center_limit
}

fn starts_with_marker(text: &str) -> bool {
    text.trim_start().chars().next().is_some_and(|character| {
        matches!(character, '-' | '\u{2013}' | '\u{2014}' | '\u{2022}' | '*')
    })
}

fn is_marker_only(text: &str) -> bool {
    matches!(
        text.trim(),
        "-" | "\u{2013}" | "\u{2014}" | "\u{2022}" | "*"
    )
}

fn marker_precedes_line(marker: Bounds, line: Bounds) -> bool {
    if marker.max_x >= line.min_x {
        return false;
    }
    let line_height = line.height();
    let marker_center = (marker.min_y + marker.max_y) / 2;
    let line_center = (line.min_y + line.max_y) / 2;
    marker_center.abs_diff(line_center) <= line_height / 2
        && line.min_x.saturating_sub(marker.max_x) <= line_height
}

fn find_horizontal_marker(image: &RgbImage, bounds: Bounds, background: &str) -> Option<u32> {
    if bounds.min_x == 0 {
        return None;
    }
    let line_height = bounds.height();
    let search_width = (line_height * 3 / 2).clamp(8, MAX_SEARCH_WIDTH);
    let left = bounds.min_x.saturating_sub(search_width);
    let right = bounds.min_x - 1;
    let top = bounds.min_y + line_height * 3 / 10;
    let bottom = (bounds.min_y + line_height * 7 / 10).min(image.height() - 1);
    let minimum_width = (line_height / 6).max(2);
    let maximum_width = (line_height * 3 / 4).max(minimum_width);
    let maximum_gap = (line_height * 3 / 4).max(3);
    let background_luminance = parse_luminance(background)?;
    let mut best: Option<(u32, u32, u32)> = None;

    for y in top..=bottom {
        let mut x = left;
        while x <= right {
            if !contrasts(image.get_pixel(x, y).0, background_luminance) {
                x += 1;
                continue;
            }
            let start = x;
            while x < right && contrasts(image.get_pixel(x + 1, y).0, background_luminance) {
                x += 1;
            }
            let width = x - start + 1;
            let gap = bounds.min_x.saturating_sub(x + 1);
            if width >= minimum_width && width <= maximum_width && gap <= maximum_gap {
                let candidate = (gap, u32::MAX - width, start);
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
            x += 1;
        }
    }
    best.map(|candidate| candidate.2)
}

fn contrasts(pixel: [u8; 3], background: i16) -> bool {
    (luminance(pixel) - background).abs() >= MIN_LUMINANCE_DELTA
}

fn parse_luminance(value: &str) -> Option<i16> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(luminance([
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ]))
}

fn luminance([red, green, blue]: [u8; 3]) -> i16 {
    (u32::from(red) * 54 / 255 + u32::from(green) * 183 / 255 + u32::from(blue) * 19 / 255) as i16
}

fn extend_quad_left(points: &mut [crate::types::OcrPoint], marker_left: u32) {
    let mut indices = (0..points.len()).collect::<Vec<_>>();
    indices.sort_by_key(|index| (points[*index].x, points[*index].y));
    for index in indices.into_iter().take(2) {
        points[index].x = marker_left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OcrPoint, OcrTextSpan};

    #[test]
    fn recovers_a_pixel_evidenced_dash_immediately_left_of_the_line() {
        let mut image = RgbImage::from_pixel(120, 50, image::Rgb([16, 16, 16]));
        draw_horizontal(&mut image, 18, 24, 24);
        let mut blocks = vec![block("保留现有发布信任密钥", 32, 12, 108, 38)];
        blocks[0].character_spans = vec![placeholder_span()];

        recover_leading_list_markers(&image, &mut blocks);

        assert_eq!(blocks[0].text, "- 保留现有发布信任密钥");
        assert_eq!(blocks[0].raw_text.as_deref(), Some("保留现有发布信任密钥"));
        assert_eq!(
            blocks[0].box_points.iter().map(|point| point.x).min(),
            Some(18)
        );
        assert!(blocks[0].character_spans.is_empty());
    }

    #[test]
    fn rejects_vertical_or_distant_strokes_and_never_duplicates_a_marker() {
        let mut vertical = RgbImage::from_pixel(120, 50, image::Rgb([16, 16, 16]));
        for y in 18..=30 {
            vertical.put_pixel(24, y, image::Rgb([245, 245, 245]));
        }
        let mut blocks = vec![block("正文", 32, 12, 108, 38)];
        recover_leading_list_markers(&vertical, &mut blocks);
        assert_eq!(blocks[0].text, "正文");

        let mut distant = RgbImage::from_pixel(120, 50, image::Rgb([16, 16, 16]));
        draw_horizontal(&mut distant, 0, 6, 24);
        recover_leading_list_markers(&distant, &mut blocks);
        assert_eq!(blocks[0].text, "正文");

        let mut marked = vec![block("- 正文", 32, 12, 108, 38)];
        let mut nearby = RgbImage::from_pixel(120, 50, image::Rgb([16, 16, 16]));
        draw_horizontal(&mut nearby, 18, 24, 24);
        recover_leading_list_markers(&nearby, &mut marked);
        assert_eq!(marked[0].text, "- 正文");
    }

    #[test]
    fn does_not_treat_a_stroke_behind_an_existing_row_fragment_as_a_list_marker() {
        let mut image = RgbImage::from_pixel(140, 50, image::Rgb([16, 16, 16]));
        draw_horizontal(&mut image, 18, 24, 24);
        let mut blocks = vec![block("##", 10, 12, 28, 38), block("标题", 32, 12, 108, 38)];

        recover_leading_list_markers(&image, &mut blocks);

        assert_eq!(blocks[1].text, "标题");
    }

    fn draw_horizontal(image: &mut RgbImage, left: u32, right: u32, y: u32) {
        for x in left..=right {
            image.put_pixel(x, y, image::Rgb([245, 245, 245]));
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
            color_hex: "#f7f8ef".to_owned(),
            bg_color_hex: "#101010".to_owned(),
            raw_text: None,
            line_geometry: estimate_line_geometry(&[
                OcrPoint { x: left, y: top },
                OcrPoint { x: right, y: top },
                OcrPoint {
                    x: right,
                    y: bottom,
                },
                OcrPoint { x: left, y: bottom },
            ]),
            character_spans: Vec::new(),
            word_spans: Vec::new(),
        }
    }

    fn placeholder_span() -> OcrTextSpan {
        use crate::types::{OcrMetricPoint, OcrTextSpanSource};
        OcrTextSpan {
            text: "保".to_owned(),
            box_points: [OcrMetricPoint { x: 32.0, y: 12.0 }; 4],
            score: 0.99,
            source: OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
        }
    }
}
