use loom_ocr::{OcrQualityMode, OcrRegion};
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

pub(crate) fn parse_region(input: &Value) -> Result<Option<OcrRegion>, CommandFailure> {
    let Some(value) = input.get("region") else {
        return Ok(None);
    };
    let read_coordinate = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|coordinate| u32::try_from(coordinate).ok())
            .ok_or_else(|| invalid_region("OCR region coordinates must be unsigned integers"))
    };
    let region = OcrRegion {
        left: read_coordinate("left")?,
        top: read_coordinate("top")?,
        width: read_coordinate("width")?,
        height: read_coordinate("height")?,
    };
    if region.width == 0 || region.height == 0 {
        return Err(invalid_region("OCR region must not be empty"));
    }
    Ok(Some(region))
}

fn invalid_mode(message: &str) -> CommandFailure {
    CommandFailure::new(CapabilityErrorCode::InvalidInput, message, false)
}

fn invalid_region(message: &str) -> CommandFailure {
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

    #[test]
    fn parses_an_optional_pixel_region() {
        assert_eq!(parse_region(&Value::Null).unwrap(), None);
        assert_eq!(
            parse_region(&json!({
                "region": { "left": 10, "top": 20, "width": 30, "height": 40 }
            }))
            .unwrap(),
            Some(OcrRegion {
                left: 10,
                top: 20,
                width: 30,
                height: 40,
            })
        );
    }

    #[test]
    fn rejects_incomplete_or_empty_regions() {
        assert!(parse_region(&json!({ "region": { "left": 0 } })).is_err());
        assert!(parse_region(&json!({
            "region": { "left": 0, "top": 0, "width": 0, "height": 10 }
        }))
        .is_err());
        assert!(parse_region(&json!({
            "region": { "left": -1, "top": 0, "width": 10, "height": 10 }
        }))
        .is_err());
    }
}
