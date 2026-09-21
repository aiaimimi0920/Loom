//! Independent frame transport; pixels and timing never enter the durable wall document.
use super::{WallValidationError, WALL_MAX_REVISION};
use serde::{Deserialize, Serialize};

pub const WALL_MEDIA_PROTOCOL_VERSION: &str = "loom.wall.media.v1";
pub const WALL_MEDIA_HEADER_LEN: usize = 80;
pub const WALL_MEDIA_MAX_PAYLOAD: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WallMediaCodec {
    RawBgra,
    Png,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WallMediaMetadata {
    pub epoch: u64,
    pub frame_id: u64,
    pub capture_timestamp_ms: u64,
    pub encode_timestamp_ms: u64,
    pub received_timestamp_ms: u64,
    pub sent_timestamp_ms: u64,
    pub width: u32,
    pub height: u32,
    pub dropped_frames: u32,
    pub codec: WallMediaCodec,
}

pub struct WallMediaFrame<'a> {
    pub metadata: WallMediaMetadata,
    pub payload: &'a [u8],
}
impl<'a> WallMediaFrame<'a> {
    pub fn encode(&self) -> Result<Vec<u8>, WallValidationError> {
        self.validate()?;
        let m = self.metadata;
        let mut bytes = vec![0; WALL_MEDIA_HEADER_LEN + self.payload.len()];
        bytes[..8].copy_from_slice(&[b'N', b'L', b'W', b'M', 1, 1, 0, 80]);
        for (offset, value) in [
            (8, m.epoch),
            (16, m.frame_id),
            (24, m.capture_timestamp_ms),
            (32, m.encode_timestamp_ms),
            (64, m.received_timestamp_ms),
            (72, m.sent_timestamp_ms),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
        }
        for (offset, value) in [
            (40, m.width),
            (44, m.height),
            (48, m.dropped_frames),
            (52, self.payload.len() as u32),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        bytes[56] = 1;
        bytes[57] = match m.codec {
            WallMediaCodec::RawBgra => 1,
            WallMediaCodec::Png => 2,
        };
        bytes[WALL_MEDIA_HEADER_LEN..].copy_from_slice(self.payload);
        Ok(bytes)
    }
    pub fn decode(bytes: &'a [u8]) -> Result<Self, WallValidationError> {
        if bytes.len() < WALL_MEDIA_HEADER_LEN
            || bytes[..8] != [b'N', b'L', b'W', b'M', 1, 1, 0, 80]
            || bytes[56] != 1
            || bytes[58..64].iter().any(|byte| *byte != 0)
            || u32_at(bytes, 52) as usize != bytes.len() - WALL_MEDIA_HEADER_LEN
        {
            return Err(WallValidationError("invalid wall media header"));
        }
        let codec = match bytes[57] {
            1 => WallMediaCodec::RawBgra,
            2 => WallMediaCodec::Png,
            _ => return Err(WallValidationError("unsupported wall media codec")),
        };
        let frame = Self {
            metadata: WallMediaMetadata {
                epoch: u64_at(bytes, 8),
                frame_id: u64_at(bytes, 16),
                capture_timestamp_ms: u64_at(bytes, 24),
                encode_timestamp_ms: u64_at(bytes, 32),
                received_timestamp_ms: u64_at(bytes, 64),
                sent_timestamp_ms: u64_at(bytes, 72),
                width: u32_at(bytes, 40),
                height: u32_at(bytes, 44),
                dropped_frames: u32_at(bytes, 48),
                codec,
            },
            payload: &bytes[WALL_MEDIA_HEADER_LEN..],
        };
        frame.validate()?;
        Ok(frame)
    }
    fn validate(&self) -> Result<(), WallValidationError> {
        let m = self.metadata;
        if self.payload.is_empty()
            || self.payload.len() > WALL_MEDIA_MAX_PAYLOAD
            || m.epoch == 0
            || m.frame_id == 0
            || m.width == 0
            || m.height == 0
            || m.width > 16_384
            || m.height > 16_384
            || [
                m.capture_timestamp_ms,
                m.encode_timestamp_ms,
                m.received_timestamp_ms,
                m.sent_timestamp_ms,
            ]
            .iter()
            .any(|time| *time > WALL_MAX_REVISION)
            || m.received_timestamp_ms == 0
            || m.sent_timestamp_ms < m.received_timestamp_ms
        {
            return Err(WallValidationError(
                "invalid wall media identity, size or time",
            ));
        }
        match m.codec {
            WallMediaCodec::RawBgra
                if u64::from(m.width) * u64::from(m.height) * 4 != self.payload.len() as u64 =>
            {
                Err(WallValidationError("wall raw pixel length mismatch"))
            }
            WallMediaCodec::Png
                if m.width > 1280
                    || m.height > 720
                    || self.payload.len() > 4 * 1024 * 1024
                    || !valid_png(self.payload, m.width, m.height) =>
            {
                Err(WallValidationError("wall PNG dimensions or chunks invalid"))
            }
            _ => Ok(()),
        }
    }
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().expect("checked header"))
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(bytes[at..at + 8].try_into().expect("checked header"))
}

