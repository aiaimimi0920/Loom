// Calculates OCR scene text metrics without relying on one container axis.

const HEIGHT_RATIO: f32 = 0.74;
const WIDTH_SAFETY_RATIO: f32 = 0.94;

pub(crate) struct SceneTypography {
    pub(crate) font_size: String,
    pub(crate) line_height: String,
}

pub(crate) fn scene_typography(
    text: &str,
    width: f32,
    height: f32,
    source_width: u32,
    source_height: u32,
) -> SceneTypography {
    let height_bound =
        (height / source_height.max(1) as f32 * 100.0 * HEIGHT_RATIO).clamp(0.001, 100.0);
    let width_bound = (width / source_width.max(1) as f32 * 100.0 / maximum_line_advance(text)
        * WIDTH_SAFETY_RATIO)
        .clamp(0.001, 100.0);
    let line_height = (height / source_height.max(1) as f32 * 100.0).clamp(0.001, 100.0);
    SceneTypography {
        font_size: format!("min({height_bound:.4}cqh, {width_bound:.4}cqw)"),
        line_height: format!("{line_height:.4}cqh"),
    }
}

fn maximum_line_advance(text: &str) -> f32 {
    text.lines()
        .map(|line| line.chars().map(character_advance).sum::<f32>())
        .fold(1.0, f32::max)
}

fn character_advance(character: char) -> f32 {
    if character.is_whitespace() {
        0.32
    } else if matches!(character, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}') {
        1.0
    } else if matches!(character, 'M' | 'W' | 'm' | 'w' | '@' | '%') {
        0.9
    } else if matches!(character, 'i' | 'l' | 'I' | '1' | 't' | 'f') {
        0.4
    } else if character.is_ascii_uppercase() {
        0.66
    } else if character.is_ascii_lowercase() || character.is_ascii_digit() {
        0.56
    } else if character == '_' {
        0.58
    } else if character.is_ascii_punctuation() {
        0.38
    } else {
        0.86
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_receives_independent_height_and_width_bounds() {
        let style = scene_typography("require_trusted", 120.0, 24.0, 900, 350);

        assert!(style.font_size.starts_with("min("));
        assert!(style.font_size.contains("cqh, "));
        assert!(style.font_size.ends_with("cqw)"));
        assert_eq!(style.line_height, "6.8571cqh");
    }

    #[test]
    fn longest_visual_line_controls_width_fit() {
        let short = scene_typography("短\n文本", 100.0, 20.0, 400, 200);
        let long = scene_typography("短\n这是一条更长的文本", 100.0, 20.0, 400, 200);

        assert!(width_bound(&long.font_size) < width_bound(&short.font_size));
    }

    fn width_bound(value: &str) -> f32 {
        value
            .split(", ")
            .nth(1)
            .unwrap()
            .trim_end_matches("cqw)")
            .parse()
            .unwrap()
    }
}
