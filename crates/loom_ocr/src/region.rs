use crate::{EnhancedTextBlock, OcrError, OcrRegion, OcrResult};

pub(crate) fn validate(
    region: OcrRegion,
    image_width: u32,
    image_height: u32,
) -> OcrResult<OcrRegion> {
    let right = region.left.checked_add(region.width);
    let bottom = region.top.checked_add(region.height);
    if region.width == 0
        || region.height == 0
        || right.is_none_or(|value| value > image_width)
        || bottom.is_none_or(|value| value > image_height)
    {
        return Err(OcrError::InvalidImage(
            "OCR region is empty or outside the source image".to_owned(),
        ));
    }
    Ok(region)
}

pub(crate) fn translate_blocks(
    blocks: &mut [EnhancedTextBlock],
    region: OcrRegion,
    image_width: u32,
    image_height: u32,
) {
    for block in blocks {
        for point in &mut block.box_points {
            point.x = point.x.saturating_add(region.left).min(image_width);
            point.y = point.y.saturating_add(region.top).min(image_height);
        }
        if let Some(geometry) = &mut block.line_geometry {
            for point in &mut geometry.baseline {
                translate_metric(point, region, image_width, image_height);
            }
        }
        for span in block
            .character_spans
            .iter_mut()
            .chain(block.word_spans.iter_mut())
        {
            for point in &mut span.box_points {
                translate_metric(point, region, image_width, image_height);
            }
        }
    }
}

fn translate_metric(
    point: &mut crate::OcrMetricPoint,
    region: OcrRegion,
    image_width: u32,
    image_height: u32,
) {
    point.x = (point.x + region.left as f32).clamp(0.0, image_width as f32);
    point.y = (point.y + region.top as f32).clamp(0.0, image_height as f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OcrGeometrySource, OcrLineGeometry, OcrMetricPoint, OcrPoint};

    #[test]
    fn rejects_empty_overflowing_and_out_of_bounds_regions() {
        assert!(validate(region(0, 0, 0, 10), 100, 100).is_err());
        assert!(validate(region(u32::MAX, 0, 2, 10), 100, 100).is_err());
        assert!(validate(region(90, 90, 11, 10), 100, 100).is_err());
        assert_eq!(
            validate(region(90, 90, 10, 10), 100, 100).unwrap(),
            region(90, 90, 10, 10)
        );
    }

    #[test]
    fn translates_line_and_span_geometry_back_to_source_coordinates() {
        let mut blocks = [fixture_block()];
        translate_blocks(&mut blocks, region(20, 30, 40, 20), 100, 80);
        let block = &blocks[0];
        assert_eq!(block.box_points[0], OcrPoint { x: 21, y: 32 });
        assert_eq!(block.line_geometry.as_ref().unwrap().baseline[1].x, 30.0);
        assert_eq!(block.character_spans[0].box_points[0].y, 33.0);
    }

    const fn region(left: u32, top: u32, width: u32, height: u32) -> OcrRegion {
        OcrRegion {
            left,
            top,
            width,
            height,
        }
    }

    fn fixture_block() -> EnhancedTextBlock {
        EnhancedTextBlock {
            box_points: vec![OcrPoint { x: 1, y: 2 }, OcrPoint { x: 10, y: 8 }],
            box_score: 1.0,
            text: "A".to_owned(),
            text_score: 1.0,
            color_hex: "#ffffff".to_owned(),
            bg_color_hex: "#000000".to_owned(),
            raw_text: None,
            confidence: None,
            line_geometry: Some(OcrLineGeometry {
                baseline: [
                    OcrMetricPoint { x: 1.0, y: 8.0 },
                    OcrMetricPoint { x: 10.0, y: 8.0 },
                ],
                angle_degrees: 0.0,
                source: OcrGeometrySource::EstimatedFromRapidOcrLineQuad,
            }),
            character_spans: vec![crate::OcrTextSpan {
                text: "A".to_owned(),
                box_points: [OcrMetricPoint { x: 1.0, y: 3.0 }; 4],
                score: 1.0,
                source: crate::OcrTextSpanSource::CtcAlignedFromRecognitionTimesteps,
            }],
            word_spans: Vec::new(),
        }
    }
}
