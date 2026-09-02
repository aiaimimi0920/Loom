use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OcrPoint {
    pub x: u32,
    pub y: u32,
}

/// Pixel coordinates in the original image for an optional OCR subregion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRegion {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrMetricPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrGeometrySource {
    EstimatedFromRapidOcrLineQuad,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrTextSpanSource {
    CtcAlignedFromRecognitionTimesteps,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OcrTextConfidenceSource {
    CtcDecodedSymbolScores,
}

/// Bounded diagnostics for the decoded symbols; values are not calibrated probabilities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrTextConfidence {
    pub mean_symbol_score: f32,
    pub minimum_symbol_score: f32,
    pub symbol_count: u32,
    pub recovered_symbol_count: u32,
    pub source: OcrTextConfidenceSource,
}

/// Recognition geometry projected from CTC timesteps into the detected line quad.
/// This is model-aligned evidence, not an independently detected glyph outline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrTextSpan {
    pub text: String,
    pub box_points: [OcrMetricPoint; 4],
    pub score: f32,
    pub source: OcrTextSpanSource,
}

/// Layout evidence derived from RapidOCR's detected line quadrilateral.
///
/// RapidOCR does not expose a typographic baseline through the Rust API. The
/// source tag prevents downstream renderers from treating this estimate as a
/// model-detected character or word box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLineGeometry {
    pub baseline: [OcrMetricPoint; 2],
    pub angle_degrees: f32,
    pub source: OcrGeometrySource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedTextBlock {
    pub box_points: Vec<OcrPoint>,
    pub box_score: f32,
    pub text: String,
    pub text_score: f32,
    pub color_hex: String,
    pub bg_color_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<OcrTextConfidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_geometry: Option<OcrLineGeometry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub character_spans: Vec<OcrTextSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_spans: Vec<OcrTextSpan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrDetectResult {
    pub text_blocks: Vec<EnhancedTextBlock>,
    pub scale_factor: f32,
    pub full_text: String,
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn metric_point(x: f32, y: f32) -> OcrMetricPoint {
        OcrMetricPoint { x, y }
    }

    #[test]
    fn layout_evidence_serializes_with_the_capability_contract_shape() {
        let span = OcrTextSpan {
            text: "A".to_owned(),
            box_points: [
                metric_point(10.0, 20.0),
                metric_point(24.0, 20.0),
                metric_point(24.0, 50.0),
                metric_point(10.0, 50.0),
            ],
            score: 0.97,
            source: OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
        };
        let result = OcrDetectResult {
            text_blocks: vec![EnhancedTextBlock {
                box_points: vec![
                    OcrPoint { x: 10, y: 20 },
                    OcrPoint { x: 90, y: 20 },
                    OcrPoint { x: 90, y: 50 },
                    OcrPoint { x: 10, y: 50 },
                ],
                box_score: 0.99,
                text: "Alt+2".to_owned(),
                text_score: 0.98,
                color_hex: "#ffffff".to_owned(),
                bg_color_hex: "#101010".to_owned(),
                raw_text: Some("A1t+2".to_owned()),
                confidence: Some(OcrTextConfidence {
                    mean_symbol_score: 0.98,
                    minimum_symbol_score: 0.96,
                    symbol_count: 5,
                    recovered_symbol_count: 0,
                    source: OcrTextConfidenceSource::CtcDecodedSymbolScores,
                }),
                line_geometry: Some(OcrLineGeometry {
                    baseline: [metric_point(10.0, 44.6), metric_point(90.0, 44.6)],
                    angle_degrees: 0.0,
                    source: OcrGeometrySource::EstimatedFromRapidOcrLineQuad,
                }),
                character_spans: vec![span.clone()],
                word_spans: vec![OcrTextSpan {
                    text: "A1t".to_owned(),
                    ..span
                }],
            }],
            scale_factor: 1.0,
            full_text: "Alt+2".to_owned(),
            width: 100,
            height: 60,
        };

        let serialized = serde_json::to_value(result).expect("serialize OCR result");
        let block = &serialized["textBlocks"][0];
        assert_eq!(block["rawText"], "A1t+2");
        assert_eq!(block["lineGeometry"]["baseline"][1]["x"], 90.0);
        assert_eq!(block["characterSpans"][0]["boxPoints"][1]["x"], 24.0);
        assert_eq!(block["wordSpans"][0]["text"], "A1t");
        let minimum_score = block["confidence"]["minimumSymbolScore"]
            .as_f64()
            .expect("serialized score");
        assert!((minimum_score - 0.96).abs() < 0.0001);
        assert!(block.get("raw_text").is_none());
    }
}
