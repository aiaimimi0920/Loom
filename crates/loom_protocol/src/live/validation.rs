use thiserror::Error;

use super::{
    trigger_validation::{
        validate_trigger_audit, validate_trigger_binding, validate_trigger_condition,
    },
    LiveControlEnvelope, LiveControlMessage, LiveFrameMetadata, LiveInputEvent, LiveInputKind,
    LiveObservation, LiveObservationConfidence, LiveObservationSource, LiveObservationState,
    LiveScreenshotSession, LIVE_MAX_DIMENSION, LIVE_MAX_OBSERVATION_VALUE,
    LIVE_MAX_TRIGGER_BINDINGS, LIVE_MAX_VIEWERS, LIVE_PROTOCOL_VERSION,
};

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LiveProtocolError {
    #[error("unsupported live protocol version: {0}")]
    UnsupportedProtocolVersion(String),
    #[error("unsupported live binary version: {0}")]
    UnsupportedBinaryVersion(u8),
    #[error("invalid live field: {0}")]
    InvalidField(&'static str),
    #[error("invalid live identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid live sequence for {stream}: expected {expected}, received {actual}")]
    InvalidSequence {
        stream: &'static str,
        expected: u64,
        actual: u64,
    },
    #[error("live frame is truncated")]
    TruncatedFrame,
    #[error("invalid live frame field: {0}")]
    InvalidFrame(&'static str),
    #[error("live frame payload length mismatch")]
    PayloadLengthMismatch,
}

pub fn validate_live_identifier(value: &str, field: &'static str) -> Result<(), LiveProtocolError> {
    if value.is_empty()
        || value.len() > 160
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        return Err(LiveProtocolError::InvalidIdentifier(field));
    }
    Ok(())
}

pub fn validate_live_session(session: &LiveScreenshotSession) -> Result<(), LiveProtocolError> {
    if session.protocol_version != LIVE_PROTOCOL_VERSION {
        return Err(LiveProtocolError::UnsupportedProtocolVersion(
            session.protocol_version.clone(),
        ));
    }
    for (value, field) in [
        (&session.session_id, "session_id"),
        (&session.source_device_id, "source_device_id"),
        (&session.source_hook_id, "source_hook_id"),
        (&session.source_window_identity.window_id, "window_id"),
        (&session.frame_stream.stream_id, "stream_id"),
    ] {
        validate_live_identifier(value, field)?;
    }
    if session
        .source_window_identity
        .title
        .as_ref()
        .is_some_and(|title| title.len() > 512)
    {
        return Err(LiveProtocolError::InvalidField("window_title"));
    }
    validate_dimensions(session.source_region.width, session.source_region.height)?;
    validate_dimensions(session.frame_stream.width, session.frame_stream.height)?;
    if !(1..=120).contains(&session.frame_stream.target_fps) {
        return Err(LiveProtocolError::InvalidField("target_fps"));
    }
    if !(2..=3).contains(&session.frame_stream.max_buffered_frames) {
        return Err(LiveProtocolError::InvalidField("max_buffered_frames"));
    }
    if !(1..=3_600).contains(&session.frame_stream.keyframe_interval) {
        return Err(LiveProtocolError::InvalidField("keyframe_interval"));
    }
    if session
        .frame_stream
        .endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.len() > 2_048)
    {
        return Err(LiveProtocolError::InvalidField("frame_stream_endpoint"));
    }
    validate_unique(
        &session.interaction_capabilities,
        "interaction_capabilities",
    )?;
    validate_unique(
        &session.observation_capabilities,
        "observation_capabilities",
    )?;
    if session.trigger_bindings.len() > LIVE_MAX_TRIGGER_BINDINGS {
        return Err(LiveProtocolError::InvalidField("trigger_bindings"));
    }
    for binding in &session.trigger_bindings {
        validate_trigger_binding(binding)?;
    }
    if session.viewer_devices.len() > LIVE_MAX_VIEWERS {
        return Err(LiveProtocolError::InvalidField("viewer_devices"));
    }
    for viewer in &session.viewer_devices {
        validate_live_identifier(viewer, "viewer_device_id")?;
    }
    if let Some(controller) = &session.controller_device {
        validate_live_identifier(controller, "controller_device_id")?;
    }
    if session.last_seen_at_ms < session.created_at_ms {
        return Err(LiveProtocolError::InvalidField("last_seen_at_ms"));
    }
    Ok(())
}

