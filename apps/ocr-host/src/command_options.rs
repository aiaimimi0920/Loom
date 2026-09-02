use loom_ocr::OcrQualityMode;
use loom_protocol::CapabilityErrorCode;
use serde_json::Value;

use crate::commands::CommandFailure;

/// Parses optional command input without changing the behavior of legacy callers.
pub(crate) fn parse_quality_mode(input: &Value) -> Result<OcrQualityMode, CommandFailure> {
    let Some(mode) = input.get("mode") else {
        return Ok(OcrQualityMode::Auto);
    };
    let Some(mode) = mode.as_str() else {
        return Err(invalid_mode("OCR mode must be a string"));
    };
    match mode {
        "quick" => Ok(OcrQualityMode::Quick),
        "auto" => Ok(OcrQualityMode::Auto),
        "highAccuracy" => Ok(OcrQualityMode::HighAccuracy),
        _ => Err(invalid_mode("OCR mode is unsupported")),
    }
}

fn invalid_mode(message: &str) -> CommandFailure {
    CommandFailure::new(CapabilityErrorCode::InvalidInput, message, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_mode_keeps_auto_compatibility() {
        assert_eq!(
            parse_quality_mode(&Value::Null).expect("default mode"),
            OcrQualityMode::Auto
        );
    }

    #[test]
    fn accepts_declared_modes_only() {
        for (value, expected) in [
            ("quick", OcrQualityMode::Quick),
            ("auto", OcrQualityMode::Auto),
            ("highAccuracy", OcrQualityMode::HighAccuracy),
        ] {
            assert_eq!(
                parse_quality_mode(&json!({ "mode": value })).expect("declared mode"),
                expected
            );
        }
    }

    #[test]
    fn rejects_malformed_or_unknown_modes() {
        assert!(parse_quality_mode(&json!({ "mode": 1 })).is_err());
        assert!(parse_quality_mode(&json!({ "mode": "turbo" })).is_err());
    }
}
