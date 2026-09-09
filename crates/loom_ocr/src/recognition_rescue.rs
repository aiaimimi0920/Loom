use crate::ctc_decode::DecodedLine;
use crate::ctc_recognizer::{CtcRecognizer, SessionBuilderFn};
use crate::OcrResult;

const TINY_TEXT_HEIGHT: u32 = 24;
const LOW_CONFIDENCE_SCORE: f32 = 0.92;
const LOWEST_SYMBOL_SCORE: f32 = 0.55;
const STRONGER_CANDIDATE_MARGIN: f32 = 0.08;
const PUNCTUATION_RECOVERY_MARGIN: f32 = 0.12;
const CORE_EXTENSION_SCORE_TOLERANCE: f32 = 0.04;
const MIN_INSERTED_SYMBOL_SCORE: f32 = 0.85;
const MAX_CORE_EXTENSION: usize = 4;

#[derive(Debug)]
pub(crate) struct RecognitionRescue {
    primary: CtcRecognizer,
    fallback_model: Option<Vec<u8>>,
    fallback: Option<CtcRecognizer>,
    builder_fn: SessionBuilderFn,
}

impl RecognitionRescue {
    pub(crate) fn new(
        primary_model: &[u8],
        fallback_model: Option<Vec<u8>>,
        builder_fn: SessionBuilderFn,
    ) -> OcrResult<Self> {
        Ok(Self {
            primary: CtcRecognizer::from_memory(primary_model, builder_fn)?,
            fallback_model,
            fallback: None,
            builder_fn,
        })
    }

    /// Runs bounded secondary preprocessing/model passes only for tiny or weak lines.
    pub(crate) fn recognize(
        &mut self,
        image: &image::RgbImage,
        allow_rescue: bool,
        enhanced_fallback: bool,
    ) -> OcrResult<DecodedLine> {
        let mut best = self.primary.recognize(image)?;
        if !allow_rescue || !needs_rescue(image, &best) {
            return Ok(best);
        }

        let enhanced = enhance_contrast(image);
        let enhanced_candidate = self.primary.recognize(&enhanced)?;
        best = choose_preferred_line(best, enhanced_candidate);

        if needs_rescue(image, &best) {
            if let Some(fallback) = self.fallback() {
                let fallback_candidate = fallback.recognize(image)?;
                best = choose_preferred_line(best, fallback_candidate);
                if enhanced_fallback && needs_rescue(image, &best) {
                    let enhanced_fallback_candidate = fallback.recognize(&enhanced)?;
                    best = choose_preferred_line(best, enhanced_fallback_candidate);
                }
            }
        }
        Ok(best)
    }

    /// Lazily materialises the optional fallback recognizer.
    ///
    /// The fallback model is an accuracy bonus, not a requirement: the primary pass has
    /// already produced a usable line by the time this runs. Propagating a session-build
    /// failure out of here aborted the whole page instead of returning the text the
    /// primary model had recognised, so a bad or unreadable optional model turned a
    /// degraded result into no result at all. The failure is reported once on stderr and
    /// the fallback stays permanently unavailable for the rest of the process.
    fn fallback(&mut self) -> Option<&mut CtcRecognizer> {
        if self.fallback.is_none() {
            let model = self.fallback_model.take()?;
            match CtcRecognizer::from_memory(&model, self.builder_fn) {
                Ok(recognizer) => self.fallback = Some(recognizer),
                Err(error) => {
                    eprintln!("ocr fallback recognizer unavailable, continuing with the primary model: {error}");
                    return None;
                }
            }
        }
        self.fallback.as_mut()
    }
}

fn confidence(line: &DecodedLine) -> f32 {
    if line.text_score.is_finite() {
        line.text_score
    } else {
        0.0
    }
}

fn needs_rescue(image: &image::RgbImage, line: &DecodedLine) -> bool {
    line.recovered_symbol_count > 0
        || image.height() <= TINY_TEXT_HEIGHT
        || line.text.trim().is_empty()
        || confidence(line) < LOW_CONFIDENCE_SCORE
        || line
            .symbols
            .iter()
            .any(|symbol| !symbol.score.is_finite() || symbol.score < LOWEST_SYMBOL_SCORE)
}

