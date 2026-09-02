// Merges detector fragments that occupy one evidenced visual row.

use crate::confidence;
use crate::geometry::{block_bounds, estimate_line_geometry, Bounds};
use crate::types::{EnhancedTextBlock, OcrPoint, OcrTextSpan};

const MAX_FRAGMENT_BLOCKS: usize = 512;
const MAX_CENTER_DISTANCE_RATIO: f32 = 0.5;
const MAX_GAP_HEIGHT_RATIO: f32 = 2.4;

pub(crate) fn merge_line_fragments(
    blocks: Vec<EnhancedTextBlock>,
    image_width: u32,
    image_height: u32,
) -> Vec<EnhancedTextBlock> {
    let mut source = blocks.into_iter();
    let mut merged: Vec<EnhancedTextBlock> = Vec::new();
    for candidate in source.by_ref().take(MAX_FRAGMENT_BLOCKS) {
        let Some(candidate_bounds) = block_bounds(&candidate.box_points, image_width, image_height)
        else {
            merged.push(candidate);
            continue;
        };
        let merge_index = merged
            .iter()
            .enumerate()
            .filter_map(|(index, existing)| {
                let bounds = block_bounds(&existing.box_points, image_width, image_height)?;
                can_merge(existing, bounds, &candidate, candidate_bounds).then_some((
                    index,
                    center_y(bounds).abs_diff(center_y(candidate_bounds)),
                    horizontal_gap(bounds, candidate_bounds),
                    bounds.height().abs_diff(candidate_bounds.height()),
                ))
            })
            .min_by_key(|(_, center_distance, gap, height_delta)| {
                (*center_distance, *gap, *height_delta)
            })
            .map(|(index, ..)| index);
        if let Some(index) = merge_index {
            merge_pair(&mut merged[index], candidate, candidate_bounds);
        } else {
            merged.push(candidate);
        }
    }
    merged.extend(source);
    merged
}

fn can_merge(
    left: &EnhancedTextBlock,
    left_bounds: Bounds,
    right: &EnhancedTextBlock,
    right_bounds: Bounds,
) -> bool {
    let minimum_height = left_bounds.height().min(right_bounds.height()) as f32;
    let maximum_height = left_bounds.height().max(right_bounds.height()) as f32;
    let center_distance = center_y(left_bounds).abs_diff(center_y(right_bounds)) as f32;
    if center_distance > minimum_height * MAX_CENTER_DISTANCE_RATIO
        || maximum_height > minimum_height * 2.0
        || line_angle(left) > 10.0
        || line_angle(right) > 10.0
    {
        return false;
    }
    let overlap = overlap_width(left_bounds, right_bounds);
    let minimum_width = left_bounds.width().min(right_bounds.width());
    if minimum_width > 0 && overlap as f32 / minimum_width as f32 >= 0.75 {
        return false;
    }
    horizontal_gap(left_bounds, right_bounds)
        <= (minimum_height * MAX_GAP_HEIGHT_RATIO).max(8.0) as u32
}

fn merge_pair(first: &mut EnhancedTextBlock, second: EnhancedTextBlock, second_bounds: Bounds) {
    let first_bounds = raw_bounds(&first.box_points);
    let second_is_left = second_bounds.min_x < first_bounds.min_x;
    let first_text = first.text.clone();
    let second_text = second.text.clone();
    let (left_text, right_text) = if second_is_left {
        (&second_text, &first_text)
    } else {
        (&first_text, &second_text)
    };
    let (text, inserted_space) = join_fragments(left_text, right_text);
    let raw_text = if first.raw_text.is_some() || second.raw_text.is_some() {
        let first_raw = first.raw_text.as_deref().unwrap_or(&first_text);
        let second_raw = second.raw_text.as_deref().unwrap_or(&second_text);
        let (left_raw, right_raw) = if second_is_left {
            (second_raw, first_raw)
        } else {
            (first_raw, second_raw)
        };
        Some(join_fragments(left_raw, right_raw).0)
    } else {
        None
    };
    let bounds = union(first_bounds, second_bounds);
    if second_is_left {
        first.color_hex.clone_from(&second.color_hex);
        first.bg_color_hex.clone_from(&second.bg_color_hex);
    }
    first.box_score = first.box_score.min(second.box_score);
    first.text_score = first.text_score.min(second.text_score);
    first.text = text;
    first.raw_text = raw_text;
    first.confidence = confidence::merge(first.confidence.take(), second.confidence);
    first.box_points = rectangle(bounds);
    first.line_geometry = estimate_line_geometry(&first.box_points);
    merge_spans(
        &mut first.character_spans,
        second.character_spans,
        inserted_space,
    );
    merge_spans(&mut first.word_spans, second.word_spans, inserted_space);
}

