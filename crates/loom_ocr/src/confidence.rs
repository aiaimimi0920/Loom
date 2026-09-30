use crate::ctc_decode::DecodedLine;
use crate::types::{OcrTextConfidence, OcrTextConfidenceSource};

pub(crate) fn summarize_line(line: &DecodedLine) -> Option<OcrTextConfidence> {
    if line.symbols.is_empty() {
        return None;
    }
    let symbol_count = u32::try_from(line.symbols.len()).ok()?;
    let recovered_symbol_count = u32::try_from(line.recovered_symbol_count)
        .unwrap_or(u32::MAX)
        .min(symbol_count);
    let minimum_symbol_score = line
        .symbols
        .iter()
        .map(|symbol| bounded_score(symbol.score))
        .fold(1.0, f32::min);
    Some(OcrTextConfidence {
        mean_symbol_score: bounded_score(line.text_score),
        minimum_symbol_score,
        symbol_count,
        recovered_symbol_count,
        source: OcrTextConfidenceSource::CtcDecodedSymbolScores,
    })
}

pub(crate) fn merge(
    left: Option<OcrTextConfidence>,
    right: Option<OcrTextConfidence>,
) -> Option<OcrTextConfidence> {
    let (left, right) = (left?, right?);
    if left.source != right.source {
        return None;
    }
    let symbol_count = left.symbol_count.checked_add(right.symbol_count)?;
    if symbol_count == 0 {
        return None;
    }
    let weighted_sum = left.mean_symbol_score * left.symbol_count as f32
        + right.mean_symbol_score * right.symbol_count as f32;
    Some(OcrTextConfidence {
        mean_symbol_score: bounded_score(weighted_sum / symbol_count as f32),
        minimum_symbol_score: left.minimum_symbol_score.min(right.minimum_symbol_score),
        symbol_count,
        recovered_symbol_count: left
            .recovered_symbol_count
            .saturating_add(right.recovered_symbol_count)
            .min(symbol_count),
        source: left.source,
    })
}

fn bounded_score(score: f32) -> f32 {
    if score.is_finite() {
        score.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(mean: f32, minimum: f32, count: u32) -> OcrTextConfidence {
        OcrTextConfidence {
            mean_symbol_score: mean,
            minimum_symbol_score: minimum,
            symbol_count: count,
            recovered_symbol_count: 0,
            source: OcrTextConfidenceSource::CtcDecodedSymbolScores,
        }
    }

    #[test]
    fn merged_confidence_is_weighted_and_keeps_the_weakest_symbol() {
        let merged = merge(Some(summary(0.9, 0.7, 2)), Some(summary(0.6, 0.4, 1)))
            .expect("complete evidence");
        assert!((merged.mean_symbol_score - 0.8).abs() < 0.001);
        assert_eq!(merged.minimum_symbol_score, 0.4);
        assert_eq!(merged.symbol_count, 3);
    }
}
