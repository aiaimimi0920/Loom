use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const MAX_TEXT_CHARS: usize = 16_384;
const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAttachment {
    pub attachment_id: String,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranslationProviderMode {
    #[default]
    Auto,
    Local,
    Gateway,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextBlock {
    pub text: String,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub text_color: String,
    pub background_color: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranslationInput {
    pub text: String,
    pub target_language: String,
    #[serde(default)]
    pub provider_mode: TranslationProviderMode,
    #[serde(default)]
    pub source_revision: Option<u64>,
    #[serde(default)]
    pub source_attachment: Option<SourceAttachment>,
    #[serde(default = "default_dimension")]
    pub source_width: f64,
    #[serde(default = "default_dimension")]
    pub source_height: f64,
    #[serde(default)]
    pub text_blocks: Vec<TextBlock>,
}

fn default_dimension() -> f64 {
    100.0
}

pub fn validate(input: &TranslationInput) -> Result<()> {
    if input.text.trim().is_empty()
        || input.text.chars().count() > MAX_TEXT_CHARS
        || !matches!(input.target_language.as_str(), "zh-CN" | "en")
        || input
            .source_revision
            .is_some_and(|value| value > MAX_SAFE_REVISION)
        || input.text_blocks.len() > 128
    {
        bail!("translation text, language, revision or block count is invalid");
    }
    if [input.source_width, input.source_height]
        .iter()
        .any(|value| !value.is_finite() || !(1.0..=100_000.0).contains(value))
    {
        bail!("translation source dimensions are invalid");
    }
    let mut chars = input.text.chars().count();
    for block in &input.text_blocks {
        chars += block.text.chars().count();
        if block.text.trim().is_empty()
            || block.text.chars().count() > MAX_TEXT_CHARS
            || chars > MAX_TEXT_CHARS * 3
            || [block.left, block.top, block.width, block.height]
                .iter()
                .any(|value| !value.is_finite())
            || block.left < 0.0
            || block.top < 0.0
            || block.width <= 0.0
            || block.height <= 0.0
            || block.left + block.width > input.source_width + 1.0
            || block.top + block.height > input.source_height + 1.0
            || !valid_color(&block.text_color)
            || !valid_color(&block.background_color)
        {
            bail!("translation block text or geometry is invalid");
        }
    }
    if let Some(source) = &input.source_attachment {
        if source.attachment_id != "neuro.official/ocr.result"
            || source.revision > MAX_SAFE_REVISION
            || source.digest.as_ref().is_some_and(|value| {
                value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            bail!("translation source attachment is invalid");
        }
    }
    Ok(())
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
