//! C1 Annex-B access-unit boundary. This is bounded syntax validation, not a decoder.
use super::{LiveCodec, LiveColorSpace, LiveFrameMetadata, LiveProtocolError};

pub const LIVE_MAX_H264_PAYLOAD: usize = 1024 * 1024;

pub(super) fn validate_h264(
    metadata: &LiveFrameMetadata,
    bytes: &[u8],
) -> Result<(), LiveProtocolError> {
    if metadata.codec != LiveCodec::H264 {
        return Ok(());
    }
    let fail = || LiveProtocolError::InvalidFrame("h264_profile");
    if bytes.is_empty()
        || bytes.len() > LIVE_MAX_H264_PAYLOAD
        || metadata.color_space != LiveColorSpace::Srgb
        || metadata.width < 2
        || metadata.height < 2
        || metadata.width % 2 != 0
        || metadata.height % 2 != 0
        || metadata.width > 4096
        || metadata.height > 4096
        || u64::from(metadata.width) * u64::from(metadata.height) > 8_294_400
    {
        return Err(fail());
    }
    let prefix = |at: usize| {
        if bytes.get(at..at + 4) == Some(&[0, 0, 0, 1]) {
            Some(4)
        } else if bytes.get(at..at + 3) == Some(&[0, 0, 1]) {
            Some(3)
        } else {
            None
        }
    };
    let (mut sps, mut pps, mut idr, mut delta) = (false, false, false, false);
    let (mut cursor, mut count) = (0, 0);
    while cursor < bytes.len() {
        let begin = cursor + prefix(cursor).ok_or_else(fail)?;
        let end = (begin..bytes.len())
            .find(|&at| prefix(at).is_some())
            .unwrap_or(bytes.len());
        if end <= begin || bytes[begin] & 0x80 != 0 {
            return Err(fail());
        }
        match bytes[begin] & 31 {
            1 => {
                if idr || end - begin < 2 {
                    return Err(fail());
                }
                delta = true;
            }
            5 => {
                if delta || !sps || !pps || end - begin < 2 {
                    return Err(fail());
                }
                idr = true;
            }
            7 => {
                if idr || delta || end - begin < 4 {
                    return Err(fail());
                }
                sps = true;
            }
            8 => {
                if idr || delta || end - begin < 2 {
                    return Err(fail());
                }
                pps = true;
            }
            6 | 9 | 10 | 11 | 12 => {}
            _ => return Err(fail()),
        }
        count += 1;
        if count > 256 {
            return Err(fail());
        }
        cursor = end;
    }
    if (!idr && !delta) || metadata.keyframe != idr || (delta && (sps || pps)) {
        return Err(fail());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata(keyframe: bool) -> LiveFrameMetadata {
        LiveFrameMetadata {
            frame_id: 1,
            capture_timestamp_ms: 1,
            encode_timestamp_ms: 2,
            width: 320,
            height: 240,
            keyframe,
            dropped_frames: 0,
            color_space: LiveColorSpace::Srgb,
            codec: LiveCodec::H264,
        }
    }
    const IDR: &[u8] = &[
        0, 0, 0, 1, 0x67, 0x42, 0, 30, 0, 0, 1, 0x68, 1, 0, 0, 1, 0x65, 1,
    ];
    #[test]
    fn h264_accepts_bounded_idr_headers_and_delta_and_rejects_false_flags() {
        assert!(validate_h264(&metadata(true), IDR).is_ok());
        assert!(validate_h264(&metadata(false), &[0, 0, 1, 0x41, 1]).is_ok());
        assert!(validate_h264(&metadata(false), IDR).is_err());
        assert!(validate_h264(&metadata(true), &[0, 0, 1, 0x41, 1]).is_err());
        for invalid in [
            vec![],
            vec![0, 0, 1, 0x65, 1],
            vec![0, 0, 1, 0xe5, 1],
            vec![0; LIVE_MAX_H264_PAYLOAD + 1],
            [0, 0, 1, 9].repeat(257),
        ] {
            assert!(validate_h264(&metadata(true), &invalid).is_err());
        }
    }
    #[test]
    fn h264_rejects_profile_changes_and_headers_after_picture() {
        for (width, height) in [(321, 240), (0, 240), (4096, 4096)] {
            assert!(validate_h264(
                &LiveFrameMetadata {
                    width,
                    height,
                    ..metadata(true)
                },
                IDR
            )
            .is_err());
        }
        assert!(validate_h264(
            &LiveFrameMetadata {
                color_space: LiveColorSpace::Hdr10,
                ..metadata(true)
            },
            IDR
        )
        .is_err());
        let mut mixed = IDR.to_vec();
        mixed.extend_from_slice(&[0, 0, 1, 0x41, 1]);
        assert!(validate_h264(&metadata(true), &mixed).is_err());
    }
}
