// Phase 6 observation authority, capability, ordering, and snapshot contracts.
fn phase_six_store(session_id: &str) -> LiveSessionStore {
    let store = LiveSessionStore::new();
    let mut start = live_start_envelope(session_id, &format!("nonce:{session_id}"));
    let LiveControlMessage::SessionStart(message) = &mut start.message else {
        panic!("fixture must contain session_start");
    };
    message.session.observation_capabilities = vec![
        LiveObservationCapability::UiaTree,
        LiveObservationCapability::RangeValue,
        LiveObservationCapability::Value,
    ];
    store
        .create("device-source", start)
        .expect("create Phase 6 session");
    store
        .attach_viewer(
            "device-viewer",
            live_viewer_envelope(session_id, "device-viewer"),
        )
        .expect("attach Phase 6 viewer");
    store
}

fn phase_six_observation(
    session_id: &str,
    control_sequence: u64,
    observation_sequence: u64,
) -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence: control_sequence,
        message: LiveControlMessage::Observation(LiveObservation {
            observation_id: "uia:progress_bar:buildProgress".to_owned(),
            sequence: observation_sequence,
            state: loom_protocol::LiveObservationState::Stable,
            source: LiveObservationSource::UiAutomation,
            confidence: loom_protocol::LiveObservationConfidence::Exact,
            observed_at_ms: unix_time_millis(),
            stable_since_ms: Some(unix_time_millis()),
            locator: Some(loom_protocol::LiveElementLocator {
                automation_id: Some("buildProgress".to_owned()),
                name: None,
                control_type: "ProgressBar".to_owned(),
                ancestor_path: vec!["Window:fixture".to_owned()],
                runtime_id: Some(vec![42, 7]),
            }),
            value: Some(json!({ "value": 75.0, "minimum": 0.0, "maximum": 100.0 })),
            reason: None,
        }),
    }
}

#[test]
fn live_observations_are_source_only_reliable_and_visible_in_snapshots() {
    let session_id = "live:phase6-observation";
    let store = phase_six_store(session_id);
    let denied = store
        .publish_observation(
            "device-viewer",
            session_id,
            phase_six_observation(session_id, 2, 1),
        )
        .expect_err("viewer observation publishing must fail closed");
    assert_eq!(denied.code, "live_observation_source_required");

    let event = store
        .publish_observation(
            "device-source",
            session_id,
            phase_six_observation(session_id, 2, 1),
        )
        .expect("publish source observation");
    assert!(matches!(
        event.event.message,
        LiveControlMessage::Observation(_)
    ));
    let snapshot = store.get(session_id).expect("read observation snapshot");
    assert_eq!(snapshot.observations.len(), 1);
    assert_eq!(snapshot.observations[0].sequence, 1);
    let (_, events) = store
        .wait_events_after(session_id, "device-viewer", 0, Duration::ZERO)
        .expect("viewer reads reliable observations");
    assert!(events
        .iter()
        .any(|item| matches!(item.message, LiveControlMessage::Observation(_))));
}

#[test]
fn live_observation_sequence_rejection_is_atomic_and_capability_is_enforced() {
    let session_id = "live:phase6-order";
    let store = phase_six_store(session_id);
    store
        .publish_observation(
            "device-source",
            session_id,
            phase_six_observation(session_id, 2, 1),
        )
        .expect("publish first observation");
    let duplicate = store
        .publish_observation(
            "device-source",
            session_id,
            phase_six_observation(session_id, 3, 1),
        )
        .expect_err("duplicate observation sequence must fail");
    assert_eq!(duplicate.code, "live_observation_sequence_invalid");
    store
        .publish_observation(
            "device-source",
            session_id,
            phase_six_observation(session_id, 3, 2),
        )
        .expect("failed observation must not consume control sequence");

    let mut vision = phase_six_observation(session_id, 4, 3);
    let LiveControlMessage::Observation(value) = &mut vision.message else {
        unreachable!();
    };
    value.source = LiveObservationSource::Vision;
    let denied = store
        .publish_observation("device-source", session_id, vision)
        .expect_err("unadvertised observation backend must fail");
    assert_eq!(denied.code, "live_observation_capability_missing");
}

#[test]
fn live_observation_limit_rejection_does_not_consume_control_sequence() {
    let session_id = "live:phase6-limit";
    let store = phase_six_store(session_id);
    for index in 0..LIVE_OBSERVATION_LIMIT {
        let mut envelope = phase_six_observation(session_id, index as u64 + 2, 1);
        let LiveControlMessage::Observation(observation) = &mut envelope.message else {
            unreachable!();
        };
        observation.observation_id = format!("uia:item:{index}");
        store
            .publish_observation("device-source", session_id, envelope)
            .expect("publish bounded observation");
    }

    let mut overflow = phase_six_observation(session_id, LIVE_OBSERVATION_LIMIT as u64 + 2, 1);
    let LiveControlMessage::Observation(observation) = &mut overflow.message else {
        unreachable!();
    };
    observation.observation_id = "uia:item:overflow".to_owned();
    let denied = store
        .publish_observation("device-source", session_id, overflow)
        .expect_err("a new observation past the bound must fail closed");
    assert_eq!(denied.code, "live_observation_limit");

    let mut existing = phase_six_observation(session_id, LIVE_OBSERVATION_LIMIT as u64 + 2, 2);
    let LiveControlMessage::Observation(observation) = &mut existing.message else {
        unreachable!();
    };
    observation.observation_id = "uia:item:0".to_owned();
    store
        .publish_observation("device-source", session_id, existing)
        .expect("limit rejection must not consume the control sequence");
    assert_eq!(
        store
            .get(session_id)
            .expect("read bounded snapshot")
            .observations
            .len(),
        LIVE_OBSERVATION_LIMIT
    );
}

#[test]
fn live_observations_are_redacted_from_discovery_and_member_scoped() {
    let session_id = "live:phase6-read-scope";
    let store = phase_six_store(session_id);
    store
        .publish_observation(
            "device-source",
            session_id,
            phase_six_observation(session_id, 2, 1),
        )
        .expect("publish observation before discovery");
    assert!(store.list().expect("list discoverable live sessions")[0]
        .observations
        .is_empty());
    assert_eq!(
        store
            .get_for_member(session_id, "device-outsider")
            .expect_err("non-member snapshot read must fail closed")
            .code,
        "live_session_denied"
    );
    assert_eq!(
        store
            .get_for_member(session_id, "device-viewer")
            .expect("attached viewer reads snapshot")
            .observations
            .len(),
        1
    );
}
