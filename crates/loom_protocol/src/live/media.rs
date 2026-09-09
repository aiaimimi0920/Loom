use serde::{Deserialize, Serialize};

use super::{
    validate_frame_metadata, LiveCodec, LiveColorSpace, LiveProtocolError, LIVE_BINARY_VERSION,
    LIVE_MAX_FRAME_PAYLOAD,
};

pub const LIVE_BINARY_MAGIC: [u8; 4] = *b"NLLV";
pub const LIVE_BINARY_HEADER_LEN: usize = 64;
const KEYFRAME_FLAG: u8 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveFrameMetadata {
    pub frame_id: u64,
    pub capture_timestamp_ms: u64,
    pub encode_timestamp_ms: u64,
    pub width: u32,
    pub height: u32,
    pub keyframe: bool,
    pub dropped_frames: u32,
    pub color_space: LiveColorSpace,
    pub codec: LiveCodec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveBinaryFrame {
    pub epoch: u64,
    pub metadata: LiveFrameMetadata,
    pub payload: Vec<u8>,
}

impl LiveBinaryFrame {
    pub fn encode(&self) -> Result<Vec<u8>, LiveProtocolError> {
        validate_binary_identity(self.epoch, &self.metadata)?;
        validate_payload_len(self.payload.len())?;
        let mut output = vec![0u8; LIVE_BINARY_HEADER_LEN + self.payload.len()];
        output[0..4].copy_from_slice(&LIVE_BINARY_MAGIC);
        output[4] = LIVE_BINARY_VERSION;
        output[5] = u8::from(self.metadata.keyframe) * KEYFRAME_FLAG;
        output[6..8].copy_from_slice(&(LIVE_BINARY_HEADER_LEN as u16).to_be_bytes());
        output[8..16].copy_from_slice(&self.epoch.to_be_bytes());
        output[16..24].copy_from_slice(&self.metadata.frame_id.to_be_bytes());
        output[24..32].copy_from_slice(&self.metadata.capture_timestamp_ms.to_be_bytes());
        output[32..40].copy_from_slice(&self.metadata.encode_timestamp_ms.to_be_bytes());
        output[40..44].copy_from_slice(&self.metadata.width.to_be_bytes());
        output[44..48].copy_from_slice(&self.metadata.height.to_be_bytes());
        output[48..52].copy_from_slice(&self.metadata.dropped_frames.to_be_bytes());
        output[52..56].copy_from_slice(&(self.payload.len() as u32).to_be_bytes());
        output[56] = color_code(self.metadata.color_space);
        output[57] = codec_code(self.metadata.codec);
        output[LIVE_BINARY_HEADER_LEN..].copy_from_slice(&self.payload);
        Ok(output)
    }

    pub fn decode(input: &[u8]) -> Result<Self, LiveProtocolError> {
        if input.len() < LIVE_BINARY_HEADER_LEN {
            return Err(LiveProtocolError::TruncatedFrame);
        }
        if input[0..4] != LIVE_BINARY_MAGIC {
            return Err(LiveProtocolError::InvalidFrame("magic"));
        }
        if input[4] != LIVE_BINARY_VERSION {
            return Err(LiveProtocolError::UnsupportedBinaryVersion(input[4]));
        }
        if input[5] & !KEYFRAME_FLAG != 0 {
            return Err(LiveProtocolError::InvalidFrame("flags"));
        }
        if u16::from_be_bytes([input[6], input[7]]) as usize != LIVE_BINARY_HEADER_LEN {
            return Err(LiveProtocolError::InvalidFrame("header_length"));
        }
        if input[58..64].iter().any(|value| *value != 0) {
            return Err(LiveProtocolError::InvalidFrame("reserved"));
        }
        let payload_len = u32_at(input, 52) as usize;
        validate_payload_len(payload_len)?;
        if input.len() != LIVE_BINARY_HEADER_LEN + payload_len {
            return Err(LiveProtocolError::PayloadLengthMismatch);
        }
        let frame = Self {
            epoch: u64_at(input, 8),
            metadata: LiveFrameMetadata {
                frame_id: u64_at(input, 16),
                capture_timestamp_ms: u64_at(input, 24),
                encode_timestamp_ms: u64_at(input, 32),
                width: u32_at(input, 40),
                height: u32_at(input, 44),
                keyframe: input[5] & KEYFRAME_FLAG != 0,
                dropped_frames: u32_at(input, 48),
                color_space: color_from_code(input[56])?,
                codec: codec_from_code(input[57])?,
            },
            payload: input[LIVE_BINARY_HEADER_LEN..].to_vec(),
        };
        validate_binary_identity(frame.epoch, &frame.metadata)?;
        Ok(frame)
    }
}

fn validate_binary_identity(
    epoch: u64,
    metadata: &LiveFrameMetadata,
) -> Result<(), LiveProtocolError> {
    if epoch == 0 {
        return Err(LiveProtocolError::InvalidFrame("epoch"));
    }
    validate_frame_metadata(metadata).map_err(|_| LiveProtocolError::InvalidFrame("metadata"))
}

fn validate_payload_len(len: usize) -> Result<(), LiveProtocolError> {
    if len == 0 || len > LIVE_MAX_FRAME_PAYLOAD || len > u32::MAX as usize {
        return Err(LiveProtocolError::InvalidFrame("payload_length"));
    }
    Ok(())
}

fn u64_at(input: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(input[offset..offset + 8].try_into().expect("fixed header"))
}

fn u32_at(input: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(input[offset..offset + 4].try_into().expect("fixed header"))
}

fn color_code(value: LiveColorSpace) -> u8 {
    match value {
        LiveColorSpace::Srgb => 1,
        LiveColorSpace::Hdr10 => 2,
    }
}

fn color_from_code(value: u8) -> Result<LiveColorSpace, LiveProtocolError> {
    match value {
        1 => Ok(LiveColorSpace::Srgb),
        2 => Ok(LiveColorSpace::Hdr10),
        _ => Err(LiveProtocolError::InvalidFrame("color_space")),
    }
}

fn codec_code(value: LiveCodec) -> u8 {
    match value {
        LiveCodec::RawBgra => 1,
        LiveCodec::H264 => 2,
    }
}

fn codec_from_code(value: u8) -> Result<LiveCodec, LiveProtocolError> {
    match value {
        1 => Ok(LiveCodec::RawBgra),
        2 => Ok(LiveCodec::H264),
        _ => Err(LiveProtocolError::InvalidFrame("codec")),
    }
}
