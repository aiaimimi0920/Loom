//! Connection-local control for the explicitly negotiated continuous-video profile.
use serde::{Deserialize, Serialize};

pub const LIVE_H264_PROTOCOL_VERSION: &str = "loom.live.h264.v1";
pub const LIVE_VIDEO_CONTROL_MAX_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveVideoViewerControl {
    KeyframeRequest { epoch: u64 },
    VideoFallback { epoch: u64 },
}

impl LiveVideoViewerControl {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > LIVE_VIDEO_CONTROL_MAX_BYTES {
            return None;
        }
        let value: Self = serde_json::from_str(text).ok()?;
        (value.epoch() > 0).then_some(value)
    }

    pub fn epoch(self) -> u64 {
        match self {
            Self::KeyframeRequest { epoch } | Self::VideoFallback { epoch } => epoch,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveVideoSourceControl {
    VideoPolicy {
        epoch: u64,
        h264_allowed: bool,
        keyframe_sequence: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_control_has_exact_bounded_role_specific_shape() {
        let text = r#"{"type":"keyframe_request","epoch":1}"#;
        assert_eq!(
            LiveVideoViewerControl::parse(text),
            Some(LiveVideoViewerControl::KeyframeRequest { epoch: 1 })
        );
        for invalid in [
            r#"{"type":"keyframe_request","epoch":0}"#,
            r#"{"type":"keyframe_request","epoch":1,"deviceId":"other"}"#,
            r#"{"type":"video_policy","epoch":1,"h264_allowed":true,"keyframe_sequence":1}"#,
            r#"{"type":"video_fallback","epoch":-1}"#,
        ] {
            assert!(LiveVideoViewerControl::parse(invalid).is_none());
        }
        assert!(LiveVideoViewerControl::parse(&format!("{text}{}", " ".repeat(256))).is_none());
        let policy = LiveVideoSourceControl::VideoPolicy {
            epoch: 1,
            h264_allowed: true,
            keyframe_sequence: 3,
        };
        let encoded = serde_json::to_string(&policy).unwrap();
        assert_eq!(
            serde_json::from_str::<LiveVideoSourceControl>(&encoded).unwrap(),
            policy
        );
    }
}
