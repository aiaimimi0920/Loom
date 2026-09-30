use crate::types::{OcrGeometrySource, OcrLineGeometry, OcrMetricPoint, OcrPoint};

const ESTIMATED_BASELINE_INSET_RATIO: f32 = 0.18;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

impl Bounds {
    /// Coordinates are inclusive because they address source image pixels.
    pub(crate) fn width(self) -> u32 {
        self.max_x.saturating_sub(self.min_x).saturating_add(1)
    }

    pub(crate) fn height(self) -> u32 {
        self.max_y.saturating_sub(self.min_y).saturating_add(1)
    }
}

pub(crate) fn block_bounds(points: &[OcrPoint], width: u32, height: u32) -> Option<Bounds> {
    if points.is_empty() || width == 0 || height == 0 {
        return None;
    }

    let max_valid_x = width - 1;
    let max_valid_y = height - 1;
    let min_x = points.iter().map(|point| point.x).min()?.min(max_valid_x);
    let min_y = points.iter().map(|point| point.y).min()?.min(max_valid_y);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .max()?
        .min(max_valid_x)
        .max(min_x);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .max()?
        .min(max_valid_y)
        .max(min_y);

    Some(Bounds {
        min_x,
        max_x,
        min_y,
        max_y,
    })
}

fn point(point: OcrPoint) -> OcrMetricPoint {
    OcrMetricPoint {
        x: point.x as f32,
        y: point.y as f32,
    }
}

fn squared_length(start: OcrMetricPoint, end: OcrMetricPoint) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    dx * dx + dy * dy
}

fn midpoint(start: OcrMetricPoint, end: OcrMetricPoint) -> OcrMetricPoint {
    OcrMetricPoint {
        x: (start.x + end.x) / 2.0,
        y: (start.y + end.y) / 2.0,
    }
}

/// Estimates a baseline from the lower of the two long edges of an OCR quad.
///
/// The inset moves the detector's padded bottom edge toward the quad centre.
/// This remains an explicitly derived estimate; no character geometry is
/// invented when the recognizer did not provide it.
pub(crate) fn estimate_line_geometry(points: &[OcrPoint]) -> Option<OcrLineGeometry> {
    if points.len() != 4 {
        return None;
    }
    let quad = [
        point(points[0]),
        point(points[1]),
        point(points[2]),
        point(points[3]),
    ];
    let longest_edge = (0..4).max_by(|left, right| {
        squared_length(quad[*left], quad[(*left + 1) % 4])
            .total_cmp(&squared_length(quad[*right], quad[(*right + 1) % 4]))
    })?;
    let opposite_edge = (longest_edge + 2) % 4;
    let longest_midpoint = midpoint(quad[longest_edge], quad[(longest_edge + 1) % 4]);
    let opposite_midpoint = midpoint(quad[opposite_edge], quad[(opposite_edge + 1) % 4]);
    let lower_edge = if longest_midpoint.y >= opposite_midpoint.y {
        longest_edge
    } else {
        opposite_edge
    };
    let upper_edge = (lower_edge + 2) % 4;
    let lower_midpoint = midpoint(quad[lower_edge], quad[(lower_edge + 1) % 4]);
    let upper_midpoint = midpoint(quad[upper_edge], quad[(upper_edge + 1) % 4]);
    let toward_center_x = upper_midpoint.x - lower_midpoint.x;
    let toward_center_y = upper_midpoint.y - lower_midpoint.y;
    let edge_separation =
        (toward_center_x * toward_center_x + toward_center_y * toward_center_y).sqrt();
    let edge_length = squared_length(quad[lower_edge], quad[(lower_edge + 1) % 4]).sqrt();
    if !edge_length.is_finite() || edge_length <= f32::EPSILON {
        return None;
    }

    let inset_scale = if edge_separation > f32::EPSILON {
        ESTIMATED_BASELINE_INSET_RATIO
    } else {
        0.0
    };
    let inset_x = toward_center_x * inset_scale;
    let inset_y = toward_center_y * inset_scale;
    let mut start = OcrMetricPoint {
        x: quad[lower_edge].x + inset_x,
        y: quad[lower_edge].y + inset_y,
    };
    let mut end = OcrMetricPoint {
        x: quad[(lower_edge + 1) % 4].x + inset_x,
        y: quad[(lower_edge + 1) % 4].y + inset_y,
    };
    if start.x > end.x || (start.x == end.x && start.y > end.y) {
        std::mem::swap(&mut start, &mut end);
    }
    let angle_degrees = (end.y - start.y).atan2(end.x - start.x).to_degrees();
    if !angle_degrees.is_finite() {
        return None;
    }

    Some(OcrLineGeometry {
        baseline: [start, end],
        angle_degrees,
        source: OcrGeometrySource::EstimatedFromRapidOcrLineQuad,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusive_bounds_retain_a_single_pixel_punctuation_box() {
        let bounds = block_bounds(&[OcrPoint { x: 4, y: 7 }], 10, 10).expect("bounds");
        assert_eq!(bounds.width(), 1);
        assert_eq!(bounds.height(), 1);
    }

    #[test]
    fn derives_an_inset_horizontal_baseline_from_a_line_quad() {
        let geometry = estimate_line_geometry(&[
            OcrPoint { x: 10, y: 20 },
            OcrPoint { x: 110, y: 20 },
            OcrPoint { x: 110, y: 50 },
            OcrPoint { x: 10, y: 50 },
        ])
        .expect("line geometry");

        assert!((geometry.baseline[0].x - 10.0).abs() < 0.01);
        assert!((geometry.baseline[1].x - 110.0).abs() < 0.01);
        assert!((geometry.baseline[0].y - 44.6).abs() < 0.01);
        assert!(geometry.angle_degrees.abs() < 0.01);
    }

    #[test]
    fn derives_the_quad_skew_angle_without_claiming_model_baseline_data() {
        let geometry = estimate_line_geometry(&[
            OcrPoint { x: 10, y: 20 },
            OcrPoint { x: 110, y: 25 },
            OcrPoint { x: 110, y: 55 },
            OcrPoint { x: 10, y: 50 },
        ])
        .expect("line geometry");

        assert!((geometry.angle_degrees - 2.862).abs() < 0.01);
        assert_eq!(
            geometry.source,
            OcrGeometrySource::EstimatedFromRapidOcrLineQuad
        );
    }
}
