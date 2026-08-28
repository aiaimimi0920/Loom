use crate::geometry::Bounds;

pub(crate) fn estimate_text_and_background_color(
    image_buffer: &image::RgbImage,
    bounds: Bounds,
) -> (String, String) {
    let mut total_lum: u64 = 0;
    let mut total_color = [0_u64; 3];
    let mut count: u64 = 0;

    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pixel = image_buffer.get_pixel(x, y);
            total_lum += luminance(pixel) as u64;
            total_color[0] += pixel[0] as u64;
            total_color[1] += pixel[1] as u64;
            total_color[2] += pixel[2] as u64;
            count += 1;
        }
    }

    if count == 0 {
        return ("#000000".to_owned(), "#ffffff".to_owned());
    }

    let avg_lum = total_lum / count;
    let mut dark_sum = [0_u64; 3];
    let mut dark_count: u64 = 0;
    let mut light_sum = [0_u64; 3];
    let mut light_count: u64 = 0;

    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pixel = image_buffer.get_pixel(x, y);
            let lum = luminance(pixel) as u64;
            let target = if lum < avg_lum {
                dark_count += 1;
                &mut dark_sum
            } else {
                light_count += 1;
                &mut light_sum
            };
            target[0] += pixel[0] as u64;
            target[1] += pixel[1] as u64;
            target[2] += pixel[2] as u64;
        }
    }

    let dark = average_color(dark_sum, dark_count, [0, 0, 0]);
    let light = average_color(light_sum, light_count, [255, 255, 255]);
    if dark_count == 0 || light_count == 0 {
        let region = average_color(total_color, count, [0, 0, 0]);
        let background = surrounding_average(image_buffer, bounds)
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
    let (foreground, background) = if dark_count < light_count {
        (dark, light)
    } else {
        (light, dark)
    };

    (format_hex(foreground), format_hex(background))
}

fn surrounding_average(image_buffer: &image::RgbImage, bounds: Bounds) -> Option<[u8; 3]> {
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
    let mut sum = [0_u64; 3];
    let mut count = 0_u64;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if x >= bounds.min_x && x <= bounds.max_x && y >= bounds.min_y && y <= bounds.max_y {
                continue;
            }
            let pixel = image_buffer.get_pixel(x, y);
            sum[0] += pixel[0] as u64;
            sum[1] += pixel[1] as u64;
            sum[2] += pixel[2] as u64;
            count += 1;
        }
    }
    (count > 0).then(|| average_color(sum, count, [0, 0, 0]))
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
