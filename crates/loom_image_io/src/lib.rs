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

/// An image file turned into a data URL, together with the MIME type that URL actually carries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDataUrl {
    pub data_url: String,
    pub mime_type: &'static str,
    /// Whether the bytes were re-encoded as PNG rather than passed through as they were on disk.
    pub re_encoded: bool,
}

/// Read an image file into a data URL, passing the bytes through when a browser can render them.
///
/// [`read_image_path_as_data_url`] decodes the file and re-encodes it as PNG. That is three buffers at
/// peak — the file, the decoded surface, and the PNG — and the decoded surface is the dangerous one,
/// because its size is width × height × 4 and has no relation to the compressed size a caller checked
/// against a limit. A small, highly compressible PNG decodes to as much memory as its dimensions say.
///
/// This reads the file, identifies the container from its magic bytes, and for a format browsers render
/// natively emits those same bytes with the format's own MIME type. Peak memory is then the file plus
/// its base64, which the caller's file-size limit does bound. It also stops mislabelling: the old path
/// always announced `image/png` because it always produced PNG, so a JPEG output either came back
/// re-encoded and larger or, with only the PNG decoder compiled in, failed to be read at all.
///
/// A format outside that set — TIFF, and the other containers `image` understands but browsers do not —
/// still goes through the decode-and-re-encode path, since passing those bytes to a viewer would give it
/// something it cannot display. `re_encoded` says which of the two happened.
///
/// The bytes are not decoded when they are passed through, so this validates the container type and not
/// the pixel data. That is the trade being made: a full decode is exactly the unbounded allocation this
/// avoids, and every consumer decodes the image itself anyway.
pub fn read_image_path_as_web_data_url(path: impl AsRef<Path>) -> ImageIoResult<ImageDataUrl> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| ImageIoError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;
    if let Some(image) = web_renderable_image_bytes_as_data_url(&bytes) {
        return Ok(image);
    }
    let image =
        image::load_from_memory(&bytes).map_err(|error| ImageIoError::Image(error.to_string()))?;
    Ok(ImageDataUrl {
        data_url: dynamic_image_to_png_data_url(image)?,
        mime_type: "image/png",
        re_encoded: true,
    })
}

/// Encode already-bounded browser-renderable image bytes without decoding their pixel surface.
///
/// Callers that enforce their own file-handle and byte limits can use this to avoid reopening a path
/// after validation. Unsupported or unrecognized containers return `None` rather than taking the
/// potentially unbounded decode-and-re-encode fallback used by [`read_image_path_as_web_data_url`].
pub fn web_renderable_image_bytes_as_data_url(bytes: &[u8]) -> Option<ImageDataUrl> {
    let mime_type = image::guess_format(bytes)
        .ok()
        .and_then(web_renderable_mime_type)?;
    Some(ImageDataUrl {
        data_url: format!("data:{mime_type};base64,{}", BASE64.encode(bytes)),
        mime_type,
        re_encoded: false,
    })
}

/// The MIME type for a format a browser renders from its own bytes, or `None` if it does not.
///
/// Listed explicitly rather than taken from `ImageFormat::to_mime_type` for every format, because the
/// question here is not what the container is called but whether handing those bytes to a viewer
/// displays an image.
fn web_renderable_mime_type(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png
        | ImageFormat::Jpeg
        | ImageFormat::Gif
        | ImageFormat::WebP
        | ImageFormat::Bmp
        | ImageFormat::Ico
        | ImageFormat::Avif => Some(format.to_mime_type()),
        _ => None,
    }
}

/// The shortest string this crate will guess is a base64 image nobody labelled.
///
/// Below a kilobyte the guess is not worth making. A base64 image that small is smaller than
/// anything the Art and tool paths here produce, while the values that get mistaken for one —
/// request ids, opaque tokens, hex digests, status words — all sit well under the bound. A caller
/// that knows it holds an image says so with a `data:image/` prefix or an `image/*` MIME type, and
/// neither of those signals needs this guess.
pub const MIN_UNLABELLED_BASE64_IMAGE_LENGTH: usize = 1024;

