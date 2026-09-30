// Phase 7/8 deterministic trigger evaluation, offline policy, and trust safety.
fn phase_seven_trigger_envelope(
    session_id: &str,
    revision: u64,
    rising_edge: bool,
    stable_for_ms: u32,
) -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence: revision + 1,
        message: LiveControlMessage::TriggerCondition(LiveTriggerCondition {
            condition_id: "condition:progress-complete".to_owned(),
            revision,
            observation_id: "uia:progress_bar:buildProgress".to_owned(),
            operator: LiveConditionOperator::GreaterOrEqual,
            operand: json!({ "path": "/value", "value": 100 }),
            stable_for_ms,
            rising_edge,
            rearm: true,
            minimum_confidence: LiveObservationConfidence::Exact,
        }),
    }
}

fn phase_seven_register(
    store: &LiveSessionStore,
    session_id: &str,
    rising_edge: bool,
    stable_for_ms: u32,
) {
    store
        .upsert_trigger_binding(
            "device-viewer",
            "binding:progress-complete",
            true,
            LiveTriggerTarget {
                surface_instance_id: "surface:phase7".to_owned(),
                surface_attachment_id: "attachment:phase7".to_owned(),
                surface_node_id: "node:complete".to_owned(),
                surface_event: "complete".to_owned(),
                surface_action: "finish".to_owned(),
            },
            phase_seven_trigger_envelope(session_id, 1, rising_edge, stable_for_ms),
        )
        .expect("register Phase 7 trigger");
}

fn phase_seven_observation(
    session_id: &str,
    sequence: u64,
    value: f64,
    stable_age_ms: u64,
) -> LiveControlEnvelope {
    let now = unix_time_millis();
    let mut envelope = phase_six_observation(session_id, sequence + 1, sequence);
    let LiveControlMessage::Observation(observation) = &mut envelope.message else {
        unreachable!();
    };
    observation.observed_at_ms = now;
    observation.stable_since_ms = Some(now.saturating_sub(stable_age_ms));
    observation.value = Some(json!({ "value": value, "minimum": 0, "maximum": 100 }));
    envelope
}

fn phase_seven_close_envelope(session_id: &str, sequence: u64) -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence,
        message: LiveControlMessage::SessionEnd(loom_protocol::LiveSessionEnd {
            reason: loom_protocol::LiveSessionEndReason::Closed,
            ended_by_device_id: "device-source".to_owned(),
            detail: None,
        }),
    }
}

#[test]
fn live_trigger_progress_fires_once_across_more_than_one_hundred_observations() {
    let session_id = "live:phase7-progress";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, false, 0);
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");

    let mut fired = Vec::new();
    for index in 0..=100u64 {
        let outcome = store
            .publish_observation(
                "device-source",
                session_id,
                phase_seven_observation(session_id, index + 1, index as f64, 1_000),
            )
            .expect("publish ordered progress observation");
        fired.extend(outcome.dispatches);
    }
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].audit.observation_sequence, 101);
    assert_eq!(
        fired[0].audit.observation_source,
        LiveObservationSource::UiAutomation
    );
    assert_eq!(fired[0].audit.authorized_by, "device-viewer");

    let repeated = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 102, 100.0, 2_000),
        )
        .expect("publish stable heartbeat");
    assert!(repeated.dispatches.is_empty());
}

#[test]
fn live_trigger_requires_a_real_rising_edge_and_rearms_after_false() {
    let session_id = "live:phase7-rising";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, true, 500);
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");

    assert!(store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 1, 100.0, 1_000),
        )
        .expect("publish initial matching value")
        .dispatches
        .is_empty());
    assert!(store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 2, 50.0, 1_000),
        )
        .expect("publish false edge")
        .dispatches
        .is_empty());
    assert_eq!(
        store
            .publish_observation(
                "device-source",
                session_id,
                phase_seven_observation(session_id, 3, 100.0, 1_000),
            )
            .expect("publish rising edge")
            .dispatches
            .len(),
        1
    );
}

