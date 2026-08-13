use std::collections::HashMap;
use std::io::Cursor;
use std::time::Instant;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProcessResult {
    pub success: bool,
    pub output_base64: Option<String>,
    pub error: Option<String>,
    pub processing_time_ms: u64,
}

#[must_use]
pub fn is_native_art_id(art_id: &str) -> bool {
    matches!(
        art_id,
        "core.image.pixelate"
            | "core.image.blur"
            | "core.image.grayscale"
            | "core.image.brightness"
            | "core.image.contrast"
            | "core.image.invert"
    )
}

#[must_use]
pub fn apply_pixelate(img: &DynamicImage, block_size: u32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let block_size = block_size.max(1);
    let mut output = img.to_rgba8();

    for block_y in (0..height).step_by(block_size as usize) {
        for block_x in (0..width).step_by(block_size as usize) {
            let end_x = (block_x + block_size).min(width);
            let end_y = (block_y + block_size).min(height);

            let mut r_sum: u32 = 0;
            let mut g_sum: u32 = 0;
            let mut b_sum: u32 = 0;
            let mut a_sum: u32 = 0;
            let mut count: u32 = 0;

            for y in block_y..end_y {
                for x in block_x..end_x {
                    let pixel = img.get_pixel(x, y);
                    r_sum += pixel[0] as u32;
                    g_sum += pixel[1] as u32;
                    b_sum += pixel[2] as u32;
                    a_sum += pixel[3] as u32;
                    count += 1;
                }
            }

            if count > 0 {
                let avg_color = Rgba([
                    (r_sum / count) as u8,
                    (g_sum / count) as u8,
                    (b_sum / count) as u8,
                    (a_sum / count) as u8,
                ]);
                for y in block_y..end_y {
                    for x in block_x..end_x {
                        output.put_pixel(x, y, avg_color);
                    }
                }
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

#[must_use]
pub fn apply_blur(img: &DynamicImage, radius: u32) -> DynamicImage {
    let radius = radius.max(1) as i32;
    let (width, height) = img.dimensions();
    let src = img.to_rgba8();
    let mut output = src.clone();

    for y in 0..height {
        for x in 0..width {
            let mut r_sum: u32 = 0;
            let mut g_sum: u32 = 0;
            let mut b_sum: u32 = 0;
            let mut a_sum: u32 = 0;
            let mut count: u32 = 0;

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;

                    if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                        let pixel = src.get_pixel(nx as u32, ny as u32);
                        r_sum += pixel[0] as u32;
                        g_sum += pixel[1] as u32;
                        b_sum += pixel[2] as u32;
                        a_sum += pixel[3] as u32;
                        count += 1;
                    }
                }
            }

            if count > 0 {
                output.put_pixel(
                    x,
                    y,
                    Rgba([
                        (r_sum / count) as u8,
                        (g_sum / count) as u8,
                        (b_sum / count) as u8,
                        (a_sum / count) as u8,
                    ]),
                );
            }
        }
    }

    DynamicImage::ImageRgba8(output)
}

#[must_use]
pub fn apply_grayscale(img: &DynamicImage) -> DynamicImage {
    img.grayscale()
}

#[must_use]
pub fn apply_brightness(img: &DynamicImage, amount: i32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let src = img.to_rgba8();
    let mut output = src.clone();

    for y in 0..height {
        for x in 0..width {
            let pixel = src.get_pixel(x, y);
            let new_pixel = Rgba([
                (pixel[0] as i32 + amount).clamp(0, 255) as u8,
                (pixel[1] as i32 + amount).clamp(0, 255) as u8,
                (pixel[2] as i32 + amount).clamp(0, 255) as u8,
                pixel[3],
            ]);
            output.put_pixel(x, y, new_pixel);
        }
    }

    DynamicImage::ImageRgba8(output)
}

#[must_use]
pub fn apply_contrast(img: &DynamicImage, factor: f32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let src = img.to_rgba8();
    let mut output = src.clone();

    for y in 0..height {
        for x in 0..width {
            let pixel = src.get_pixel(x, y);
            let new_pixel = Rgba([
                (((pixel[0] as f32 - 128.0) * factor + 128.0).clamp(0.0, 255.0)) as u8,
                (((pixel[1] as f32 - 128.0) * factor + 128.0).clamp(0.0, 255.0)) as u8,
                (((pixel[2] as f32 - 128.0) * factor + 128.0).clamp(0.0, 255.0)) as u8,
                pixel[3],
            ]);
            output.put_pixel(x, y, new_pixel);
        }
    }

    DynamicImage::ImageRgba8(output)
}

#[must_use]
pub fn apply_invert(img: &DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let src = img.to_rgba8();
    let mut output = src.clone();

    for y in 0..height {
        for x in 0..width {
            let pixel = src.get_pixel(x, y);
            let new_pixel = Rgba([255 - pixel[0], 255 - pixel[1], 255 - pixel[2], pixel[3]]);
            output.put_pixel(x, y, new_pixel);
        }
    }

    DynamicImage::ImageRgba8(output)
}

pub fn process_image(
    art_id: &str,
    img: &DynamicImage,
    params: &HashMap<String, serde_json::Value>,
) -> Result<DynamicImage, String> {
    match art_id {
        "core.image.pixelate" => {
            let block_size = params
                .get("pixel_size")
                .or_else(|| params.get("blockSize"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(10) as u32;
            Ok(apply_pixelate(img, block_size))
        }
        "core.image.blur" => {
            let radius = params
                .get("radius")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(5) as u32;
            Ok(apply_blur(img, radius))
        }
        "core.image.grayscale" => Ok(apply_grayscale(img)),
        "core.image.brightness" => {
            let amount = params
                .get("amount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0) as i32;
            Ok(apply_brightness(img, amount))
        }
        "core.image.contrast" => {
            let factor = params
                .get("factor")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            Ok(apply_contrast(img, factor))
        }
        "core.image.invert" => Ok(apply_invert(img)),
        _ => Err(format!("Unknown art_id: {art_id}")),
    }
}

#[must_use]
pub fn process_art(
    art_id: &str,
    input_base64: &str,
    params: HashMap<String, serde_json::Value>,
) -> ProcessResult {
    let start = Instant::now();
    let base64_data = input_base64.split(',').last().unwrap_or(input_base64);
    let image_bytes = match BASE64.decode(base64_data) {
        Ok(bytes) => bytes,
        Err(error) => {
            return process_error(start, format!("Failed to decode base64: {error}"));
        }
    };
    let img = match image::load_from_memory(&image_bytes) {
        Ok(img) => img,
        Err(error) => {
            return process_error(start, format!("Failed to load image: {error}"));
        }
    };
    let result_img = match process_image(art_id, &img, &params) {
        Ok(img) => img,
        Err(error) => return process_error(start, error),
    };

    let mut output_bytes = Cursor::new(Vec::new());
    if let Err(error) = result_img.write_to(&mut output_bytes, ImageFormat::Png) {
        return process_error(start, format!("Failed to encode output: {error}"));
    }

    ProcessResult {
        success: true,
        output_base64: Some(format!(
            "data:image/png;base64,{}",
            BASE64.encode(output_bytes.into_inner())
        )),
        error: None,
        processing_time_ms: start.elapsed().as_millis() as u64,
    }
}

fn process_error(start: Instant, message: String) -> ProcessResult {
    ProcessResult {
        success: false,
        output_base64: None,
        error: Some(message),
        processing_time_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Cursor;

    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};

    use super::*;

    fn encode_rgba_png(width: u32, height: u32, pixels: &[[u8; 4]]) -> String {
        let bytes = pixels
            .iter()
            .flat_map(|pixel| pixel.iter().copied())
            .collect::<Vec<_>>();
        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, bytes)
            .expect("test image buffer");
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut png, ImageFormat::Png)
            .expect("encode test png");
        format!("data:image/png;base64,{}", BASE64.encode(png.into_inner()))
    }

    fn decode_rgba_pixels(output_base64: &str) -> Vec<[u8; 4]> {
        let payload = output_base64.split(',').last().expect("base64 payload");
        let bytes = BASE64.decode(payload).expect("decode output png");
        let image = image::load_from_memory(&bytes)
            .expect("load output png")
            .to_rgba8();
        image.pixels().map(|pixel| pixel.0).collect()
    }

    fn decode_first_pixel(output_base64: &str) -> [u8; 4] {
        decode_rgba_pixels(output_base64)[0]
    }

    #[test]
    fn core_image_invert_wraps_png_base64_output() {
        let input = encode_rgba_png(1, 1, &[[10, 20, 30, 255]]);

        let result = process_art("core.image.invert", &input, HashMap::new());

        assert!(result.success);
        let output = result.output_base64.as_deref().expect("output base64");
        assert!(output.starts_with("data:image/png;base64,"));
        assert_eq!(decode_first_pixel(output), [245, 235, 225, 255]);
    }

    #[test]
    fn pixelate_averages_block_when_pixel_size_is_two() {
        let input = encode_rgba_png(2, 1, &[[0, 0, 0, 255], [100, 50, 0, 255]]);
        let mut params = HashMap::new();
        params.insert("pixel_size".to_owned(), serde_json::json!(2));

        let result = process_art("core.image.pixelate", &input, params);

        assert!(result.success);
        let output = result.output_base64.as_deref().expect("output base64");
        let pixels = decode_rgba_pixels(output);
        assert_eq!(pixels[0], [50, 25, 0, 255]);
        assert_eq!(pixels[1], [50, 25, 0, 255]);
    }

    #[test]
    fn unknown_art_id_returns_failure_result() {
        let input = encode_rgba_png(1, 1, &[[10, 20, 30, 255]]);

        let result = process_art("unknown", &input, HashMap::new());

        assert!(!result.success);
        assert!(result.error.expect("error").contains("Unknown art_id"));
    }
}
