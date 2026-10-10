//! Bounded PNG decode/encode; limits are installed before decoder construction.
use std::io::{self, BufRead, Cursor, Read, Seek, SeekFrom, Write};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};

use crate::budget::{self, Budget, MAX_DIMENSION, MAX_INPUT_BYTES, MAX_OUTPUT_BYTES};

struct GuardedIo<'a, T> {
    inner: T,
    budget: Budget<'a>,
}
impl<T> GuardedIo<'_, T> {
    fn check(&self) -> io::Result<()> {
        self.budget.check().map_err(io::Error::other)
    }
}
impl<T: Read> Read for GuardedIo<'_, T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let length = bytes.len().min(64 * 1024);
        self.inner.read(&mut bytes[..length])
    }
}
impl<T: BufRead> BufRead for GuardedIo<'_, T> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.check()?;
        let bytes = self.inner.fill_buf()?;
        Ok(&bytes[..bytes.len().min(64 * 1024)])
    }
    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}
impl<T: Seek> Seek for GuardedIo<'_, T> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.inner.seek(position)
    }
}
impl Write for GuardedIo<'_, Cursor<Vec<u8>>> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.check()?;
        if self.inner.position().saturating_add(bytes.len() as u64) > MAX_OUTPUT_BYTES as u64 {
            return Err(io::Error::other("native image output limit exceeded"));
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()
    }
}

pub(crate) fn decode(input: &str, budget: Budget<'_>) -> Result<DynamicImage, String> {
    budget.check()?;
    if input.len() > MAX_INPUT_BYTES.div_ceil(3) * 4 + 128 {
        return Err("native image input limit exceeded".to_owned());
    }
    let payload = input.rsplit(',').next().unwrap_or(input);
    let bytes = BASE64
        .decode(payload)
        .map_err(|e| format!("Failed to decode base64: {e}"))?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("native image input limit exceeded".to_owned());
    }
    // Native image workflows support PNG, regardless of feature unification by consumers.
    let mut reader = ImageReader::with_format(
        GuardedIo {
            inner: Cursor::new(bytes),
            budget,
        },
        ImageFormat::Png,
    );
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Failed to load image: {e}"))?;
    let (width, height) = decoder.dimensions();
    budget::validate_surface(width, height)?;
    if decoder.total_bytes() > 64 * 1024 * 1024 {
        return Err("native image decoded byte limit exceeded".to_owned());
    }
    budget.check()?;
    DynamicImage::from_decoder(decoder).map_err(|e| format!("Failed to load image: {e}"))
}

pub(crate) fn encode(image: &DynamicImage, budget: Budget<'_>) -> Result<String, String> {
    budget.check()?;
    let mut output = GuardedIo {
        inner: Cursor::new(Vec::new()),
        budget,
    };
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode output: {e}"))?;
    budget.check()?;
    let result = format!(
        "data:image/png;base64,{}",
        BASE64.encode(output.inner.into_inner())
    );
    budget.check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_image(width: u32, height: u32) -> String {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(1, 1)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let mut bytes = bytes.into_inner();
        bytes[16..20].copy_from_slice(&width.to_be_bytes());
        bytes[20..24].copy_from_slice(&height.to_be_bytes());
        let mut crc = u32::MAX;
        for byte in &bytes[12..29] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320_u32 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
        BASE64.encode(bytes)
    }

    #[test]
    fn header_dimensions_and_pixel_product_are_checked_before_decode() {
        let budget = Budget::new(None, None);
        let wide = decode(&header_image(8193, 1), budget).unwrap_err();
        assert!(wide.contains("limit"), "{wide}");
        let large = decode(&header_image(4096, 4096), budget).unwrap_err();
        assert!(large.contains("surface limit"), "{large}");
    }

    #[test]
    fn encoded_input_and_output_are_bounded() {
        let budget = Budget::new(None, None);
        let input = "A".repeat(MAX_INPUT_BYTES.div_ceil(3) * 4 + 129);
        assert!(decode(&input, budget).unwrap_err().contains("input limit"));
        let mut output = GuardedIo {
            inner: Cursor::new(Vec::new()),
            budget,
        };
        output
            .seek(SeekFrom::Start(MAX_OUTPUT_BYTES as u64))
            .unwrap();
        assert!(output
            .write_all(&[1])
            .unwrap_err()
            .to_string()
            .contains("output limit"));
        assert!(output.inner.get_ref().is_empty());
    }
}
