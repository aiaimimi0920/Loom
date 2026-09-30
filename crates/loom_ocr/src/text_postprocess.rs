#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CorrectedText {
    pub text: String,
    pub raw_text: Option<String>,
}

const SHORTCUT_CONFUSIONS: &[(&str, &str)] = &[("ctr1+", "Ctrl+"), ("a1t+", "Alt+")];

fn is_token_boundary_before(text: &str, index: usize) -> bool {
    text[..index]
        .chars()
        .next_back()
        .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
}

fn has_shortcut_key_after(text: &str, index: usize) -> bool {
    text[index..]
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
}

fn replace_shortcut_confusions(text: &str) -> Option<String> {
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    let mut changed = false;

    while index < text.len() {
        let replacement = SHORTCUT_CONFUSIONS
            .iter()
            .find_map(|(confusion, canonical)| {
                let end = index.checked_add(confusion.len())?;
                let candidate = text.get(index..end)?;
                (candidate.eq_ignore_ascii_case(confusion)
                    && is_token_boundary_before(text, index)
                    && has_shortcut_key_after(text, end))
                .then_some((*canonical, end))
            });
        if let Some((canonical, end)) = replacement {
            output.push_str(canonical);
            index = end;
            changed = true;
            continue;
        }

        let character = text[index..].chars().next()?;
        output.push(character);
        index += character.len_utf8();
    }

    changed.then_some(output)
}

/// Applies only evidence-bounded corrections for keyboard shortcut tokens.
///
/// The raw model text is retained whenever a correction occurs. General words,
/// punctuation, quotes, and whitespace are never synthesized by this layer.
pub(crate) fn correct_recognized_text(raw_text: String) -> CorrectedText {
    match replace_shortcut_confusions(&raw_text) {
        Some(text) => CorrectedText {
            text,
            raw_text: Some(raw_text),
        },
        None => CorrectedText {
            text: raw_text,
            raw_text: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrects_only_confusions_that_form_a_keyboard_shortcut() {
        assert_eq!(
            correct_recognized_text("使用A1t+2，再按 Ctr1+E。".to_owned()),
            CorrectedText {
                text: "使用Alt+2，再按 Ctrl+E。".to_owned(),
                raw_text: Some("使用A1t+2，再按 Ctr1+E。".to_owned()),
            }
        );
    }

    #[test]
    fn preserves_literal_confusion_examples_and_arbitrary_words() {
        assert_eq!(
            correct_recognized_text("Ctr1被识别成Ctr1；A1titude 不应修改".to_owned()),
            CorrectedText {
                text: "Ctr1被识别成Ctr1；A1titude 不应修改".to_owned(),
                raw_text: None,
            }
        );
    }

    #[test]
    fn preserves_punctuation_quotes_and_spacing_verbatim() {
        let text = "#  -  “quoted text”  'x'".to_owned();
        assert_eq!(
            correct_recognized_text(text.clone()),
            CorrectedText {
                text,
                raw_text: None,
            }
        );
    }
}