// RGB8, non-interlaced, and no ancillary/animated chunks. Bound decoder work before allocating pixels.
fn valid_png(bytes: &[u8], width: u32, height: u32) -> bool {
    if bytes.len() < 57
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || u32_at(bytes, 8) != 13
        || &bytes[12..16] != b"IHDR"
        || u32_at(bytes, 16) != width
        || u32_at(bytes, 20) != height
        || bytes[24..29] != [8, 2, 0, 0, 0]
    {
        return false;
    }
    let mut at = 33;
    let mut data = false;
    while bytes.len().saturating_sub(at) >= 12 {
        let length = u32_at(bytes, at) as usize;
        if length > bytes.len() - at - 12 {
            return false;
        }
        match &bytes[at + 4..at + 8] {
            b"IDAT" => data = true,
            b"IEND" => return data && length == 0 && at + 12 == bytes.len(),
            _ => return false,
        }
        at += length + 12;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_golden_frames_preserve_the_common_timeline_and_reject_png_dimension_spoofing() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../protocol/fixtures/wall-media.v1.json"
        ))
        .unwrap();
        for (name, codec) in [
            ("raw_bgra", WallMediaCodec::RawBgra),
            ("png", WallMediaCodec::Png),
        ] {
            let hex = fixture[name].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect();
            let frame = WallMediaFrame::decode(&bytes).unwrap();
            assert_eq!((frame.metadata.epoch, frame.metadata.frame_id), (2, 7));
            assert_eq!(
                (
                    frame.metadata.width,
                    frame.metadata.height,
                    frame.metadata.dropped_frames
                ),
                (2, 1, 3)
            );
            assert_eq!(frame.metadata.received_timestamp_ms, 1_700_000_000_020);
            assert_eq!(frame.metadata.sent_timestamp_ms, 1_700_000_000_024);
            assert_eq!(frame.metadata.codec, codec);
            assert_eq!(frame.encode().unwrap(), bytes);
            if codec == WallMediaCodec::Png {
                let mut invalid = bytes;
                invalid[96..100].copy_from_slice(&16_384u32.to_be_bytes());
                assert!(WallMediaFrame::decode(&invalid).is_err());
            }
        }
    }
    #[test]
    fn rejects_lengths_reserved_bits_and_time_regression_before_decoding_pixels() {
        let frame = WallMediaFrame {
            metadata: WallMediaMetadata {
                epoch: 1,
                frame_id: 1,
                capture_timestamp_ms: 1,
                encode_timestamp_ms: 2,
                received_timestamp_ms: 3,
                sent_timestamp_ms: 4,
                width: 1,
                height: 1,
                dropped_frames: 0,
                codec: WallMediaCodec::RawBgra,
            },
            payload: &[1, 2, 3, 255],
        };
        let bytes = frame.encode().unwrap();
        assert_eq!(
            WallMediaFrame::decode(&bytes).unwrap().metadata,
            frame.metadata
        );
        for offset in [0, 4, 5, 7, 52, 56, 57, 58, 63] {
            let mut invalid = bytes.clone();
            invalid[offset] = 255;
            assert!(WallMediaFrame::decode(&invalid).is_err(), "offset {offset}");
        }
        let mut invalid = bytes;
        invalid[72..80].copy_from_slice(&2u64.to_be_bytes());
        assert!(WallMediaFrame::decode(&invalid).is_err());
    }
}
