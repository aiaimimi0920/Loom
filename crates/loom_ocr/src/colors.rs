use crate::geometry::Bounds;

#[path = "background_palette.rs"]
mod background_palette;
use background_palette::{distance, Palette};

const LUMINANCE_BINS: usize = 256;

pub(crate) fn estimate_text_and_background_color(
    image_buffer: &image::RgbImage,
    bounds: Bounds,
) -> (String, String) {
    if image_buffer.width() == 0
        || image_buffer.height() == 0
        || bounds.min_x > bounds.max_x
        || bounds.min_y > bounds.max_y
        || bounds.max_x >= image_buffer.width()
        || bounds.max_y >= image_buffer.height()
    {
        return ("#000000".to_owned(), "#ffffff".to_owned());
    }
    // At most 16384 source pixels per block; retain RGB modes separately from
    // the luminance split used to estimate text, including antialiased strokes.
    let mut palette = Palette::new();
    let mut bin_counts = [0_u64; LUMINANCE_BINS];
    let mut bin_colors = [[0_u64; 3]; LUMINANCE_BINS];
    let mut total_lum: u64 = 0;
    let mut total_color = [0_u64; 3];
    let mut count: u64 = 0;

    let step_x = (bounds.max_x - bounds.min_x + 1).div_ceil(128) as usize;
    let step_y = (bounds.max_y - bounds.min_y + 1).div_ceil(128) as usize;
    for y in (bounds.min_y..=bounds.max_y).step_by(step_y) {
        for x in (bounds.min_x..=bounds.max_x).step_by(step_x) {
            let pixel = image_buffer.get_pixel(x, y);
            palette.add(pixel.0);
            let lum = luminance(pixel) as u64;
            let bin = (lum as usize).min(LUMINANCE_BINS - 1);
            bin_counts[bin] += 1;
            bin_colors[bin][0] += pixel[0] as u64;
            bin_colors[bin][1] += pixel[1] as u64;
            bin_colors[bin][2] += pixel[2] as u64;
            total_lum += lum;
            total_color[0] += pixel[0] as u64;
            total_color[1] += pixel[1] as u64;
            total_color[2] += pixel[2] as u64;
            count += 1;
        }
    }

    if count == 0 {
        return ("#000000".to_owned(), "#ffffff".to_owned());
    }

    let avg_lum = (total_lum / count) as usize;
    let mut dark_sum = [0_u64; 3];
    let mut dark_count: u64 = 0;
    let mut light_sum = [0_u64; 3];
    let mut light_count: u64 = 0;

    for (bin, bin_count) in bin_counts.iter().copied().enumerate() {
        if bin_count == 0 {
            continue;
        }
        let (sum, total) = if bin < avg_lum {
            (&mut dark_sum, &mut dark_count)
        } else {
            (&mut light_sum, &mut light_count)
        };
        *total += bin_count;
        sum[0] += bin_colors[bin][0];
        sum[1] += bin_colors[bin][1];
        sum[2] += bin_colors[bin][2];
    }

    let dark = average_color(dark_sum, dark_count, [0, 0, 0]);
    let light = average_color(light_sum, light_count, [255, 255, 255]);
    if dark_count == 0 || light_count == 0 {
        let region = average_color(total_color, count, [0, 0, 0]);
        let background = surrounding_background(image_buffer, bounds)
            .filter(|candidate| luminance_color(*candidate).abs_diff(luminance_color(region)) >= 24)
            .unwrap_or_else(|| {
                if luminance_color(region) < 128 {
                    [255, 255, 255]
                } else {
                    [0, 0, 0]
                }
            });
        return (format_hex(region), format_hex(background));
    }
    // A tight box can contain more ink than background. A contrasting perimeter
    // disambiguates the light/dark classes before excluding the text color.
    let surrounding = surrounding_background(image_buffer, bounds);
    let dark_is_text = surrounding
        .filter(|rim| distance(*rim, dark).abs_diff(distance(*rim, light)) >= 32)
        .map_or(dark_count < light_count, |rim| {
            distance(rim, light) < distance(rim, dark)
        });
    let (foreground, fallback) = if dark_is_text {
        (dark, light)
    } else {
        (light, dark)
    };

    // Exclude the most populated ink bin, rather than its antialiased mean:
    // averaging black strokes with gray edges can move the exclusion off black.
    let ink_bin = (0..LUMINANCE_BINS)
        .filter(|&bin| (bin < avg_lum) == dark_is_text)
        .max_by_key(|&bin| bin_counts[bin]);
    let ink = ink_bin.map_or(foreground, |bin| {
        average_color(bin_colors[bin], bin_counts[bin], foreground)
    });
    let background = palette
        .background(ink, surrounding.unwrap_or(fallback))
        .unwrap_or(fallback);
    (format_hex(foreground), format_hex(background))
}

fn surrounding_background(image_buffer: &image::RgbImage, bounds: Bounds) -> Option<[u8; 3]> {
    if image_buffer.width() == 0 || image_buffer.height() == 0 {
        return None;
    }
    let min_x = bounds.min_x.saturating_sub(1);
    let min_y = bounds.min_y.saturating_sub(1);
    let max_x = bounds.max_x.saturating_add(1).min(image_buffer.width() - 1);
    let max_y = bounds
        .max_y
        .saturating_add(1)
        .min(image_buffer.height() - 1);
    let mut palette = Palette::new();
    // Sample only the actual outside perimeter, never walk the full rectangle.
    for x in (min_x..=max_x).step_by((max_x - min_x + 1).div_ceil(128) as usize) {
        for y in [min_y, max_y] {
            if y < bounds.min_y || y > bounds.max_y {
                palette.add(image_buffer.get_pixel(x, y).0);
            }
        }
    }
    for y in (min_y..=max_y).step_by((max_y - min_y + 1).div_ceil(128) as usize) {
        for x in [min_x, max_x] {
            if x < bounds.min_x || x > bounds.max_x {
                palette.add(image_buffer.get_pixel(x, y).0);
            }
        }
    }
    palette.dominant(None)
}

fn luminance(pixel: &image::Rgb<u8>) -> u32 {
    (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) as u32
}

fn luminance_color(color: [u8; 3]) -> u32 {
    luminance(&image::Rgb(color))
}

fn average_color(sum: [u64; 3], count: u64, fallback: [u8; 3]) -> [u8; 3] {
    if count == 0 {
        return fallback;
    }
    [
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    ]
}

fn format_hex(color: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2])
}

#[cfg(test)]
#[path = "background_color_tests.rs"]
mod background_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_surrounding_pixels_for_a_single_dark_punctuation_pixel() {
        let mut image = image::RgbImage::from_pixel(3, 3, image::Rgb([255, 255, 255]));
        image.put_pixel(1, 1, image::Rgb([0, 0, 0]));

        assert_eq!(
            estimate_text_and_background_color(
                &image,
                Bounds {
                    min_x: 1,
                    max_x: 1,
                    min_y: 1,
                    max_y: 1,
                },
            ),
            ("#000000".to_owned(), "#ffffff".to_owned())
        );
    }

    #[test]
    fn gives_an_isolated_uniform_pixel_a_contrasting_fallback() {
        let image = image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 0]));

        assert_eq!(
            estimate_text_and_background_color(
                &image,
                Bounds {
                    min_x: 0,
                    max_x: 0,
                    min_y: 0,
                    max_y: 0,
                },
            ),
            ("#000000".to_owned(), "#ffffff".to_owned())
        );
    }
}
