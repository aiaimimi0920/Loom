//! Conservative font metrics for OCR grouping and bounded declarative reflow.
use crate::translation_input::TextBlock;

const LINE_HEIGHT: f64 = 1.2;

pub fn source_font_size(block: &TextBlock) -> f64 {
    let units: f64 = block.text.chars().map(glyph_width).sum();
    (block.height * 0.8).min(block.width / units.max(1.0))
}

pub fn fit_font_size(text: &str, width: f64, height: f64, source_font: f64) -> f64 {
    let fits = |font: f64| {
        estimated_lines(text, width * 0.96 / font) as f64 * font * LINE_HEIGHT <= height * 0.96
    };
    let mut lower = 0.0;
    let mut upper = source_font;
    if fits(upper) {
        return upper;
    }
    // Input/output budgets bound both the text walk and this fixed-size search.
    for _ in 0..20 {
        let middle = (lower + upper) / 2.0;
        if fits(middle) {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    lower
}

fn estimated_lines(text: &str, capacity: f64) -> usize {
    let mut lines = 1;
    let mut used = 0.0;
    let mut word = 0.0;
    let place = |units: f64, used: &mut f64, lines: &mut usize| {
        if *used > 0.0 && *used + units > capacity {
            *lines += 1;
            *used = 0.0;
        }
        let extra = (units / capacity).ceil().max(1.0) as usize - 1;
        *lines += extra;
        *used += units - extra as f64 * capacity;
    };
    for ch in text.chars().chain(std::iter::once('\n')) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '\'' | '-') {
            word += glyph_width(ch);
            continue;
        }
        if word > 0.0 {
            place(word, &mut used, &mut lines);
            word = 0.0;
        }
        if ch == '\n' {
            lines += 1;
            used = 0.0;
        } else {
            place(glyph_width(ch), &mut used, &mut lines);
        }
    }
    lines - 1
}

fn glyph_width(ch: char) -> f64 {
    match ch {
        '\n' | '\r' | '\u{0300}'..='\u{036f}' => 0.0,
        ' ' | '\t' => 0.35,
        'i' | 'l' | 'I' | '.' | ',' | ':' | ';' | '!' | '\'' | '|' => 0.35,
        'm' | 'w' | 'M' | 'W' | '@' => 0.95,
        ch if ch.is_ascii() => 0.65,
        _ => 1.05,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_reflow_keeps_readable_source_size_when_several_lines_fit() {
        let text = "A complete paragraph can wrap inside its original multi-line region.";
        assert_eq!(fit_font_size(text, 220.0, 120.0, 18.0), 18.0);
        assert!(estimated_lines(text, 220.0 * 0.96 / 18.0) > 1);
    }

    #[test]
    fn long_words_cjk_and_explicit_breaks_fit_without_a_minimum_font_overflow() {
        for text in [
            "W".repeat(500),
            "完整译文".repeat(100),
            "one\ntwo\nthree".to_owned(),
        ] {
            let font = fit_font_size(&text, 30.0, 20.0, 18.0);
            assert!(font > 0.0 && font < 18.0);
            assert!(estimated_lines(&text, 30.0 * 0.96 / font) as f64 * font * LINE_HEIGHT <= 20.0);
        }
    }
}
