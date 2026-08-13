//! Image helper conversion contracts for Loom Art execution.

use std::io::Cursor;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageIoError {
    #[error("image data URL decode failed: {0}")]
    Decode(String),
    #[error("image load failed: {0}")]
    Image(String),
    #[error("image encode failed: {0}")]
    Encode(String),
    #[error("invalid RGBA8 image dimensions {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("RGBA8 size mismatch: expected {expected} bytes but received {actual} bytes")]
    SizeMismatch { expected: usize, actual: usize },
    #[error("image file read failed for `{path}`: {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },
}

pub type ImageIoResult<T> = Result<T, ImageIoError>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RgbaImageData {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size: usize,
    pub data: Vec<u8>,
}

pub fn decode_data_url_bytes(data_url_or_base64: &str) -> ImageIoResult<Vec<u8>> {
    let payload = data_url_or_base64
        .split_once(',')
        .map(|(_, payload)| payload)
        .unwrap_or(data_url_or_base64)
        .trim();
    BASE64
        .decode(payload)
        .map_err(|error| ImageIoError::Decode(error.to_string()))
}

pub fn decode_image_base64_to_rgba8(data_url_or_base64: &str) -> ImageIoResult<RgbaImageData> {
    let bytes = decode_data_url_bytes(data_url_or_base64)?;
    let image =
        image::load_from_memory(&bytes).map_err(|error| ImageIoError::Image(error.to_string()))?;
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let data = rgba.into_raw();
    let size = rgba8_size(width, height)?;
    Ok(RgbaImageData {
        width,
        height,
        format: "rgba8".to_owned(),
        size,
        data,
    })
}

pub fn read_image_path_as_data_url(path: impl AsRef<Path>) -> ImageIoResult<String> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| ImageIoError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;
    let image =
        image::load_from_memory(&bytes).map_err(|error| ImageIoError::Image(error.to_string()))?;
    dynamic_image_to_png_data_url(image)
}

pub fn rgba8_to_png_data_url(width: u32, height: u32, data: &[u8]) -> ImageIoResult<String> {
    let expected = rgba8_size(width, height)?;
    if data.len() != expected {
        return Err(ImageIoError::SizeMismatch {
            expected,
            actual: data.len(),
        });
    }
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, data.to_vec())
        .ok_or_else(|| ImageIoError::Encode("invalid RGBA8 image buffer".to_owned()))?;
    dynamic_image_to_png_data_url(DynamicImage::ImageRgba8(image))
}

fn dynamic_image_to_png_data_url(image: DynamicImage) -> ImageIoResult<String> {
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|error| ImageIoError::Encode(error.to_string()))?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64.encode(png.into_inner())
    ))
}

fn rgba8_size(width: u32, height: u32) -> ImageIoResult<usize> {
    if width == 0 || height == 0 {
        return Err(ImageIoError::InvalidDimensions { width, height });
    }
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|bytes| bytes as usize)
        .ok_or(ImageIoError::InvalidDimensions { width, height })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    const TEST_PNG_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAANSURBVBhXY+ASkfsPAAGkATy8Tqj3AAAAAElFTkSuQmCC";

    #[test]
    fn image_base64_data_url_decodes_to_rgba8() {
        let rgba = decode_image_base64_to_rgba8(TEST_PNG_DATA_URL).expect("decode image");

        assert_eq!(rgba.width, 1);
        assert_eq!(rgba.height, 1);
        assert_eq!(rgba.format, "rgba8");
        assert_eq!(rgba.size, 4);
        assert_eq!(rgba.data, vec![10, 20, 30, 255]);
    }

    #[test]
    fn rgba8_encodes_to_png_data_url() {
        let data_url = rgba8_to_png_data_url(1, 1, &[10, 20, 30, 255]).expect("encode png");

        assert!(data_url.starts_with("data:image/png;base64,"));
        let roundtrip = decode_image_base64_to_rgba8(&data_url).expect("decode roundtrip");
        assert_eq!(roundtrip.data, vec![10, 20, 30, 255]);
    }

    #[test]
    fn image_path_reads_as_png_data_url() {
        let root = temp_root("path-data-url");
        let path = root.join("pixel.png");
        fs::write(
            &path,
            decode_data_url_bytes(TEST_PNG_DATA_URL).expect("decode fixture"),
        )
        .expect("write fixture");

        let data_url = read_image_path_as_data_url(&path).expect("read path");

        assert!(data_url.starts_with("data:image/png;base64,"));
        let roundtrip = decode_image_base64_to_rgba8(&data_url).expect("decode roundtrip");
        assert_eq!(roundtrip.data, vec![10, 20, 30, 255]);
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn invalid_base64_reports_decode_error() {
        let error = decode_image_base64_to_rgba8("data:image/png;base64,not-valid")
            .expect_err("invalid base64 fails");

        assert!(error.to_string().contains("decode"));
    }

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("loom-image-io-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
