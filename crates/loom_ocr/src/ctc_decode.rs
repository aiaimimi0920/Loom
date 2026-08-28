use crate::{OcrError, OcrResult};

const MAX_CTC_TIMESTEPS: usize = 16_384;
const MAX_CTC_CLASSES: usize = 100_000;
const MAX_CTC_SYMBOLS: usize = 4_096;
const MIN_PUNCTUATION_ALTERNATIVE_SCORE: f32 = 0.12;
const MIN_PUNCTUATION_COMBINED_SCORE: f32 = 0.55;
const MIN_PUNCTUATION_PEAK_SCORE: f32 = 0.25;
const MIN_REPEATED_PUNCTUATION_BLANK_SCORE: f32 = 0.25;
const MIN_REPEATED_PUNCTUATION_SIDE_SCORE: f32 = 0.55;
const MIN_SPACE_COMBINED_SCORE: f32 = 0.03;
const MAX_ALTERNATIVE_TIMESTEP_GAP: usize = 2;

#[derive(Clone, Copy, Debug)]
struct TimestepChoice {
    primary_index: usize,
    primary_score: f32,
    alternative_index: usize,
    alternative_score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedSymbol {
    pub text: String,
    pub score: f32,
    pub start_timestep: usize,
    pub end_timestep: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedLine {
    pub text: String,
    pub text_score: f32,
    pub symbols: Vec<DecodedSymbol>,
    pub timestep_count: usize,
    pub recovered_symbol_count: usize,
}

impl DecodedLine {
    pub(crate) fn empty(timestep_count: usize) -> Self {
        Self {
            text: String::new(),
            text_score: 0.0,
            symbols: Vec::new(),
            timestep_count,
            recovered_symbol_count: 0,
        }
    }
}

/// Applies standard greedy CTC blank/repeat collapse while retaining timestep spans.
pub(crate) fn decode_ctc(
    output: &[f32],
    timesteps: usize,
    classes: usize,
    keys: &[String],
) -> OcrResult<DecodedLine> {
    let expected = timesteps
        .checked_mul(classes)
        .ok_or_else(|| OcrError::Detect("CTC output dimensions overflowed".to_owned()))?;
    if timesteps == 0
        || timesteps > MAX_CTC_TIMESTEPS
        || classes < 2
        || classes > MAX_CTC_CLASSES
        || output.len() != expected
        || keys.len() < 2
    {
        return Err(OcrError::Detect("invalid CTC output dimensions".to_owned()));
    }

    let choices = output
        .chunks_exact(classes)
        .map(top_two)
        .collect::<Vec<_>>();
    let mut line = DecodedLine::empty(timesteps);
    let mut previous_class = 0_usize;
    for (timestep, choice) in choices.iter().enumerate() {
        let class_index = choice.primary_index;
        let score = choice.primary_score;

        if class_index == 0 || class_index >= keys.len() || keys[class_index].is_empty() {
            previous_class = class_index;
            continue;
        }
        if class_index == previous_class {
            if let Some(symbol) = line.symbols.last_mut() {
                symbol.end_timestep = timestep + 1;
                symbol.score = symbol.score.max(score);
            }
            continue;
        }
        if line.symbols.len() >= MAX_CTC_SYMBOLS {
            return Err(OcrError::Detect("CTC symbol limit exceeded".to_owned()));
        }

        let text = keys[class_index].clone();
        line.symbols.push(DecodedSymbol {
            text,
            score,
            start_timestep: timestep,
            end_timestep: timestep + 1,
        });
        previous_class = class_index;
    }

    line.recovered_symbol_count +=
        split_repeated_punctuation_symbols(&choices, keys, &mut line.symbols);
    let primary_symbols = line.symbols.clone();
    let punctuation = recover_punctuation_symbols(&choices, keys, &primary_symbols);
    line.recovered_symbol_count += punctuation.len();
    line.symbols.extend(punctuation);
    let spaces = recover_spaces(&choices, keys, &line.symbols);
    line.recovered_symbol_count += spaces.len();
    line.symbols.extend(spaces);
    line.symbols
        .sort_by_key(|symbol| (symbol.start_timestep, symbol.end_timestep));
    line.symbols.truncate(MAX_CTC_SYMBOLS);
    line.text = line
        .symbols
        .iter()
        .map(|symbol| symbol.text.as_str())
        .collect();

    if !line.symbols.is_empty() {
        line.text_score =
            line.symbols.iter().map(|symbol| symbol.score).sum::<f32>() / line.symbols.len() as f32;
    }
    Ok(line)
}

fn top_two(row: &[f32]) -> TimestepChoice {
    let mut primary = (0_usize, f32::NEG_INFINITY);
    let mut alternative = (0_usize, f32::NEG_INFINITY);
    for (index, score) in row.iter().copied().enumerate() {
        if score > primary.1 {
            alternative = primary;
            primary = (index, score);
        } else if score > alternative.1 {
            alternative = (index, score);
        }
    }
    TimestepChoice {
        primary_index: primary.0,
        primary_score: primary.1,
        alternative_index: alternative.0,
        alternative_score: alternative.1,
    }
}

fn is_recoverable_punctuation(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|character| {
            character.is_ascii_punctuation()
                || "，。；：！？、“”‘’（）【】《》—…·＃－".contains(character)
        })
}

fn overlaps_primary(start: usize, end: usize, primary: &[DecodedSymbol]) -> bool {
    primary
        .iter()
        .any(|symbol| start < symbol.end_timestep && end > symbol.start_timestep)
}

/// Recovers adjacent repeated punctuation when CTC collapsed a blank valley.
/// A strong blank alternative must split a sustained punctuation run into two
/// independently supported sides; a wide glyph alone is not enough evidence.
fn split_repeated_punctuation_symbols(
    choices: &[TimestepChoice],
    keys: &[String],
    symbols: &mut Vec<DecodedSymbol>,
) -> usize {
    let mut recovered = Vec::new();
    for symbol in symbols.iter_mut() {
        if symbol.text.chars().count() != 1
            || !is_recoverable_punctuation(&symbol.text)
            || symbol.end_timestep.saturating_sub(symbol.start_timestep) < 3
        {
            continue;
        }
        let class_index = choices
            .get(symbol.start_timestep)
            .map(|choice| choice.primary_index)
            .unwrap_or(0);
        if class_index == 0 || keys.get(class_index) != Some(&symbol.text) {
            continue;
        }

        let split = (symbol.start_timestep + 1..symbol.end_timestep - 1)
            .filter_map(|timestep| {
                let choice = choices[timestep];
                (choice.primary_index == class_index
                    && choice.alternative_index == 0
                    && choice.alternative_score >= MIN_REPEATED_PUNCTUATION_BLANK_SCORE)
                    .then_some((timestep, choice.alternative_score))
            })
            .max_by(|left, right| left.1.total_cmp(&right.1));
        let Some((split_timestep, blank_score)) = split else {
            continue;
        };
        let left_score = choices[symbol.start_timestep..split_timestep]
            .iter()
            .filter(|choice| choice.primary_index == class_index)
            .map(|choice| choice.primary_score)
            .fold(0.0_f32, f32::max);
        let right_score = choices[split_timestep + 1..symbol.end_timestep]
            .iter()
            .filter(|choice| choice.primary_index == class_index)
            .map(|choice| choice.primary_score)
            .fold(0.0_f32, f32::max);
        if left_score < MIN_REPEATED_PUNCTUATION_SIDE_SCORE
            || right_score < MIN_REPEATED_PUNCTUATION_SIDE_SCORE
        {
            continue;
        }

        let original_end = symbol.end_timestep;
        symbol.end_timestep = split_timestep;
        symbol.score = left_score;
        recovered.push(DecodedSymbol {
            text: symbol.text.clone(),
            score: blank_score.min(right_score),
            start_timestep: split_timestep + 1,
            end_timestep: original_end,
        });
    }
    let count = recovered.len();
    symbols.extend(recovered);
    symbols.sort_by_key(|symbol| (symbol.start_timestep, symbol.end_timestep));
    count
}

fn combined_probability(scores: &[f32]) -> f32 {
    1.0 - scores
        .iter()
        .map(|score| 1.0 - score.clamp(0.0, 1.0))
        .product::<f32>()
}

fn recover_punctuation_symbols(
    choices: &[TimestepChoice],
    keys: &[String],
    primary: &[DecodedSymbol],
) -> Vec<DecodedSymbol> {
    let mut recovered = Vec::new();
    let mut cursor = 0;
    while cursor < choices.len() {
        let choice = choices[cursor];
        let text = keys
            .get(choice.alternative_index)
            .map(String::as_str)
            .unwrap_or("");
        if choice.primary_index != 0
            || choice.alternative_index == 0
            || choice.alternative_score < MIN_PUNCTUATION_ALTERNATIVE_SCORE
            || !is_recoverable_punctuation(text)
        {
            cursor += 1;
            continue;
        }

        let class_index = choice.alternative_index;
        let start = cursor;
        let mut last = cursor;
        let mut scores = vec![choice.alternative_score];
        cursor += 1;
        while cursor < choices.len() {
            let next = choices[cursor];
            if next.primary_index == 0
                && next.alternative_index == class_index
                && next.alternative_score >= MIN_PUNCTUATION_ALTERNATIVE_SCORE
                && cursor - last <= MAX_ALTERNATIVE_TIMESTEP_GAP
            {
                scores.push(next.alternative_score);
                last = cursor;
            }
            if cursor - last > MAX_ALTERNATIVE_TIMESTEP_GAP {
                break;
            }
            cursor += 1;
        }
        let end = last + 1;
        let combined = combined_probability(&scores);
        let peak = scores.iter().copied().fold(0.0_f32, f32::max);
        if scores.len() >= 2
            && combined >= MIN_PUNCTUATION_COMBINED_SCORE
            && peak >= MIN_PUNCTUATION_PEAK_SCORE
            && !overlaps_primary(start, end, primary)
        {
            recovered.push(DecodedSymbol {
                text: keys[class_index].clone(),
                score: combined,
                start_timestep: start,
                end_timestep: end,
            });
        }
    }
    recovered
}

fn median(mut values: Vec<usize>) -> usize {
    values.sort_unstable();
    values[values.len() / 2]
}

fn has_script_boundary(left: &str, right: &str) -> bool {
    let left_ascii = left.chars().all(|character| character.is_ascii());
    let right_ascii = right.chars().all(|character| character.is_ascii());
    left_ascii != right_ascii
        || left
            .chars()
            .all(|character| character.is_ascii_punctuation())
}

fn recover_spaces(
    choices: &[TimestepChoice],
    keys: &[String],
    symbols: &[DecodedSymbol],
) -> Vec<DecodedSymbol> {
    let Some(space_index) = keys.iter().position(|key| key == " ") else {
        return Vec::new();
    };
    let mut ordered = symbols.to_vec();
    ordered.sort_by_key(|symbol| symbol.start_timestep);
    if ordered.len() < 4 {
        return Vec::new();
    }
    let advances = ordered
        .windows(2)
        .map(|pair| {
            pair[1]
                .start_timestep
                .saturating_sub(pair[0].start_timestep)
        })
        .filter(|advance| *advance > 0)
        .collect::<Vec<_>>();
    if advances.is_empty() {
        return Vec::new();
    }
    let typical_advance = median(advances);
    let minimum_advance = typical_advance
        .saturating_add(1)
        .max((typical_advance as f32 * 1.2).ceil() as usize);

    ordered
        .windows(2)
        .filter_map(|pair| {
            let left = &pair[0];
            let right = &pair[1];
            let advance = right.start_timestep.saturating_sub(left.start_timestep);
            if advance < minimum_advance
                || right.start_timestep <= left.end_timestep
                || !has_script_boundary(&left.text, &right.text)
            {
                return None;
            }
            let scores = choices[left.end_timestep..right.start_timestep]
                .iter()
                .filter_map(|choice| {
                    (choice.primary_index == space_index)
                        .then_some(choice.primary_score)
                        .or_else(|| {
                            (choice.alternative_index == space_index)
                                .then_some(choice.alternative_score)
                        })
                })
                .collect::<Vec<_>>();
            let score = combined_probability(&scores);
            (score >= MIN_SPACE_COMBINED_SCORE).then(|| DecodedSymbol {
                text: " ".to_owned(),
                score,
                start_timestep: left.end_timestep,
                end_timestep: right.start_timestep,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(winner: usize, score: f32) -> Vec<f32> {
        let mut row = vec![0.01; 4];
        row[winner] = score;
        row
    }

    fn scored_row(classes: usize, values: &[(usize, f32)]) -> Vec<f32> {
        let mut row = vec![0.0001; classes];
        for (index, score) in values {
            row[*index] = *score;
        }
        row
    }

    #[test]
    fn retains_ctc_timestep_spans_across_blank_and_repeat_collapse() {
        let keys = ["#", "A", "-", "B"].map(str::to_owned);
        let output = [
            row(0, 0.9),
            row(1, 0.8),
            row(1, 0.9),
            row(2, 0.7),
            row(0, 0.8),
            row(1, 0.85),
        ]
        .concat();

        let decoded = decode_ctc(&output, 6, 4, &keys).expect("decode");
        assert_eq!(decoded.text, "A-A");
        assert_eq!(decoded.symbols[0].start_timestep, 1);
        assert_eq!(decoded.symbols[0].end_timestep, 3);
        assert_eq!(decoded.symbols[1].text, "-");
        assert_eq!(decoded.symbols[2].start_timestep, 5);
    }

    #[test]
    fn rejects_malformed_or_unbounded_model_output() {
        let keys = ["#".to_owned(), "A".to_owned()];
        assert!(decode_ctc(&[0.0; 3], 2, 2, &keys).is_err());
        assert!(decode_ctc(&[0.0; 2], MAX_CTC_TIMESTEPS + 1, 2, &keys).is_err());
    }

    #[test]
    fn restores_repeated_punctuation_from_consistent_blank_alternatives() {
        let keys = ["#", "#", " ", "建"].map(str::to_owned);
        let output = [
            scored_row(4, &[(1, 0.99), (0, 0.001)]),
            scored_row(4, &[(0, 0.78), (1, 0.21)]),
            scored_row(4, &[(0, 0.98), (1, 0.01)]),
            scored_row(4, &[(0, 0.62), (1, 0.37)]),
            scored_row(4, &[(0, 0.68), (1, 0.31)]),
            scored_row(4, &[(0, 0.99), (3, 0.001)]),
            scored_row(4, &[(3, 0.99), (0, 0.001)]),
        ]
        .concat();

        assert_eq!(
            decode_ctc(&output, 7, 4, &keys).expect("decode").text,
            "##建"
        );
    }

    #[test]
    fn restores_repeated_punctuation_from_an_internal_ctc_blank_valley() {
        let keys = ["#", "A", "#", "B"].map(str::to_owned);
        let output = [
            scored_row(4, &[(0, 0.99), (2, 0.001)]),
            scored_row(4, &[(2, 0.98), (0, 0.02)]),
            scored_row(4, &[(2, 0.63), (0, 0.36)]),
            scored_row(4, &[(2, 0.69), (0, 0.30)]),
            scored_row(4, &[(0, 0.99), (2, 0.001)]),
            scored_row(4, &[(3, 0.99), (0, 0.001)]),
        ]
        .concat();

        let decoded = decode_ctc(&output, 6, 4, &keys).expect("decode");
        assert_eq!(decoded.text, "##B");
        assert_eq!(decoded.recovered_symbol_count, 1);
        assert_eq!(decoded.symbols[0].end_timestep, 2);
        assert_eq!(decoded.symbols[1].start_timestep, 3);
    }

    #[test]
    fn restores_spaces_only_when_gap_and_ctc_evidence_agree() {
        let keys = ["#", "A", "B", "中", " "].map(str::to_owned);
        let mut rows = Vec::new();
        for values in [
            vec![(1, 0.99)],
            vec![(0, 0.99)],
            vec![(2, 0.99)],
            vec![(0, 0.99), (4, 0.01)],
            vec![(0, 0.97), (4, 0.02)],
            vec![(0, 0.97), (4, 0.02)],
            vec![(0, 0.99), (4, 0.01)],
            vec![(3, 0.99)],
            vec![(0, 0.99)],
            vec![(1, 0.99)],
            vec![(0, 0.99)],
            vec![(2, 0.99)],
        ] {
            rows.extend(scored_row(5, &values));
        }

        assert_eq!(
            decode_ctc(&rows, 12, 5, &keys).expect("decode").text,
            "AB 中AB"
        );
    }
}