#[test]
fn live_trigger_pauses_offline_and_resumes_without_replaying_an_observation() {
    let session_id = "live:phase7-offline";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, false, 0);

    let paused = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 1, 100.0, 1_000),
        )
        .expect("publish while authorizer is offline");
    assert!(paused.dispatches.is_empty());
    let snapshot = store.get(session_id).expect("read paused trigger");
    assert_eq!(snapshot.trigger_audits.len(), 1);
    assert_eq!(
        snapshot.trigger_audits[0].outcome,
        LiveTriggerOutcome::Skipped
    );
    assert_eq!(
        snapshot.trigger_audits[0].reason.as_deref(),
        Some("authorizer_offline")
    );

    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("reconnect authorizing viewer");
    let resumed = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 2, 100.0, 1_000),
        )
        .expect("publish fresh heartbeat after reconnect");
    assert_eq!(resumed.dispatches.len(), 1);

    let duplicate = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 3, 100.0, 1_000),
        )
        .expect("publish another matching heartbeat");
    assert!(duplicate.dispatches.is_empty());
}

#[test]
fn live_trigger_skips_stale_and_low_confidence_values_with_bounded_audit_noise() {
    let session_id = "live:phase7-untrusted";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, false, 0);
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");

    let mut stale = phase_seven_observation(session_id, 1, 100.0, 1_000);
    let LiveControlMessage::Observation(observation) = &mut stale.message else {
        unreachable!();
    };
    observation.state = LiveObservationState::Stale;
    observation.value = None;
    observation.stable_since_ms = None;
    store
        .publish_observation("device-source", session_id, stale)
        .expect("publish stale state");

    let mut repeated = phase_seven_observation(session_id, 2, 100.0, 1_000);
    let LiveControlMessage::Observation(observation) = &mut repeated.message else {
        unreachable!();
    };
    observation.state = LiveObservationState::Stale;
    observation.value = None;
    observation.stable_since_ms = None;
    store
        .publish_observation("device-source", session_id, repeated)
        .expect("publish repeated stale state");
    assert_eq!(
        store
            .get(session_id)
            .expect("read stale audit")
            .trigger_audits
            .len(),
        1
    );

    let mut low = phase_seven_observation(session_id, 3, 100.0, 1_000);
    let LiveControlMessage::Observation(observation) = &mut low.message else {
        unreachable!();
    };
    observation.confidence = LiveObservationConfidence::High;
    store
        .publish_observation("device-source", session_id, low)
        .expect("publish low-confidence state");
    let audits = store
        .get(session_id)
        .expect("read confidence audit")
        .trigger_audits;
    assert_eq!(audits.len(), 2);
    assert_eq!(
        audits[1].reason.as_deref(),
        Some("observation_confidence_too_low")
    );
}

#[test]
fn live_trigger_reservation_blocks_close_without_consuming_sequence_and_audit_is_upserted() {
    let session_id = "live:phase7-reservation";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, false, 0);
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");
    let mut dispatch = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 1, 100.0, 1_000),
        )
        .expect("reserve trigger dispatch")
        .dispatches
        .pop()
        .expect("matching observation dispatch");

    let reserved = store.get(session_id).expect("read reserved audit");
    assert_eq!(reserved.trigger_audits.len(), 1);
    assert_eq!(
        reserved.trigger_audits[0].reason.as_deref(),
        Some("surface_action_dispatch_reserved")
    );
    let close = phase_seven_close_envelope(session_id, 3);
    let blocked = store
        .close("device-source", close.clone())
        .expect_err("close must wait for reserved action finalization");
    assert_eq!(blocked.code, "live_trigger_dispatch_pending");

    dispatch.audit.reason = Some("surface_action_accepted".to_owned());
    dispatch.audit.action_request_id = Some("request:phase7".to_owned());
    store
        .finalize_trigger_dispatch(session_id, 1, dispatch.audit.clone())
        .expect("finalize trigger audit");
    let finalized = store.get(session_id).expect("read finalized audit");
    assert_eq!(finalized.trigger_audits.len(), 1);
    assert_eq!(finalized.trigger_audits[0], dispatch.audit);
    store
        .close("device-source", close)
        .expect("blocked close did not consume source control sequence");
}

#[test]
fn failed_live_trigger_dispatch_rearms_and_allows_a_fresh_observation() {
    let session_id = "live:phase7-retry";
    let store = phase_six_store(session_id);
    phase_seven_register(&store, session_id, false, 0);
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");
    let mut first = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 1, 100.0, 1_000),
        )
        .expect("reserve first dispatch")
        .dispatches
        .pop()
        .expect("first dispatch");
    first.audit.outcome = LiveTriggerOutcome::Failed;
    first.audit.reason = Some("surface_action_failed".to_owned());
    store
        .finalize_trigger_dispatch(session_id, 1, first.audit)
        .expect("finalize failed dispatch");

    let retry = store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 2, 100.0, 2_000),
        )
        .expect("publish fresh observation after failure");
    assert_eq!(retry.dispatches.len(), 1);
}

