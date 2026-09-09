use serde_json::json;

use crate::schemas;

use super::*;

fn sample_session() -> LiveScreenshotSession {
    LiveScreenshotSession {
        protocol_version: LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live-session-1".to_owned(),
        source_device_id: "device-a".to_owned(),
        source_hook_id: "hook-a".to_owned(),
        source_kind: LiveSourceKind::Window,
        source_window_identity: LiveWindowIdentity {
            window_id: "0x1234".to_owned(),
            process_id: 42,
            process_started_at_ms: Some(1_000),
            title: Some("Fixture".to_owned()),
        },
        source_region: LiveRect {
            x: 0,
            y: 0,
            width: 640,
            height: 360,
        },
        region_anchor: LiveRegionAnchor::Window,
        frame_stream: LiveFrameStreamDescriptor {
            stream_id: "stream-1".to_owned(),
            transport: LiveMediaTransport::WebsocketBinary,
            endpoint: Some("wss://127.0.0.1/v1/live/media".to_owned()),
            codec: LiveCodec::H264,
            color_space: LiveColorSpace::Srgb,
            width: 640,
            height: 360,
            target_fps: 30,
            max_buffered_frames: 3,
            keyframe_interval: 60,
        },
        interaction_capabilities: vec![
            LiveInteractionCapability::PointerMove,
            LiveInteractionCapability::PointerButton,
        ],
        observation_capabilities: vec![LiveObservationCapability::UiaTree],
        trigger_bindings: Vec::new(),
        viewer_devices: vec!["device-b".to_owned()],
        controller_device: None,
        visibility_state: LiveVisibilityState::Visible,
        capture_strategy: LiveCaptureStrategy::PersistentWindowWgc,
        render_preservation_strategy: LiveRenderPreservationStrategy::VisibleOffscreen,
        revision: 1,
        created_at_ms: 2_000,
        last_seen_at_ms: 2_000,
    }
}

fn sample_envelope() -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live-session-1".to_owned(),
        epoch: 1,
        sequence: 1,
        message: LiveControlMessage::SessionStart(LiveSessionStart {
            session: sample_session(),
            requested_by_device_id: "device-a".to_owned(),
            request_nonce: "nonce-1".to_owned(),
        }),
    }
}

fn sample_frame() -> LiveBinaryFrame {
    LiveBinaryFrame {
        epoch: 7,
        metadata: LiveFrameMetadata {
            frame_id: 11,
            capture_timestamp_ms: 10_000,
            encode_timestamp_ms: 10_004,
            width: 640,
            height: 360,
            keyframe: true,
            dropped_frames: 2,
            color_space: LiveColorSpace::Srgb,
            codec: LiveCodec::H264,
        },
        payload: vec![1, 2, 3, 4],
    }
}

#[test]
fn live_control_round_trips_and_validates_against_schema() {
    let envelope = sample_envelope();
    validate_control_envelope(&envelope).expect("valid control envelope");
    let value = serde_json::to_value(&envelope).expect("serialize control envelope");
    let schema = serde_json::from_str(schemas::LIVE_CONTROL_V1).expect("schema JSON");
    let validator = jsonschema::validator_for(&schema).expect("valid live schema");
    assert!(validator.is_valid(&value), "{value:#}");
    let decoded: LiveControlEnvelope = serde_json::from_value(value).expect("decode envelope");
    assert_eq!(decoded, envelope);
}

#[test]
fn schema_and_serde_reject_unknown_fields_and_message_types() {
    let schema = serde_json::from_str(schemas::LIVE_CONTROL_V1).expect("schema JSON");
    let validator = jsonschema::validator_for(&schema).expect("valid live schema");
    let mut value = serde_json::to_value(sample_envelope()).expect("serialize envelope");
    value["unexpected"] = json!(true);
    assert!(!validator.is_valid(&value));
    assert!(serde_json::from_value::<LiveControlEnvelope>(value).is_err());

    let mut unknown = serde_json::to_value(sample_envelope()).expect("serialize envelope");
    unknown["messageType"] = json!("future_message");
    assert!(!validator.is_valid(&unknown));
    assert!(serde_json::from_value::<LiveControlEnvelope>(unknown).is_err());

    let mut missing_version = serde_json::to_value(sample_envelope()).expect("serialize envelope");
    missing_version
        .as_object_mut()
        .expect("envelope object")
        .remove("protocolVersion");
    assert!(!validator.is_valid(&missing_version));
    assert!(serde_json::from_value::<LiveControlEnvelope>(missing_version).is_err());
}

#[test]
fn protocol_versions_and_session_bounds_fail_closed() {
    let mut envelope = sample_envelope();
    envelope.protocol_version = "loom.live.v0".to_owned();
    assert_eq!(
        validate_control_envelope(&envelope),
        Err(LiveProtocolError::UnsupportedProtocolVersion(
            "loom.live.v0".to_owned()
        ))
    );

    let mut session = sample_session();
    session.frame_stream.max_buffered_frames = 4;
    assert_eq!(
        validate_live_session(&session),
        Err(LiveProtocolError::InvalidField("max_buffered_frames"))
    );
    session.frame_stream.max_buffered_frames = 3;
    session.source_region.width = LIVE_MAX_DIMENSION + 1;
    assert_eq!(
        validate_live_session(&session),
        Err(LiveProtocolError::InvalidField("dimensions"))
    );
}