fn semantic_core(text: &str) -> Vec<char> {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn is_subsequence(needle: &[char], haystack: &[char]) -> bool {
    let mut cursor = 0;
    for character in haystack {
        if needle.get(cursor) == Some(character) {
            cursor += 1;
        }
    }
    cursor == needle.len()
}

fn scored_semantic_core(line: &DecodedLine) -> Vec<(char, f32)> {
    line.symbols
        .iter()
        .flat_map(|symbol| {
            symbol
                .text
                .chars()
                .filter(|character| character.is_alphanumeric())
                .map(|character| (character, symbol.score))
        })
        .collect()
}

/// Accepts only well-supported characters inserted inside an existing core.
/// Prefix/suffix additions remain excluded because they are more likely to be
/// detector padding or neighbouring text than a character omitted by one pass.
fn has_supported_interior_extension(current: &DecodedLine, candidate: &DecodedLine) -> bool {
    let current_core = scored_semantic_core(current);
    let candidate_core = scored_semantic_core(candidate);
    if current_core.is_empty()
        || candidate_core.len() <= current_core.len()
        || candidate_core.len() - current_core.len() > MAX_CORE_EXTENSION
    {
        return false;
    }

    let mut current_index = 0;
    let mut inserted = Vec::new();
    for (candidate_index, (character, score)) in candidate_core.iter().copied().enumerate() {
        if current_core.get(current_index).map(|entry| entry.0) == Some(character) {
            current_index += 1;
        } else {
            inserted.push((candidate_index, score));
        }
    }
    current_index == current_core.len()
        && !inserted.is_empty()
        && inserted.iter().all(|(index, score)| {
            *index > 0
                && *index + 1 < candidate_core.len()
                && score.is_finite()
                && *score >= MIN_INSERTED_SYMBOL_SCORE
        })
}

pub(crate) fn choose_preferred_line(current: DecodedLine, candidate: DecodedLine) -> DecodedLine {
    if candidate.text.trim().is_empty() {
        return current;
    }
    if current.text.trim().is_empty() {
        return candidate;
    }
    let current_score = confidence(&current);
    let candidate_score = confidence(&candidate);
    let current_core = semantic_core(&current.text);
    let candidate_core = semantic_core(&candidate.text);
    if current_core == candidate_core {
        if candidate.symbols.len() > current.symbols.len()
            && candidate_score + PUNCTUATION_RECOVERY_MARGIN >= current_score
        {
            return candidate;
        }
        if current.symbols.len() > candidate.symbols.len()
            && current_score + PUNCTUATION_RECOVERY_MARGIN >= candidate_score
        {
            return current;
        }
    }
    if candidate_score >= current_score + STRONGER_CANDIDATE_MARGIN {
        return candidate;
    }
    if is_subsequence(&current_core, &candidate_core)
        && has_supported_interior_extension(&current, &candidate)
        && candidate_score + CORE_EXTENSION_SCORE_TOLERANCE >= current_score
    {
        return candidate;
    }
    current
}

fn enhance_contrast(image: &image::RgbImage) -> image::RgbImage {
    let mut enhanced = image.clone();
    for pixel in enhanced.pixels_mut() {
        for channel in &mut pixel.0 {
            let centered = i32::from(*channel) - 128;
            *channel = (128 + centered * 5 / 4).clamp(0, 255) as u8;
        }
    }
    enhanced
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctc_decode::DecodedSymbol;

    fn line(text: &str, score: f32) -> DecodedLine {
        DecodedLine {
            text: text.to_owned(),
            text_score: score,
            timestep_count: text.chars().count().max(1),
            recovered_symbol_count: 0,
            symbols: text
                .chars()
                .enumerate()
                .map(|(index, character)| DecodedSymbol {
                    text: character.to_string(),
                    score,
                    start_timestep: index,
                    end_timestep: index + 1,
                })
                .collect(),
        }
    }

    #[test]
    fn recovers_supported_punctuation_without_guessing_new_semantic_text() {
        assert_eq!(
            choose_preferred_line(line("Ctrl+2", 0.96), line("# Ctrl+2", 0.88)).text,
            "# Ctrl+2"
        );
        assert_eq!(
            choose_preferred_line(line("safe", 0.96), line("sake", 0.95)).text,
            "safe"
        );
        assert_eq!(
            choose_preferred_line(line("## title", 0.90), line("# title", 0.99)).text,
            "## title"
        );
    }

    #[test]
    fn accepts_a_higher_confidence_model_candidate_that_restores_core_characters() {
        assert_eq!(
            choose_preferred_line(line("Crl+2", 0.80), line("Ctrl+2", 0.86)).text,
            "Ctrl+2"
        );
    }

    #[test]
    fn accepts_a_slightly_lower_score_candidate_with_supported_interior_text() {
        assert_eq!(
            choose_preferred_line(line("Crl+2", 0.96), line("Ctrl+2", 0.94)).text,
            "Ctrl+2"
        );
        assert_eq!(
            choose_preferred_line(line("safe", 0.96), line("safee", 0.95)).text,
            "safe"
        );
        assert_eq!(
            choose_preferred_line(line("safe", 0.96), line("saXfe", 0.80)).text,
            "safe"
        );
    }

    #[test]
    fn treats_non_ascii_letters_as_real_semantic_core_characters() {
        assert_eq!(
            choose_preferred_line(line("甲乙", 0.96), line("甲丙乙", 0.95)).text,
            "甲丙乙"
        );
    }

    #[test]
    fn recovered_spacing_does_not_hide_a_low_confidence_line_from_rescue() {
        let image = image::RgbImage::new(120, 32);
        let mut weak = line("# title", 0.90);
        weak.recovered_symbol_count = 1;

        assert!(needs_rescue(&image, &weak));
        assert_eq!(
            choose_preferred_line(weak, line("## title", 0.82)).text,
            "## title"
        );
    }

    #[test]
    fn a_weak_symbol_triggers_rescue_even_when_the_line_average_is_high() {
        let image = image::RgbImage::new(180, 32);
        let mut weak = line("mostly confident", 0.98);
        weak.symbols[4].score = 0.20;

        assert!(needs_rescue(&image, &weak));
    }
}
