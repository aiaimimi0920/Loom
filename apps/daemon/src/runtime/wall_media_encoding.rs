// At most two scaled PNG encodes run concurrently. Admission never waits behind another terminal.
use loom_protocol::wall::media::{
    WallMediaCodec, WallMediaFrame, WallMediaMetadata, WALL_MEDIA_PROTOCOL_VERSION,
};
static WALL_IMAGE_ENCODERS: AtomicUsize = AtomicUsize::new(0);
struct WallImageEncodePermit;
impl Drop for WallImageEncodePermit {
    fn drop(&mut self) {
        WALL_IMAGE_ENCODERS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
struct WallMediaProfile {
    codec: WallMediaCodec,
    width: u32,
    height: u32,
    fps: u64,
}
impl WallMediaProfile {
    fn read(request: &ParsedHttpRequest) -> std::result::Result<Self, LiveRuntimeError> {
        let codec = match request.query_parameter("format").as_deref() {
            Some("raw_bgra") => WallMediaCodec::RawBgra,
            Some("png") => WallMediaCodec::Png,
            _ => {
                return Err(LiveRuntimeError::new(
                    400,
                    "wall_media_format_invalid",
                    "format must be raw_bgra or png",
                ))
            }
        };
        let integer = |name| {
            parse_live_query_u64(request, name).map_err(|_| {
                LiveRuntimeError::new(
                    400,
                    "wall_media_profile_invalid",
                    "media limits must be unsigned integers",
                )
            })
        };
        if codec == WallMediaCodec::RawBgra
            && (request.query_parameter("maxWidth").is_some()
                || request.query_parameter("maxHeight").is_some())
        {
            return Err(LiveRuntimeError::new(
                400,
                "wall_media_profile_invalid",
                "raw BGRA does not support scaling parameters",
            ));
        }
        let width = integer("maxWidth")?.unwrap_or(640);
        let height = integer("maxHeight")?.unwrap_or(360);
        let fps = integer("maxFps")?.unwrap_or(if codec == WallMediaCodec::Png { 10 } else { 30 });
        if width == 0
            || height == 0
            || width > 1280
            || height > 720
            || fps == 0
            || fps > if codec == WallMediaCodec::Png { 10 } else { 60 }
        {
            return Err(LiveRuntimeError::new(
                400,
                "wall_media_profile_invalid",
                "media profile exceeds its size or frame-rate bounds",
            ));
        }
        Ok(Self {
            codec,
            width: width as u32,
            height: height as u32,
            fps,
        })
    }
    fn admitted(self, endpoint: &loom_protocol::wall::TileEndpoint) -> bool {
        use loom_protocol::wall::TileRenderMode;
        endpoint.render_modes.contains(&match self.codec {
            WallMediaCodec::RawBgra => TileRenderMode::RawBgra,
            WallMediaCodec::Png => TileRenderMode::Image,
        })
    }
}

fn encode_wall_media_frame(
    frame: &StoredLiveFrame,
    walls: &SharedWallStore,
    profile: WallMediaProfile,
) -> std::result::Result<Option<Vec<u8>>, &'static str> {
    let bytes = frame.bytes.as_slice();
    if bytes.len() < 64 || bytes.get(56..58) != Some(&[1, 1]) {
        return Err("wall_source_codec_unsupported");
    }
    let u32_at =
        |at| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("admitted NLLV header"));
    let u64_at =
        |at| u64::from_be_bytes(bytes[at..at + 8].try_into().expect("admitted NLLV header"));
    let mut metadata = WallMediaMetadata {
        epoch: frame.epoch,
        frame_id: frame.frame_id,
        capture_timestamp_ms: u64_at(24),
        encode_timestamp_ms: u64_at(32),
        received_timestamp_ms: walls
            .media_timestamp(frame.received_at)
            .map_err(|_| "wall_clock_unavailable")?,
        sent_timestamp_ms: 0,
        width: u32_at(40),
        height: u32_at(44),
        dropped_frames: u32_at(48),
        codec: profile.codec,
    };
    let mut encoded = Vec::new();
    let payload = if profile.codec == WallMediaCodec::Png {
        if WALL_IMAGE_ENCODERS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < 2).then_some(count + 1)
            })
            .is_err()
        {
            return Ok(None);
        }
        let _permit = WallImageEncodePermit;
        let (width, height, pixels) = scale_wall_bgra(
            &bytes[64..],
            metadata.width,
            metadata.height,
            profile.width,
            profile.height,
        )?;
        use image::{
            codecs::png::{CompressionType, FilterType, PngEncoder},
            ImageEncoder,
        };
        PngEncoder::new_with_quality(&mut encoded, CompressionType::Fast, FilterType::NoFilter)
            .write_image(&pixels, width, height, image::ExtendedColorType::Rgb8)
            .map_err(|_| "wall_png_encode_failed")?;
        metadata.width = width;
        metadata.height = height;
        encoded.as_slice()
    } else {
        &bytes[64..]
    };
    metadata.sent_timestamp_ms = walls
        .media_timestamp(Instant::now())
        .map_err(|_| "wall_clock_unavailable")?;
    WallMediaFrame { metadata, payload }
        .encode()
        .map(Some)
        .map_err(|_| "wall_media_frame_limit")
}

fn scale_wall_bgra(
    bytes: &[u8],
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> std::result::Result<(u32, u32, Vec<u8>), &'static str> {
    if width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || u64::from(width) * u64::from(height) * 4 != bytes.len() as u64
        || max_width == 0
        || max_height == 0
        || max_width > 1280
        || max_height > 720
    {
        return Err("wall_source_dimensions_invalid");
    }
    let scale = (max_width as f64 / width as f64)
        .min(max_height as f64 / height as f64)
        .min(1.0);
    let w = ((width as f64 * scale).floor() as u32).max(1);
    let h = ((height as f64 * scale).floor() as u32).max(1);
    let mut rgb = vec![0; w as usize * h as usize * 3];
    for y in 0..h {
        for x in 0..w {
            let source = ((u64::from(y) * u64::from(height) / u64::from(h)) * u64::from(width)
                + u64::from(x) * u64::from(width) / u64::from(w)) as usize
                * 4;
            let target = (y as usize * w as usize + x as usize) * 3;
            rgb[target..target + 3].copy_from_slice(&[
                bytes[source + 2],
                bytes[source + 1],
                bytes[source],
            ]);
        }
    }
    Ok((w, h, rgb))
}

#[cfg(test)]
mod wall_media_encoding_tests {
    use super::*;
    #[test]
    fn image_profile_preserves_channel_order_and_aspect_with_bounded_allocation() {
        let bytes = [0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 255];
        let (w, h, rgb) = scale_wall_bgra(&bytes, 4, 1, 2, 2).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgb, vec![255, 0, 0, 0, 255, 0]);
        assert!(scale_wall_bgra(&bytes, 4, 1, 0, 1).is_err());
        assert!(scale_wall_bgra(&bytes, 5, 1, 2, 2).is_err());
    }
}