#[test]
fn control_and_input_are_exactly_ordered_while_frames_may_drop() {
    let mut tracker = LiveSequenceTracker::new(4).expect("tracker");
    tracker.accept_control(4, 1).expect("first control");
    assert!(matches!(
        tracker.accept_control(4, 3),
        Err(LiveProtocolError::InvalidSequence {
            stream: "control",
            expected: 2,
            actual: 3
        })
    ));
    tracker.accept_input(4, 1).expect("first input");
    assert!(tracker.accept_input(4, 3).is_err());
    assert_eq!(tracker.accept_frame(4, 1).expect("first frame"), 0);
    assert_eq!(tracker.accept_frame(4, 4).expect("frame gap"), 2);
    assert!(tracker.accept_frame(4, 4).is_err());
    tracker
        .resume(5, 9, 20, 3)
        .expect("new epoch resumes from snapshot");
    tracker
        .accept_control(5, 10)
        .expect("continued control sequence");
}

#[test]
fn binary_frame_round_trips_and_rejects_malformed_lengths() {
    let frame = sample_frame();
    let encoded = frame.encode().expect("encode frame");
    assert_eq!(
        LiveBinaryFrame::decode(&encoded).expect("decode frame"),
        frame
    );
    assert_eq!(
        LiveBinaryFrame::decode(&encoded[..20]),
        Err(LiveProtocolError::TruncatedFrame)
    );

    let mut mismatched = encoded.clone();
    mismatched[52..56].copy_from_slice(&99u32.to_be_bytes());
    assert_eq!(
        LiveBinaryFrame::decode(&mismatched),
        Err(LiveProtocolError::PayloadLengthMismatch)
    );
    let mut oversized = encoded;
    oversized[52..56].copy_from_slice(&((LIVE_MAX_FRAME_PAYLOAD as u32) + 1).to_be_bytes());
    assert_eq!(
        LiveBinaryFrame::decode(&oversized),
        Err(LiveProtocolError::InvalidFrame("payload_length"))
    );
}

#[test]
fn binary_frame_rejects_invalid_metadata_and_unknown_flags() {
    let mut invalid = sample_frame();
    invalid.epoch = 0;
    assert_eq!(
        invalid.encode(),
        Err(LiveProtocolError::InvalidFrame("epoch"))
    );
    invalid.epoch = 1;
    invalid.metadata.width = 0;
    assert_eq!(
        invalid.encode(),
        Err(LiveProtocolError::InvalidFrame("metadata"))
    );

    let mut encoded = sample_frame().encode().expect("encode valid frame");
    encoded[5] = 2;
    assert_eq!(
        LiveBinaryFrame::decode(&encoded),
        Err(LiveProtocolError::InvalidFrame("flags"))
    );
}

#[test]
fn sequence_exhaustion_returns_an_error_without_overflow() {
    let mut tracker = LiveSequenceTracker::new(1).expect("tracker");
    tracker
        .resume(2, u64::MAX, 0, u64::MAX)
        .expect("resume at exhausted counters");
    assert_eq!(
        tracker.accept_control(2, 1),
        Err(LiveProtocolError::InvalidField(
            "control_sequence_exhausted"
        ))
    );
    assert_eq!(
        tracker.accept_input(2, 1),
        Err(LiveProtocolError::InvalidField("input_sequence_exhausted"))
    );
}

#[test]
fn unreliable_observations_cannot_carry_authoritative_values() {
    let observation = LiveObservation {
        observation_id: "progress-1".to_owned(),
        sequence: 1,
        state: LiveObservationState::Unknown,
        source: LiveObservationSource::Unknown,
        confidence: LiveObservationConfidence::Low,
        observed_at_ms: 10,
        stable_since_ms: None,
        locator: None,
        value: Some(json!(100)),
        reason: Some("provider unavailable".to_owned()),
    };
    assert_eq!(
        validate_observation(&observation),
        Err(LiveProtocolError::InvalidField(
            "untrusted_observation_value"
        ))
    );
}

#[test]
fn visual_observations_cannot_claim_exact_confidence() {
    let observation = LiveObservation {
        observation_id: "vision:progress-1".to_owned(),
        sequence: 1,
        state: LiveObservationState::Stable,
        source: LiveObservationSource::Vision,
        confidence: LiveObservationConfidence::Exact,
        observed_at_ms: 10,
        stable_since_ms: Some(10),
        locator: None,
        value: Some(json!(100)),
        reason: None,
    };
    assert_eq!(
        validate_observation(&observation),
        Err(LiveProtocolError::InvalidField("vision_source_confidence"))
    );
}

