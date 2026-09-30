use crate::ctc_decode::{DecodedLine, DecodedSymbol};
use crate::types::{OcrMetricPoint, OcrPoint, OcrTextSpan, OcrTextSpanSource};

const MAX_CHARACTER_SPANS: usize = 256;
const MAX_WORD_SPANS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecognitionAxis {
    Horizontal,
    Vertical,
}

fn metric(point: OcrPoint) -> OcrMetricPoint {
    OcrMetricPoint {
        x: point.x as f32,
        y: point.y as f32,
    }
}

fn lerp(start: OcrMetricPoint, end: OcrMetricPoint, ratio: f32) -> OcrMetricPoint {
    OcrMetricPoint {
        x: start.x + (end.x - start.x) * ratio,
        y: start.y + (end.y - start.y) * ratio,
    }
}

fn projected_box(
    quad: [OcrMetricPoint; 4],
    start_ratio: f32,
    end_ratio: f32,
    axis: RecognitionAxis,
) -> [OcrMetricPoint; 4] {
    match axis {
        RecognitionAxis::Horizontal => [
            lerp(quad[0], quad[1], start_ratio),
            lerp(quad[0], quad[1], end_ratio),
            lerp(quad[3], quad[2], end_ratio),
            lerp(quad[3], quad[2], start_ratio),
        ],
        RecognitionAxis::Vertical => [
            lerp(quad[0], quad[3], start_ratio),
            lerp(quad[1], quad[2], start_ratio),
            lerp(quad[1], quad[2], end_ratio),
            lerp(quad[0], quad[3], end_ratio),
        ],
    }
}

fn span_from_symbols(
    symbols: &[DecodedSymbol],
    quad: [OcrMetricPoint; 4],
    timesteps: usize,
    axis: RecognitionAxis,
    reverse: bool,
) -> Option<OcrTextSpan> {
    let first = symbols.first()?;
    let last = symbols.last()?;
    if timesteps == 0 {
        return None;
    }
    let mut start_ratio = first.start_timestep as f32 / timesteps as f32;
    let mut end_ratio = last.end_timestep as f32 / timesteps as f32;
    if reverse {
        (start_ratio, end_ratio) = (1.0 - end_ratio, 1.0 - start_ratio);
    }
    let score = symbols.iter().map(|symbol| symbol.score).sum::<f32>() / symbols.len() as f32;
    Some(OcrTextSpan {
        text: symbols.iter().map(|symbol| symbol.text.as_str()).collect(),
        box_points: projected_box(quad, start_ratio, end_ratio, axis),
        score,
        source: OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
    })
}

fn is_ascii_word_symbol(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Projects recognizer timesteps into line geometry without claiming glyph detection.
pub(crate) fn project_text_spans(
    line: &DecodedLine,
    points: &[OcrPoint],
    axis: RecognitionAxis,
    reverse: bool,
) -> (Vec<OcrTextSpan>, Vec<OcrTextSpan>) {
    if points.len() != 4 || line.timestep_count == 0 {
        return (Vec::new(), Vec::new());
    }
    let quad = [
        metric(points[0]),
        metric(points[1]),
        metric(points[2]),
        metric(points[3]),
    ];
    let character_spans = line
        .symbols
        .iter()
        .take(MAX_CHARACTER_SPANS)
        .filter_map(|symbol| {
            span_from_symbols(
                std::slice::from_ref(symbol),
                quad,
                line.timestep_count,
                axis,
                reverse,
            )
        })
        .collect::<Vec<_>>();

    let mut word_spans = Vec::new();
    let mut cursor = 0;
    while cursor < line.symbols.len() && word_spans.len() < MAX_WORD_SPANS {
        if line.symbols[cursor].text.chars().all(char::is_whitespace) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        if is_ascii_word_symbol(&line.symbols[cursor].text) {
            cursor += 1;
            while cursor < line.symbols.len() && is_ascii_word_symbol(&line.symbols[cursor].text) {
                cursor += 1;
            }
        } else {
            cursor += 1;
        }
        if let Some(span) = span_from_symbols(
            &line.symbols[start..cursor],
            quad,
            line.timestep_count,
            axis,
            reverse,
        ) {
            word_spans.push(span);
        }
    }
    (character_spans, word_spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> DecodedLine {
        DecodedLine {
            text: "AB中".to_owned(),
            text_score: 0.9,
            timestep_count: 10,
            recovered_symbol_count: 0,
            symbols: vec![
                DecodedSymbol {
                    text: "A".to_owned(),
                    score: 0.9,
                    start_timestep: 1,
                    end_timestep: 2,
                },
                DecodedSymbol {
                    text: "B".to_owned(),
                    score: 0.8,
                    start_timestep: 3,
                    end_timestep: 4,
                },
                DecodedSymbol {
                    text: "中".to_owned(),
                    score: 1.0,
                    start_timestep: 6,
                    end_timestep: 8,
                },
            ],
        }
    }

    #[test]
    fn projects_character_and_ascii_word_spans_into_the_line_quad() {
        let points = [
            OcrPoint { x: 0, y: 0 },
            OcrPoint { x: 100, y: 0 },
            OcrPoint { x: 100, y: 20 },
            OcrPoint { x: 0, y: 20 },
        ];
        let (characters, words) =
            project_text_spans(&line(), &points, RecognitionAxis::Horizontal, false);

        assert_eq!(characters.len(), 3);
        assert!((characters[0].box_points[0].x - 10.0).abs() < 0.01);
        assert!((characters[2].box_points[1].x - 80.0).abs() < 0.01);
        assert_eq!(
            words
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>(),
            ["AB", "中"]
        );
    }

    #[test]
    fn reverses_timesteps_when_angle_correction_flips_the_crop() {
        let points = [
            OcrPoint { x: 0, y: 0 },
            OcrPoint { x: 100, y: 0 },
            OcrPoint { x: 100, y: 20 },
            OcrPoint { x: 0, y: 20 },
        ];
        let (characters, _) =
            project_text_spans(&line(), &points, RecognitionAxis::Horizontal, true);
        assert!((characters[0].box_points[0].x - 80.0).abs() < 0.01);
        assert!((characters[0].box_points[1].x - 90.0).abs() < 0.01);
    }
}