pub fn validate_control_envelope(envelope: &LiveControlEnvelope) -> Result<(), LiveProtocolError> {
    if envelope.protocol_version != LIVE_PROTOCOL_VERSION {
        return Err(LiveProtocolError::UnsupportedProtocolVersion(
            envelope.protocol_version.clone(),
        ));
    }
    validate_live_identifier(&envelope.session_id, "session_id")?;
    if envelope.epoch == 0 || envelope.sequence == 0 {
        return Err(LiveProtocolError::InvalidField("epoch_or_sequence"));
    }
    match &envelope.message {
        LiveControlMessage::SessionStart(payload) => {
            validate_live_session(&payload.session)?;
            if payload.session.session_id != envelope.session_id {
                return Err(LiveProtocolError::InvalidField("session_id_mismatch"));
            }
            validate_live_identifier(&payload.requested_by_device_id, "requested_by_device_id")?;
            validate_live_identifier(&payload.request_nonce, "request_nonce")?;
        }
        LiveControlMessage::SessionAck(payload) => {
            validate_live_identifier(&payload.responder_device_id, "responder_device_id")?;
            validate_optional_reason(payload.reason.as_deref())?;
        }
        LiveControlMessage::SessionState(payload) => {
            if payload.viewers.len() > LIVE_MAX_VIEWERS {
                return Err(LiveProtocolError::InvalidField("viewers"));
            }
            for viewer in &payload.viewers {
                validate_live_identifier(viewer, "viewer_device_id")?;
            }
            if let Some(controller) = &payload.controller_device_id {
                validate_live_identifier(controller, "controller_device_id")?;
            }
            validate_optional_reason(payload.reason.as_deref())?;
        }
        LiveControlMessage::FrameNotice(payload) => validate_frame_metadata(&payload.metadata)?,
        LiveControlMessage::InputEvent(payload) => validate_input_event(payload)?,
        LiveControlMessage::Observation(payload) => validate_observation(payload)?,
        LiveControlMessage::TriggerCondition(payload) => {
            validate_trigger_condition(payload)?;
        }
        LiveControlMessage::TriggerEvent(payload) => {
            validate_trigger_audit(payload)?;
        }
        LiveControlMessage::ControlTransfer(payload) => {
            if let Some(controller) = &payload.previous_controller_device_id {
                validate_live_identifier(controller, "previous_controller_device_id")?;
            }
            if let Some(controller) = &payload.controller_device_id {
                validate_live_identifier(controller, "controller_device_id")?;
            }
            validate_optional_reason(Some(&payload.reason))?;
        }
        LiveControlMessage::ResumeRequest(payload) => {
            validate_live_identifier(&payload.requester_device_id, "requester_device_id")?;
        }
        LiveControlMessage::SessionEnd(payload) => {
            validate_live_identifier(&payload.ended_by_device_id, "ended_by_device_id")?;
            validate_optional_reason(payload.detail.as_deref())?;
        }
    }
    Ok(())
}

pub fn validate_frame_metadata(metadata: &LiveFrameMetadata) -> Result<(), LiveProtocolError> {
    if metadata.frame_id == 0 || metadata.encode_timestamp_ms < metadata.capture_timestamp_ms {
        return Err(LiveProtocolError::InvalidField("frame_metadata"));
    }
    validate_dimensions(metadata.width, metadata.height)
}

