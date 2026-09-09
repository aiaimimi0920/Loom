//! Bounded QR/barcode decoding owned by the optional OCR capability runtime.

use image::load_from_memory;
use rxing::helpers::detect_multiple_in_luma_with_hints;
use rxing::DecodeHints;
use serde::{Deserialize, Serialize};

const MAX_RESULTS: usize = 32;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
const MAX_EXTERNAL_URL_BYTES: usize = 8 * 1024;
const MAX_POINTS: usize = 16;
const MAX_PIXELS: u64 = 50_000_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeResult {
    pub id: String,
    pub format: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub points: Vec<CodePoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<CodeBounds>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeScanResult {
    pub width: u32,
    pub height: u32,
    pub results: Vec<CodeResult>,
}

pub fn decode(image_bytes: &[u8]) -> Result<CodeScanResult, String> {
    let image = load_from_memory(image_bytes)
        .map_err(|error| format!("Code image could not be decoded: {error}"))?;
    let width = image.width();
    let height = image.height();
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "Code image dimensions overflow".to_owned())?;
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(format!(
            "Code image is outside the {MAX_PIXELS} pixel limit"
        ));
    }

    let mut hints = DecodeHints::default();
    hints.AlsoInverted = Some(true);
    let decoded = match detect_multiple_in_luma_with_hints(
        image.to_luma8().into_raw(),
        width,
        height,
        &mut hints,
    ) {
        Ok(results) => results,
        Err(rxing::Exceptions::NotFoundException(_)) => Vec::new(),
        Err(error) => return Err(format!("Code decoder failed: {error}")),
    };

    let mut results = decoded
        .into_iter()
        .filter_map(|result| {
            let text = result.getText().trim();
            if text.is_empty() || text.len() > MAX_PAYLOAD_BYTES {
                return None;
            }
            let (points, bounds) = bounded_points(result.getPoints());
            Some(CodeResult {
                id: String::new(),
                format: result.getBarcodeFormat().to_string(),
                text: text.to_owned(),
                url: classify_https_url(text),
                points,
                bounds,
            })
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        position(left)
            .partial_cmp(&position(right))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(MAX_RESULTS);
    for (index, result) in results.iter_mut().enumerate() {
        result.id = format!("code-{}", index + 1);
    }
    Ok(CodeScanResult {
        width,
        height,
        results,
    })
}

fn bounded_points(points: &[rxing::Point]) -> (Vec<CodePoint>, Option<CodeBounds>) {
    let points = points
        .iter()
        .take(MAX_POINTS)
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| CodePoint {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    let first = match points.first() {
        Some(point) => point,
        None => return (points, None),
    };
    let bounds = points.iter().skip(1).fold(
        CodeBounds {
            left: first.x,
            top: first.y,
            right: first.x,
            bottom: first.y,
        },
        |mut bounds, point| {
            bounds.left = bounds.left.min(point.x);
            bounds.top = bounds.top.min(point.y);
            bounds.right = bounds.right.max(point.x);
            bounds.bottom = bounds.bottom.max(point.y);
            bounds
        },
    );
    (points, Some(bounds))
}

fn position(result: &CodeResult) -> (f32, f32) {
    result
        .bounds
        .as_ref()
        .map(|bounds| (bounds.top, bounds.left))
        .unwrap_or((f32::MAX, f32::MAX))
}

pub(crate) fn classify_https_url(text: &str) -> Option<String> {
    let candidate = text.trim();
    // Mirrors `loom_protocol::is_safe_external_https_url`, including its byte
    // budget: a longer link would be surfaced as openable and then rejected by
    // the host at click time.
    if candidate.is_empty() || candidate.len() > MAX_EXTERNAL_URL_BYTES {
        return None;
    }
    if candidate
        .chars()
        .any(|character| character.is_control() || character.is_whitespace() || character == '\\')
    {
        return None;
    }
    let authority_and_path = candidate.strip_prefix("https://")?;
    let authority = authority_and_path
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(candidate.to_owned())
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::{classify_https_url, decode};

    #[test]
    fn only_plain_https_urls_are_classified() {
        assert_eq!(
            classify_https_url(" https://example.com/path "),
            Some("https://example.com/path".to_owned())
        );
        assert_eq!(classify_https_url("http://example.com"), None);
        assert_eq!(classify_https_url("javascript:alert(1)"), None);
        assert_eq!(classify_https_url("https://user@example.com"), None);
        assert_eq!(classify_https_url("https:///missing-host"), None);
        assert_eq!(classify_https_url("https://example.com/unsafe path"), None);
        assert_eq!(classify_https_url("https://example.com/path\nnext"), None);
        assert_eq!(classify_https_url("https://example.com/path\\next"), None);
    }

    #[test]
    fn decodes_qr_payload_and_reports_geometry() {
        let fixture = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAUoAAAFKAQAAAABTUiuoAAAB8UlEQVR4nO1bwW3DMAw8ygL6tIEO0FGcDbqyPUo2sP4OWJCUnTRAAbWPmoh5D0VS7kGAOOlIJcRoxJxamUBQU1AR1GakoCKozUhBhYAqsswygEJEFxkUF0+xNiG9NHVkwQLwJMm79Czuo9NddhZrC9JrU8suIdwIKCIwE112F+t5qflpTegX0DyA/imAFFT8nTpeiTAuxwXwMxLOTs31s5fbqUgR/MF1kCV7ijWdnopHH4GOMS73YdtWDzIdHms6PTXr+CCheehswt+VheNjTUGFQO2gFli65EmM4ay11lC/hZdYz0xFPQm1wLIqS/a08rK91SgcJyGcUJmvRMxXORf7VY5D2S1EPJW3qI7daosnyZYYDJ2ZykJbTqiwVOzG8DE9doOFJ3To4LuqLV2qeZccYeS4t/xpa5H0LJ0pSnNkN9je2uXIFjxli3nLkc5sqTdYZMtlL0MgfsO0ZQhtuXQZiu2lSzpP96VgOjzWFFR8fzu2UqvWW9ljrGem4klRdblra6dwaAvu3o7JmoXa0CBtG4q19xVrA9I5qGzdwZKtMGZebvErGrcv/ZglNfWlv1sxf66ZncSaTk/Nz2/HmiOUdxkGSVn8isbz2zFLTSzV8aSl1taD4nAZOJxK8a8FBLUZKagIajNSUBHUZvzCZXwBqA9sRqfvC2QAAAAASUVORK5CYII=").unwrap();
        let scan = decode(&fixture).expect("QR fixture should decode");
        assert_eq!((scan.width, scan.height), (330, 330));
        assert_eq!(scan.results.len(), 1);
        assert_eq!(scan.results[0].text, "https://example.com/hook");
        assert!(scan.results[0].bounds.is_some());
    }
}
