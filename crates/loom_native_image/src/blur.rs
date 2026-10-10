//! Exact box average in O(pixels), independent of the requested radius.
use image::{DynamicImage, Rgba, RgbaImage};

pub(crate) fn box_blur(
    img: &DynamicImage,
    radius: u32,
    check: impl Fn() -> Result<(), String>,
) -> Result<DynamicImage, String> {
    check()?;
    let src = img.to_rgba8();
    let (width, height) = src.dimensions();
    let mut output = RgbaImage::new(width, height);
    let radius = radius.max(1).min(width.max(height));
    let mut columns = vec![[0_u64; 4]; width as usize];
    // Keep unrounded vertical sums: separable rounded averages would change pixels.
    for y in 0..height.min(radius.saturating_add(1)) {
        check()?;
        for x in 0..width {
            for channel in 0..4 {
                columns[x as usize][channel] += u64::from(src.get_pixel(x, y)[channel]);
            }
        }
    }
    for y in 0..height {
        check()?;
        let rows =
            u64::from(y.saturating_add(radius).min(height - 1) - y.saturating_sub(radius) + 1);
        let mut sums = [0_u64; 4];
        for column in columns
            .iter()
            .take(width.min(radius.saturating_add(1)) as usize)
        {
            for channel in 0..4 {
                sums[channel] += column[channel];
            }
        }
        for x in 0..width {
            let count = rows
                * u64::from(x.saturating_add(radius).min(width - 1) - x.saturating_sub(radius) + 1);
            output.put_pixel(x, y, Rgba(sums.map(|sum| (sum / count) as u8)));
            for channel in 0..4 {
                if x >= radius {
                    sums[channel] -= columns[(x - radius) as usize][channel];
                }
                if let Some(next) = x.checked_add(radius + 1).filter(|&next| next < width) {
                    sums[channel] += columns[next as usize][channel];
                }
            }
        }
        for x in 0..width {
            for channel in 0..4 {
                if y >= radius {
                    columns[x as usize][channel] -=
                        u64::from(src.get_pixel(x, y - radius)[channel]);
                }
                if let Some(next) = y.checked_add(radius + 1).filter(|&next| next < height) {
                    columns[x as usize][channel] += u64::from(src.get_pixel(x, next)[channel]);
                }
            }
        }
    }
    Ok(DynamicImage::ImageRgba8(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn sliding_average_matches_exact_box_on_edges_and_alpha() {
        let source = RgbaImage::from_fn(7, 5, |x, y| {
            Rgba([
                (x * 31) as u8,
                (y * 41) as u8,
                (x * y * 7) as u8,
                (x + y * 3) as u8,
            ])
        });
        for radius in [0, 1, 2, 5, 128, u32::MAX] {
            let result = box_blur(&DynamicImage::ImageRgba8(source.clone()), radius, || Ok(()))
                .unwrap()
                .to_rgba8();
            for (x, y, pixel) in result.enumerate_pixels() {
                let mut sums = [0_u64; 4];
                let mut count = 0;
                for (sx, sy, sample) in source.enumerate_pixels() {
                    if sx.abs_diff(x) <= radius.max(1) && sy.abs_diff(y) <= radius.max(1) {
                        count += 1;
                        for channel in 0..4 {
                            sums[channel] += u64::from(sample[channel]);
                        }
                    }
                }
                assert_eq!(*pixel, Rgba(sums.map(|sum| (sum / count) as u8)));
            }
        }
    }

    #[test]
    fn blur_checks_cancellation_during_work_not_just_at_entry() {
        let checks = Cell::new(0);
        let image = DynamicImage::new_rgba8(32, 32);
        let result = box_blur(&image, 2, || {
            checks.set(checks.get() + 1);
            if checks.get() == 8 {
                Err("cancelled".to_owned())
            } else {
                Ok(())
            }
        });
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(checks.get(), 8);
    }
}