pub fn validate_input_event(event: &LiveInputEvent) -> Result<(), LiveProtocolError> {
    if event.input_sequence == 0 {
        return Err(LiveProtocolError::InvalidField("input_sequence"));
    }
    validate_live_identifier(&event.source_device_id, "source_device_id")?;
    match &event.kind {
        LiveInputKind::MouseMove(value) => validate_point(value.x, value.y),
        LiveInputKind::MouseButton(value) => {
            if !(1..=2).contains(&value.click_count) {
                return Err(LiveProtocolError::InvalidField("click_count"));
            }
            validate_point(value.x, value.y)
        }
        LiveInputKind::Wheel(value) => validate_point(value.x, value.y),
        LiveInputKind::Key(value) => {
            if value.code.is_empty() || value.code.len() > 80 {
                return Err(LiveProtocolError::InvalidField("key_code"));
            }
            Ok(())
        }
        LiveInputKind::Text(value) => {
            if value.text.is_empty() || value.text.chars().count() > 4096 {
                return Err(LiveProtocolError::InvalidField("text"));
            }
            Ok(())
        }
        LiveInputKind::Focus(_) | LiveInputKind::Cancel => Ok(()),
    }
}

pub fn validate_observation(value: &LiveObservation) -> Result<(), LiveProtocolError> {
    validate_live_identifier(&value.observation_id, "observation_id")?;
    if value.sequence == 0 || value.observed_at_ms == 0 {
        return Err(LiveProtocolError::InvalidField(
            "observation_sequence_or_timestamp",
        ));
    }
    if matches!(
        value.state,
        LiveObservationState::Unknown | LiveObservationState::Stale | LiveObservationState::Error
    ) && value.value.is_some()
    {
        return Err(LiveProtocolError::InvalidField(
            "untrusted_observation_value",
        ));
    }
    if matches!(value.source, LiveObservationSource::Unknown)
        && !matches!(value.confidence, LiveObservationConfidence::Low)
    {
        return Err(LiveProtocolError::InvalidField("unknown_source_confidence"));
    }
    if matches!(value.source, LiveObservationSource::Vision)
        && matches!(value.confidence, LiveObservationConfidence::Exact)
    {
        return Err(LiveProtocolError::InvalidField("vision_source_confidence"));
    }
    if matches!(value.source, LiveObservationSource::UiAutomation) && value.locator.is_none() {
        return Err(LiveProtocolError::InvalidField("uia_observation_locator"));
    }
    if value.value.as_ref().is_some_and(|payload| {
        serde_json::to_vec(payload)
            .map(|bytes| bytes.len() > LIVE_MAX_OBSERVATION_VALUE)
            .unwrap_or(true)
    }) {
        return Err(LiveProtocolError::InvalidField("observation_value"));
    }
    if value
        .stable_since_ms
        .is_some_and(|stable_since| stable_since > value.observed_at_ms)
    {
        return Err(LiveProtocolError::InvalidField("stable_since_ms"));
    }
    if let Some(locator) = &value.locator {
        validate_bounded_text(&locator.control_type, 160, "locator_control_type")?;
        for (field, text) in [
            ("locator_automation_id", locator.automation_id.as_deref()),
            ("locator_name", locator.name.as_deref()),
        ] {
            if let Some(text) = text {
                validate_bounded_text(text, 512, field)?;
            }
        }
        if locator.ancestor_path.len() > 64
            || locator.runtime_id.as_ref().is_some_and(|v| v.len() > 64)
        {
            return Err(LiveProtocolError::InvalidField("locator_path"));
        }
        for ancestor in &locator.ancestor_path {
            validate_bounded_text(ancestor, 512, "locator_ancestor")?;
        }
    }
    validate_optional_reason(value.reason.as_deref())?;
    Ok(())
}

