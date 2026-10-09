// Connection-local JPEG negotiation and lazy legacy adaptation. No session lock spans decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LiveMediaProfile {
    Legacy,
    Jpeg,
}

impl LiveMediaProfile {
    fn offered(request: &ParsedHttpRequest) -> Self {
        if request
            .header("sec-websocket-protocol")
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|token| token.trim() == loom_protocol::LIVE_JPEG_PROTOCOL_VERSION)
            })
        {
            Self::Jpeg
        } else {
            Self::Legacy
        }
    }

    fn protocol(self) -> &'static str {
        match self {
            Self::Legacy => loom_protocol::LIVE_PROTOCOL_VERSION,
            Self::Jpeg => loom_protocol::LIVE_JPEG_PROTOCOL_VERSION,
        }
    }

    fn accepts(self, bytes: &[u8]) -> bool {
        bytes.get(57) == Some(&1) || (self == Self::Jpeg && bytes.get(57) == Some(&3))
    }
}

type LiveLegacyFrameCache = Option<std::result::Result<Arc<Vec<u8>>, &'static str>>;
fn live_media_wire_codec(bytes: &[u8]) -> &'static str {
    match bytes.get(57) {
        Some(1) => "raw_bgra",
        Some(2) => "h264",
        Some(3) => "jpeg",
        _ => "unknown",
    }
}
static LIVE_JPEG_DECODERS: AtomicUsize = AtomicUsize::new(0);
struct LiveJpegDecodePermit;
impl Drop for LiveJpegDecodePermit {
    fn drop(&mut self) {
        LIVE_JPEG_DECODERS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn live_jpeg_decoder(
    bytes: &[u8],
    width: u32,
    height: u32,
) -> std::result::Result<image::codecs::jpeg::JpegDecoder<std::io::Cursor<&[u8]>>, &'static str> {
    use image::ImageDecoder;
    if bytes.len() > loom_protocol::LIVE_MAX_JPEG_PAYLOAD
        || !bytes.starts_with(&[0xff, 0xd8])
        || !bytes.ends_with(&[0xff, 0xd9])
        || width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || u64::from(width) * u64::from(height) * 4 > loom_protocol::LIVE_MAX_FRAME_PAYLOAD as u64
    {
        return Err("live_jpeg_bounds_invalid");
    }
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|_| "live_jpeg_header_invalid")?;
    if decoder.dimensions() != (width, height) {
        return Err("live_jpeg_dimensions_mismatch");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    decoder
        .set_limits(limits)
        .map_err(|_| "live_jpeg_limits_invalid")?;
    Ok(decoder)
}

impl StoredLiveFrame {
    fn representation(
        &self,
        profile: LiveMediaProfile,
    ) -> std::result::Result<Option<Arc<Vec<u8>>>, &'static str> {
        if self.bytes.get(57) == Some(&2) {
            return Err("live_h264_not_negotiated");
        }
        if self.bytes.get(57) != Some(&3) || profile == LiveMediaProfile::Jpeg {
            return Ok(Some(Arc::clone(&self.bytes)));
        }
        // A slow legacy consumer never makes the JPEG path wait. Cache identity is the immutable frame.
        let mut cache = match self.legacy.try_lock() {
            Ok(cache) => cache,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(_) => return Err("live_legacy_cache_unavailable"),
        };
        if let Some(result) = cache.as_ref() {
            return result.clone().map(Some);
        }
        if LIVE_JPEG_DECODERS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < 2).then_some(count + 1)
            })
            .is_err()
        {
            return Ok(None);
        }
        let _permit = LiveJpegDecodePermit;
        let result = self.decode_legacy();
        *cache = Some(result.clone());
        result.map(Some)
    }

    fn decode_legacy(&self) -> std::result::Result<Arc<Vec<u8>>, &'static str> {
        use image::ImageDecoder;
        let (_, metadata) =
            LiveBinaryFrame::decode_header(&self.bytes).map_err(|_| "live_jpeg_frame_invalid")?;
        let decoder = live_jpeg_decoder(&self.bytes[64..], metadata.width, metadata.height)?;
        let channels = match decoder.color_type() {
            image::ColorType::L8 => 1,
            image::ColorType::La8 => 2,
            image::ColorType::Rgb8 => 3,
            image::ColorType::Rgba8 => 4,
            _ => return Err("live_jpeg_color_unsupported"),
        };
        // live_jpeg_decoder 已限制为至多 64 MiB BGRA；解码直接写入最终帧，避免中间整帧缓冲。
        let pixels = metadata.width as usize * metadata.height as usize;
        let payload_len = pixels * 4;
        let mut bytes = vec![0; 64 + payload_len];
        decoder
            .read_image(&mut bytes[64..64 + pixels * channels])
            .map_err(|_| "live_jpeg_decode_failed")?;
        expand_live_bgra_in_place(&mut bytes[64..], channels);
        bytes[..64].copy_from_slice(&self.bytes[..64]);
        bytes[52..56].copy_from_slice(&(payload_len as u32).to_be_bytes());
        bytes[57] = 1;
        Ok(Arc::new(bytes))
    }
}

// 只接受上方 JPEG 解码器确认的 8 位通道数；反向扩展避免覆盖尚未读取的紧凑像素。
fn expand_live_bgra_in_place(payload: &mut [u8], channels: usize) {
    debug_assert!((1..=4).contains(&channels) && payload.len() % 4 == 0);
    for pixel in (0..payload.len() / 4).rev() {
        let start = pixel * channels;
        let bgra = match channels {
            1 => [payload[start], payload[start], payload[start], 255],
            2 => [
                payload[start],
                payload[start],
                payload[start],
                payload[start + 1],
            ],
            3 => [payload[start + 2], payload[start + 1], payload[start], 255],
            4 => [
                payload[start + 2],
                payload[start + 1],
                payload[start],
                payload[start + 3],
            ],
            _ => unreachable!(),
        };
        payload[pixel * 4..pixel * 4 + 4].copy_from_slice(&bgra);
    }
}

impl LiveSessionStore {
    fn viewer_frame_authorized(&self, session_id: &str, device_id: &str, epoch: u64) -> bool {
        self.state.lock().ok().is_some_and(|sessions| {
            sessions.get(session_id).is_some_and(|record| {
                !record.closed
                    && record.epoch == epoch
                    && record
                        .session
                        .viewer_devices
                        .iter()
                        .any(|viewer| viewer == device_id)
            })
        })
    }
}