#[test]
fn exact_uia_observations_require_element_provenance() {
    let observation = LiveObservation {
        observation_id: "uia:progress-1".to_owned(),
        sequence: 1,
        state: LiveObservationState::Stable,
        source: LiveObservationSource::UiAutomation,
        confidence: LiveObservationConfidence::Exact,
        observed_at_ms: 10,
        stable_since_ms: Some(10),
        locator: None,
        value: Some(json!(100)),
        reason: None,
    };
    assert_eq!(
        validate_observation(&observation),
        Err(LiveProtocolError::InvalidField("uia_observation_locator"))
    );
}

#[test]
fn observation_timestamp_and_value_size_are_bounded() {
    let mut observation = LiveObservation {
        observation_id: "progress-1".to_owned(),
        sequence: 1,
        state: LiveObservationState::Stable,
        source: LiveObservationSource::UiAutomation,
        confidence: LiveObservationConfidence::Exact,
        observed_at_ms: 0,
        stable_since_ms: None,
        locator: Some(LiveElementLocator {
            automation_id: Some("progress-1".to_owned()),
            name: None,
            control_type: "ProgressBar".to_owned(),
            ancestor_path: Vec::new(),
            runtime_id: None,
        }),
        value: Some(json!(1)),
        reason: None,
    };
    assert_eq!(
        validate_observation(&observation),
        Err(LiveProtocolError::InvalidField(
            "observation_sequence_or_timestamp"
        ))
    );
    observation.observed_at_ms = 1;
    observation.value = Some(json!("x".repeat(LIVE_MAX_OBSERVATION_VALUE)));
    assert_eq!(
        validate_observation(&observation),
        Err(LiveProtocolError::InvalidField("observation_value"))
    );
}

#[test]
fn only_pointer_moves_are_coalescible() {
    let move_event = LiveInputEvent {
        input_sequence: 1,
        issued_at_ms: 10,
        source_device_id: "device-b".to_owned(),
        kind: LiveInputKind::MouseMove(LivePointerPosition { x: 3.0, y: 4.0 }),
    };
    assert!(move_event.may_coalesce());
    let mut edge = move_event;
    edge.kind = LiveInputKind::MouseButton(LiveMouseButtonInput {
        button: LiveMouseButton::Left,
        state: LiveButtonState::Pressed,
        x: 3.0,
        y: 4.0,
        click_count: 1,
    });
    assert!(!edge.may_coalesce());
}

#[test]
fn trigger_conditions_bound_selectors_types_and_shape() {
    let mut condition = LiveTriggerCondition {
        condition_id: "condition:progress".to_owned(),
        revision: 1,
        observation_id: "uia:progress".to_owned(),
        operator: LiveConditionOperator::GreaterOrEqual,
        operand: json!({ "path": "/rangeValue/value", "value": 100 }),
        stable_for_ms: 1_000,
        rising_edge: true,
        rearm: true,
        minimum_confidence: LiveObservationConfidence::Exact,
    };
    validate_trigger_condition(&condition).expect("valid numeric selector");

    condition.operand = json!({ "path": "/value", "value": "100" });
    assert_eq!(
        validate_trigger_condition(&condition),
        Err(LiveProtocolError::InvalidField("trigger_operand_type"))
    );
    condition.operator = LiveConditionOperator::Equals;
    condition.operand = json!({ "path": "/", "value": 100 });
    assert_eq!(
        validate_trigger_condition(&condition),
        Err(LiveProtocolError::InvalidField("trigger_operand_selector"))
    );
    condition.operand = json!({ "path": "/value", "value": "x".repeat(LIVE_MAX_TRIGGER_OPERAND) });
    assert_eq!(
        validate_trigger_condition(&condition),
        Err(LiveProtocolError::InvalidField("trigger_operand"))
    );
}

#[test]
fn trigger_event_trace_round_trips_through_schema() {
    let envelope = LiveControlEnvelope {
        protocol_version: LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: "live-session-1".to_owned(),
        epoch: 1,
        sequence: 2,
        message: LiveControlMessage::TriggerEvent(LiveTriggerAudit {
            trigger_id: "trigger:progress".to_owned(),
            binding_id: "binding:progress".to_owned(),
            condition_revision: 3,
            observation_id: "uia:progress".to_owned(),
            observation_sequence: 101,
            source_device_id: "device-a".to_owned(),
            observation_source: LiveObservationSource::UiAutomation,
            idempotency_key: "trigger:0123456789abcdef".to_owned(),
            outcome: LiveTriggerOutcome::Fired,
            evaluated_at_ms: 12_345,
            authorized_by: "device-b".to_owned(),
            action_request_id: Some("request:surface-action".to_owned()),
            reason: Some("surface_action_accepted".to_owned()),
        }),
    };
    validate_control_envelope(&envelope).expect("valid trigger event");
    let schema = serde_json::from_str(schemas::LIVE_CONTROL_V1).expect("schema JSON");
    let validator = jsonschema::validator_for(&schema).expect("valid live schema");
    let value = serde_json::to_value(envelope).expect("serialize trigger event");
    assert!(validator.is_valid(&value), "{value:#}");
}
