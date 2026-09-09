use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const MAX_TEXT_CHARS: usize = 4096;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranslationInput {
    pub text: String,
    pub target_language: String,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationOutput {
    pub text: String,
    pub target_language: String,
    pub translated: bool,
}

pub fn translate(input: TranslationInput) -> Result<TranslationOutput> {
    if input.text.is_empty() || input.text.chars().count() > MAX_TEXT_CHARS {
        bail!("text must contain between 1 and {MAX_TEXT_CHARS} Unicode characters");
    }
    let translated = match input.target_language.as_str() {
        "zh-CN" => translate_to_chinese(&input.text),
        "en" => translate_to_english(&input.text),
        _ => bail!("targetLanguage must be zh-CN or en"),
    };
    Ok(TranslationOutput {
        translated: translated.is_some(),
        text: translated.unwrap_or(input.text),
        target_language: input.target_language,
    })
}

fn translate_to_chinese(text: &str) -> Option<String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "hello" => Some("你好".to_owned()),
        "thank you" => Some("谢谢".to_owned()),
        "goodbye" => Some("再见".to_owned()),
        _ => None,
    }
}

fn translate_to_english(text: &str) -> Option<String> {
    match text.trim() {
        "你好" => Some("hello".to_owned()),
        "谢谢" => Some("thank you".to_owned()),
        "再见" => Some("goodbye".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_both_supported_directions_and_preserves_unknown_text() {
        let chinese = translate(TranslationInput {
            text: "Hello".to_owned(),
            target_language: "zh-CN".to_owned(),
        })
        .unwrap();
        assert_eq!(chinese.text, "你好");
        assert!(chinese.translated);

        let unchanged = translate(TranslationInput {
            text: "bounded unknown text".to_owned(),
            target_language: "en".to_owned(),
        })
        .unwrap();
        assert_eq!(unchanged.text, "bounded unknown text");
        assert!(!unchanged.translated);

        let oversized = translate(TranslationInput {
            text: "界".repeat(MAX_TEXT_CHARS + 1),
            target_language: "en".to_owned(),
        });
        assert!(oversized.is_err());
    }
}
