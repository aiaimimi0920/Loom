use paddle_ocr_rs::{
    angle_net::AngleNet,
    base_net::BaseNet,
    db_net::DbNet,
    ocr_result::{Point, TextBox},
    ocr_utils::OcrUtils,
    scale_param::ScaleParam,
};

use crate::ctc_decode::DecodedLine;
use crate::ctc_recognizer::SessionBuilderFn;
use crate::recognition_rescue::RecognitionRescue;
use crate::span_geometry::RecognitionAxis;
use crate::types::OcrPoint;
use crate::{OcrError, OcrQualityMode, OcrResult};

const DETECTION_PADDING: u32 = 50;
const MIN_DETECTION_LONG_SIDE: u32 = 1_600;
const MAX_DETECTION_LONG_SIDE: u32 = 4_096;
const MAX_TEXT_BOXES: usize = 512;
const MAX_RESCUE_LINES: usize = 64;
const BOX_SCORE_THRESHOLD: f32 = 0.5;
const BOX_THRESHOLD: f32 = 0.3;
const UNCLIP_RATIO: f32 = 2.0;
const ANGLE_ROLLBACK_THRESHOLD: f32 = 0.9;

#[derive(Debug)]
pub(crate) struct RawOcrBlock {
    pub box_points: Vec<OcrPoint>,
    pub box_score: f32,
    pub line: DecodedLine,
    pub axis: RecognitionAxis,
    pub reverse_axis: bool,
}

#[derive(Debug)]
pub(crate) struct AlignedOcrCore {
    detector: DbNet,
    angle: AngleNet,
    recognition: RecognitionRescue,
}

impl AlignedOcrCore {
    pub(crate) fn from_models(
        det_model: &[u8],
        cls_model: &[u8],
        rec_model: &[u8],
        fallback_rec_model: Option<Vec<u8>>,
        builder_fn: SessionBuilderFn,
    ) -> OcrResult<Self> {
        let mut detector = DbNet::new();
        detector
            .init_model_from_memory(det_model, 0, Some(builder_fn))
            .map_err(|error| OcrError::Init(error.to_string()))?;
        let mut angle = AngleNet::new();
        angle
            .init_model_from_memory(cls_model, 0, Some(builder_fn))
            .map_err(|error| OcrError::Init(error.to_string()))?;
        Ok(Self {
            detector,
            angle,
            recognition: RecognitionRescue::new(rec_model, fallback_rec_model, builder_fn)?,
        })
    }

    pub(crate) fn detect(
        &mut self,
        image: &image::RgbImage,
        detect_angle: bool,
        quality_mode: OcrQualityMode,
    ) -> OcrResult<Vec<RawOcrBlock>> {
        if image.width() == 0 || image.height() == 0 {
            return Err(OcrError::InvalidImage(
                "image dimensions must be positive".to_owned(),
            ));
        }
        let padded = OcrUtils::make_padding(image, DETECTION_PADDING)
            .map_err(|error| OcrError::Detect(error.to_string()))?;
        let target_size = detection_target_size(padded.width(), padded.height());
        let scale = ScaleParam::get_scale_param(&padded, target_size);
        let mut boxes = self
            .detector
            .get_text_boxes(
                &padded,
                &scale,
                BOX_SCORE_THRESHOLD,
                BOX_THRESHOLD,
                UNCLIP_RATIO,
            )
            .map_err(|error| OcrError::Detect(error.to_string()))?;
        boxes.retain(valid_text_box);
        boxes.truncate(MAX_TEXT_BOXES);

        let part_images = OcrUtils::get_part_images(&padded, &boxes);
        let angles = self
            .angle
            .get_angles(&part_images, detect_angle, false)
            .map_err(|error| OcrError::Detect(error.to_string()))?;
        let mut blocks = Vec::with_capacity(boxes.len());
        for (index, ((text_box, mut part_image), angle)) in
            boxes.into_iter().zip(part_images).zip(angles).enumerate()
        {
            let axis = recognition_axis(&text_box);
            let original = (angle.index == 1).then(|| part_image.clone());
            let mut reverse_axis = angle.index == 1;
            if reverse_axis {
                OcrUtils::mat_rotate_clock_wise_180(&mut part_image);
            }
            let allow_rescue = !matches!(quality_mode, OcrQualityMode::Quick)
                && index < rescue_limit(quality_mode);
            let enhanced_fallback = matches!(quality_mode, OcrQualityMode::HighAccuracy);
            let mut line =
                self.recognition
                    .recognize(&part_image, allow_rescue, enhanced_fallback)?;
            if line.text_score.is_nan() || line.text_score < ANGLE_ROLLBACK_THRESHOLD {
                if let Some(original) = original {
                    // The first orientation already performed bounded rescue. The rollback
                    // pass only verifies the opposite orientation, avoiding duplicate
                    // contrast/fallback inference for the same detector box.
                    line = self.recognition.recognize(&original, false, false)?;
                    reverse_axis = false;
                }
            }
            blocks.push(RawOcrBlock {
                box_points: remove_padding(&text_box.points, image.width(), image.height()),
                box_score: text_box.score,
                line,
                axis,
                reverse_axis,
            });
        }
        Ok(blocks)
    }
}

