use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OcrPoint {
    pub x: u32,
    pub y: u32,
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