fn validate_unique<T: PartialEq>(
    values: &[T],
    field: &'static str,
) -> Result<(), LiveProtocolError> {
    if values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
    {
        return Err(LiveProtocolError::InvalidField(field));
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    max_len: usize,
    field: &'static str,
) -> Result<(), LiveProtocolError> {
    if value.is_empty() || value.len() > max_len {
        return Err(LiveProtocolError::InvalidField(field));
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), LiveProtocolError> {
    if width == 0 || height == 0 || width > LIVE_MAX_DIMENSION || height > LIVE_MAX_DIMENSION {
        return Err(LiveProtocolError::InvalidField("dimensions"));
    }
    Ok(())
}

fn validate_point(x: f64, y: f64) -> Result<(), LiveProtocolError> {
    if !x.is_finite() || !y.is_finite() || x.abs() > 1_000_000.0 || y.abs() > 1_000_000.0 {
        return Err(LiveProtocolError::InvalidField("pointer_position"));
    }
    Ok(())
}

pub(super) fn validate_optional_reason(reason: Option<&str>) -> Result<(), LiveProtocolError> {
    if reason.is_some_and(|value| value.is_empty() || value.len() > 512) {
        return Err(LiveProtocolError::InvalidField("reason"));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveSequenceTracker {
    epoch: u64,
    control_sequence: u64,
    frame_id: u64,
    input_sequence: u64,
}

impl LiveSequenceTracker {
    pub fn new(epoch: u64) -> Result<Self, LiveProtocolError> {
        if epoch == 0 {
            return Err(LiveProtocolError::InvalidField("epoch"));
        }
        Ok(Self {
            epoch,
            control_sequence: 0,
            frame_id: 0,
            input_sequence: 0,
        })
    }

    pub fn accept_control(&mut self, epoch: u64, sequence: u64) -> Result<(), LiveProtocolError> {
        self.accept_exact(epoch, sequence, "control")?;
        self.control_sequence = sequence;
        Ok(())
    }

    pub fn accept_input(&mut self, epoch: u64, sequence: u64) -> Result<(), LiveProtocolError> {
        self.check_epoch(epoch)?;
        let expected = self
            .input_sequence
            .checked_add(1)
            .ok_or(LiveProtocolError::InvalidField("input_sequence_exhausted"))?;
        if sequence != expected {
            return Err(LiveProtocolError::InvalidSequence {
                stream: "input",
                expected,
                actual: sequence,
            });
        }
        self.input_sequence = sequence;
        Ok(())
    }

    pub fn accept_frame(&mut self, epoch: u64, frame_id: u64) -> Result<u64, LiveProtocolError> {
        self.check_epoch(epoch)?;
        if frame_id <= self.frame_id {
            return Err(LiveProtocolError::InvalidSequence {
                stream: "frame",
                expected: self.frame_id.saturating_add(1),
                actual: frame_id,
            });
        }
        let dropped = frame_id.saturating_sub(self.frame_id.saturating_add(1));
        self.frame_id = frame_id;
        Ok(dropped)
    }

    pub fn resume(
        &mut self,
        epoch: u64,
        control: u64,
        frame: u64,
        input: u64,
    ) -> Result<(), LiveProtocolError> {
        if epoch <= self.epoch {
            return Err(LiveProtocolError::InvalidField("resume_epoch"));
        }
        self.epoch = epoch;
        self.control_sequence = control;
        self.frame_id = frame;
        self.input_sequence = input;
        Ok(())
    }

    fn accept_exact(
        &self,
        epoch: u64,
        sequence: u64,
        stream: &'static str,
    ) -> Result<(), LiveProtocolError> {
        self.check_epoch(epoch)?;
        let expected =
            self.control_sequence
                .checked_add(1)
                .ok_or(LiveProtocolError::InvalidField(
                    "control_sequence_exhausted",
                ))?;
        if sequence != expected {
            return Err(LiveProtocolError::InvalidSequence {
                stream,
                expected,
                actual: sequence,
            });
        }
        Ok(())
    }

    fn check_epoch(&self, epoch: u64) -> Result<(), LiveProtocolError> {
        if epoch != self.epoch {
            return Err(LiveProtocolError::InvalidField("epoch"));
        }
        Ok(())
    }
}