fn rescue_limit(quality_mode: OcrQualityMode) -> usize {
    match quality_mode {
        OcrQualityMode::Quick => 0,
        OcrQualityMode::Auto => MAX_RESCUE_LINES,
        OcrQualityMode::HighAccuracy => MAX_TEXT_BOXES,
    }
}

fn detection_target_size(width: u32, height: u32) -> u32 {
    width
        .max(height)
        .clamp(MIN_DETECTION_LONG_SIDE, MAX_DETECTION_LONG_SIDE)
}

fn valid_text_box(text_box: &TextBox) -> bool {
    if text_box.points.len() != 4 {
        return false;
    }
    let min_x = text_box
        .points
        .iter()
        .map(|point| point.x)
        .min()
        .unwrap_or(0);
    let max_x = text_box
        .points
        .iter()
        .map(|point| point.x)
        .max()
        .unwrap_or(0);
    let min_y = text_box
        .points
        .iter()
        .map(|point| point.y)
        .min()
        .unwrap_or(0);
    let max_y = text_box
        .points
        .iter()
        .map(|point| point.y)
        .max()
        .unwrap_or(0);
    max_x > min_x && max_y > min_y && text_box.score.is_finite()
}

fn edge_length(start: Point, end: Point) -> f32 {
    let dx = start.x as f32 - end.x as f32;
    let dy = start.y as f32 - end.y as f32;
    (dx * dx + dy * dy).sqrt()
}

fn recognition_axis(text_box: &TextBox) -> RecognitionAxis {
    let width = edge_length(text_box.points[0], text_box.points[1]);
    let height = edge_length(text_box.points[0], text_box.points[3]);
    if height >= width * 1.5 {
        RecognitionAxis::Vertical
    } else {
        RecognitionAxis::Horizontal
    }
}

fn remove_padding(points: &[Point], width: u32, height: u32) -> Vec<OcrPoint> {
    let max_x = width.saturating_sub(1);
    let max_y = height.saturating_sub(1);
    points
        .iter()
        .map(|point| OcrPoint {
            x: point.x.saturating_sub(DETECTION_PADDING).min(max_x),
            y: point.y.saturating_sub(DETECTION_PADDING).min(max_y),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscales_small_detector_inputs_and_caps_large_desktops() {
        assert_eq!(detection_target_size(900, 600), 1_600);
        assert_eq!(detection_target_size(2_000, 1_000), 2_000);
        assert_eq!(detection_target_size(12_000, 2_000), 4_096);
    }

    #[test]
    fn padding_removal_is_saturating_and_stays_inside_the_source_image() {
        let points = [
            Point { x: 10, y: 20 },
            Point { x: 180, y: 20 },
            Point { x: 180, y: 140 },
            Point { x: 10, y: 140 },
        ];
        assert_eq!(
            remove_padding(&points, 100, 80),
            vec![
                OcrPoint { x: 0, y: 0 },
                OcrPoint { x: 99, y: 0 },
                OcrPoint { x: 99, y: 79 },
                OcrPoint { x: 0, y: 79 },
            ]
        );
    }

    #[test]
    fn quality_modes_use_bounded_rescue_limits() {
        assert_eq!(rescue_limit(OcrQualityMode::Quick), 0);
        assert_eq!(rescue_limit(OcrQualityMode::Auto), MAX_RESCUE_LINES);
        assert_eq!(rescue_limit(OcrQualityMode::HighAccuracy), MAX_TEXT_BOXES);
    }

    #[test]
    fn thread_count_is_positive_and_capped() {
        let count = crate::ocr_thread_count();
        assert!((1..=8).contains(&count));
    }
}