#[test]
fn live_trigger_rejects_stability_timestamps_after_the_observation_atomically() {
    let session_id = "live:phase7-stability-time";
    let store = phase_six_store(session_id);
    let mut invalid = phase_seven_observation(session_id, 1, 50.0, 0);
    let LiveControlMessage::Observation(observation) = &mut invalid.message else {
        unreachable!();
    };
    observation.stable_since_ms = Some(observation.observed_at_ms.saturating_add(1));
    let rejected = store
        .publish_observation("device-source", session_id, invalid)
        .expect_err("future stability timestamp must fail closed");
    assert_eq!(rejected.code, "live_observation_stability_invalid");
    store
        .publish_observation(
            "device-source",
            session_id,
            phase_seven_observation(session_id, 1, 50.0, 0),
        )
        .expect("rejected observation did not consume either sequence");
}

#[test]
fn live_trigger_requires_viewer_authority_and_unknown_never_fires() {
    let session_id = "live:phase7-authority";
    let store = phase_six_store(session_id);
    let target = LiveTriggerTarget {
        surface_instance_id: "surface:phase7".to_owned(),
        surface_attachment_id: "attachment:phase7".to_owned(),
        surface_node_id: "node:complete".to_owned(),
        surface_event: "complete".to_owned(),
        surface_action: "finish".to_owned(),
    };
    let denied = store
        .upsert_trigger_binding(
            "device-source",
            "binding:progress-complete",
            true,
            target.clone(),
            phase_seven_trigger_envelope(session_id, 1, false, 0),
        )
        .expect_err("source cannot authorize a remote trigger");
    assert_eq!(denied.code, "live_viewer_denied");
    store
        .upsert_trigger_binding(
            "device-viewer",
            "binding:progress-complete",
            true,
            target,
            phase_seven_trigger_envelope(session_id, 1, false, 0),
        )
        .expect("denied actor did not consume the viewer sequence");
    store
        .set_media_connected(session_id, "device-viewer", LiveDeviceRole::Viewer, true)
        .expect("connect authorizing viewer");

    let mut unknown = phase_seven_observation(session_id, 1, 100.0, 1_000);
    let LiveControlMessage::Observation(observation) = &mut unknown.message else {
        unreachable!();
    };
    observation.state = LiveObservationState::Unknown;
    observation.source = LiveObservationSource::Unknown;
    observation.confidence = LiveObservationConfidence::Low;
    observation.value = None;
    observation.stable_since_ms = None;
    let outcome = store
        .publish_observation("device-source", session_id, unknown)
        .expect("publish unknown observation");
    assert!(outcome.dispatches.is_empty());
    let snapshot = store.get(session_id).expect("read unknown audit");
    assert_eq!(snapshot.trigger_audits.len(), 1);
    assert_eq!(
        snapshot.trigger_audits[0].reason.as_deref(),
        Some("observation_unknown")
    );
}

#[test]
fn phase_eight_visual_observations_cannot_dispatch_high_risk_actions() {
    let mut envelope = phase_seven_observation("live:phase8-visual", 1, 100.0, 1_000);
    let LiveControlMessage::Observation(observation) = &mut envelope.message else {
        unreachable!();
    };
    observation.source = LiveObservationSource::Vision;
    observation.confidence = LiveObservationConfidence::High;
    observation.locator = None;

    let mut action = loom_protocol::SurfaceActionDefinition {
        id: "finish".to_owned(),
        input_schema: json!({}),
        risk: loom_protocol::SurfaceActionRisk::High,
        offline_policy: loom_protocol::SurfaceOfflinePolicy::Reject,
        concurrency: loom_protocol::SurfaceActionConcurrency::Serial,
        idempotent: true,
        confirmation: true,
        cancelable: true,
        timeout_ms: Some(5_000),
        progress: false,
    };
    assert!(validate_live_trigger_action_trust(observation, &action).is_err());

    action.risk = loom_protocol::SurfaceActionRisk::Low;
    assert!(validate_live_trigger_action_trust(observation, &action).is_ok());
}