/// Whether a string is plausibly a base64-encoded image that carries no other identifying signal.
///
/// Three properties are required beyond the length: a length that is a multiple of 4, `=` padding
/// only at the end and at most two of it, and a single alphabet — standard `+/` or URL-safe `-_`,
/// never both. Real base64 has all three; text that merely happens to be alphanumeric almost never
/// does.
///
/// This lives here rather than in each consumer because both the tool registry and the workflow
/// runtime need the same answer, and when they each kept their own copy the two drifted apart on
/// what counted as an image.
pub fn looks_like_base64_image_payload(value: &str) -> bool {
    if value.len() < MIN_UNLABELLED_BASE64_IMAGE_LENGTH || value.len() % 4 != 0 {
        return false;
    }
    let body = value.trim_end_matches('=');
    if value.len() - body.len() > 2 {
        return false;
    }
    let mut standard_alphabet = false;
    let mut url_safe_alphabet = false;
    for ch in body.chars() {
        match ch {
            '+' | '/' => standard_alphabet = true,
            '-' | '_' => url_safe_alphabet = true,
            _ if ch.is_ascii_alphanumeric() => {}
            _ => return false,
        }
    }
    !(standard_alphabet && url_safe_alphabet)
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
    fn a_web_renderable_image_is_passed_through_with_its_own_mime_type() {
        // The point of the pass-through is that no decode happens, so the payload has to be the file
        // byte for byte and the label has to come from the file rather than from a constant.
        let root = temp_root("web-data-url");
        let png_path = root.join("pixel.png");
        let png_bytes = decode_data_url_bytes(TEST_PNG_DATA_URL).expect("decode fixture");
        fs::write(&png_path, &png_bytes).expect("write png fixture");

        let png = read_image_path_as_web_data_url(&png_path).expect("read png");
        assert_eq!(png.mime_type, "image/png");
        assert!(!png.re_encoded, "a PNG needs no re-encoding");
        assert_eq!(
            decode_data_url_bytes(&png.data_url).expect("decode png payload"),
            png_bytes,
            "the payload must be the file's own bytes"
        );

        // A JPEG is identified from its magic bytes, which is all the pass-through needs — this crate
        // is built with the PNG decoder only, so the old re-encoding path could not read one at all.
        let jpeg_path = root.join("photo.jpg");
        let jpeg_bytes = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
        ];
        fs::write(&jpeg_path, jpeg_bytes).expect("write jpeg fixture");

        let jpeg = read_image_path_as_web_data_url(&jpeg_path).expect("read jpeg");
        assert_eq!(jpeg.mime_type, "image/jpeg");
        assert!(!jpeg.re_encoded);
        assert!(jpeg.data_url.starts_with("data:image/jpeg;base64,"));
        fs::remove_dir_all(root).expect("cleanup fixture");
    }

    #[test]
    fn invalid_base64_reports_decode_error() {
        let error = decode_image_base64_to_rgba8("data:image/png;base64,not-valid")
            .expect_err("invalid base64 fails");

        assert!(error.to_string().contains("decode"));
    }

    #[test]
    fn ordinary_text_is_not_mistaken_for_a_base64_image() {
        for value in [
            "completed",
            "12345678",
            "req_01HX9ZQK7T2M4V8N6P3R5S7W9Y",
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            "queued-for-render",
            "",
        ] {
            assert!(
                !looks_like_base64_image_payload(value),
                "`{value}` should not read as a base64 image"
            );
        }
    }

    #[test]
    fn a_kilobyte_scale_payload_reads_as_a_base64_image() {
        let payload = BASE64.encode(vec![0x5Au8; 4096]);

        assert!(looks_like_base64_image_payload(&payload));
    }

    #[test]
    fn a_long_payload_still_needs_regular_base64_shape() {
        let payload = BASE64.encode(vec![0x5Au8; 4096]);

        // A length that is not a multiple of 4 cannot be base64 whatever else it looks like.
        assert!(!looks_like_base64_image_payload(
            &payload[..payload.len() - 1]
        ));
        // Padding belongs at the end, and there is never more than two of it.
        assert!(!looks_like_base64_image_payload(&format!(
            "{}===",
            &payload[..payload.len() - 3]
        )));
        assert!(!looks_like_base64_image_payload(&format!(
            "===={}",
            &payload[4..]
        )));
        // One alphabet or the other, never a mixture of the two.
        assert!(!looks_like_base64_image_payload(&format!(
            "-_+/{}",
            &payload[4..]
        )));
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