fn join_fragments(left: &str, right: &str) -> (String, bool) {
    let left = left.trim_end();
    let right = right.trim_start();
    let left_char = left.chars().next_back();
    let right_char = right.chars().next();
    let standalone_marker = matches!(left.trim(), "-" | "*" | "\u{2022}" | "#" | "##" | "###");
    let adjacent = !standalone_marker
        && (left_char.is_some_and(is_cjk)
            || right_char.is_some_and(is_cjk)
            || left_char.is_some_and(|value| matches!(value, '-' | '/' | '(' | '['))
            || right_char
                .is_some_and(|value| matches!(value, ',' | '.' | ';' | ':' | '!' | '?' | ')')));
    let separator = if adjacent { "" } else { " " };
    (format!("{left}{separator}{right}"), !adjacent)
}

fn merge_spans(target: &mut Vec<OcrTextSpan>, mut source: Vec<OcrTextSpan>, inserted_space: bool) {
    if inserted_space {
        target.clear();
        return;
    }
    target.append(&mut source);
    target.sort_by(|left, right| span_left(left).total_cmp(&span_left(right)));
}

fn span_left(span: &OcrTextSpan) -> f32 {
    span.box_points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min)
}

fn line_angle(block: &EnhancedTextBlock) -> f32 {
    block
        .line_geometry
        .as_ref()
        .map_or(0.0, |line| line.angle_degrees.abs())
}

const fn center_y(bounds: Bounds) -> u32 {
    bounds.min_y + bounds.max_y.saturating_sub(bounds.min_y) / 2
}

fn overlap_width(left: Bounds, right: Bounds) -> u32 {
    left.max_x
        .min(right.max_x)
        .saturating_sub(left.min_x.max(right.min_x))
}

fn horizontal_gap(left: Bounds, right: Bounds) -> u32 {
    left.min_x
        .max(right.min_x)
        .saturating_sub(left.max_x.min(right.max_x))
}

fn raw_bounds(points: &[OcrPoint]) -> Bounds {
    Bounds {
        min_x: points.iter().map(|point| point.x).min().unwrap_or(0),
        max_x: points.iter().map(|point| point.x).max().unwrap_or(0),
        min_y: points.iter().map(|point| point.y).min().unwrap_or(0),
        max_y: points.iter().map(|point| point.y).max().unwrap_or(0),
    }
}

const fn union(left: Bounds, right: Bounds) -> Bounds {
    Bounds {
        min_x: if left.min_x < right.min_x {
            left.min_x
        } else {
            right.min_x
        },
        max_x: if left.max_x > right.max_x {
            left.max_x
        } else {
            right.max_x
        },
        min_y: if left.min_y < right.min_y {
            left.min_y
        } else {
            right.min_y
        },
        max_y: if left.max_y > right.max_y {
            left.max_y
        } else {
            right.max_y
        },
    }
}

fn rectangle(bounds: Bounds) -> Vec<OcrPoint> {
    vec![
        OcrPoint {
            x: bounds.min_x,
            y: bounds.min_y,
        },
        OcrPoint {
            x: bounds.max_x,
            y: bounds.min_y,
        },
        OcrPoint {
            x: bounds.max_x,
            y: bounds.max_y,
        },
        OcrPoint {
            x: bounds.min_x,
            y: bounds.max_y,
        },
    ]
}

fn is_cjk(value: char) -> bool {
    matches!(value, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}
