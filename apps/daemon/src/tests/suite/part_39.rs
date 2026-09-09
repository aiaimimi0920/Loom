// Phase 5 live input authority, ordering, capability, and revocation contracts.
fn phase_five_store(session_id: &str) -> LiveSessionStore {
    let store = LiveSessionStore::new();
    let mut start = live_start_envelope(session_id, &format!("nonce:{session_id}"));
    let LiveControlMessage::SessionStart(message) = &mut start.message else {
        panic!("fixture must contain session_start");
    };
    message.session.interaction_capabilities = vec![
        loom_protocol::LiveInteractionCapability::PointerMove,
        loom_protocol::LiveInteractionCapability::PointerButton,
        loom_protocol::LiveInteractionCapability::Wheel,
        loom_protocol::LiveInteractionCapability::Keyboard,
        loom_protocol::LiveInteractionCapability::Cancel,
    ];
    store
        .create("device-source", start)
        .expect("create Phase 5 session");
    for viewer in ["device-viewer-a", "device-viewer-b"] {
        store
            .attach_viewer(viewer, live_viewer_envelope(session_id, viewer))
            .expect("attach Phase 5 viewer");
    }
    store
}

fn phase_five_lease(action: LiveControlLeaseAction, sequence: u64) -> LiveControlLeaseRequest {
    LiveControlLeaseRequest {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        surface_instance_id: "surface:fixture".to_owned(),
        attachment_id: "attachment:fixture".to_owned(),
        action,
        sequence,
        epoch: 1,
        lease_duration_ms: Some(30_000),
    }
}

fn phase_five_input(
    session_id: &str,
    actor: &str,
    control_sequence: u64,
    input_sequence: u64,
) -> LiveControlEnvelope {
    LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: session_id.to_owned(),
        epoch: 1,
        sequence: control_sequence,
        message: LiveControlMessage::InputEvent(loom_protocol::LiveInputEvent {
            input_sequence,
            issued_at_ms: unix_time_millis(),
            source_device_id: actor.to_owned(),
            kind: loom_protocol::LiveInputKind::MouseButton(loom_protocol::LiveMouseButtonInput {
                button: loom_protocol::LiveMouseButton::Left,
                state: loom_protocol::LiveButtonState::Pressed,
                x: 0.5,
                y: 0.5,
                click_count: 1,
            }),
        }),
    }
}

#[test]
fn live_input_requires_controller_capability_and_exact_atomic_sequences() {
    let session_id = "live:phase5-input";
    let store = phase_five_store(session_id);
    let denied = store
        .forward_input(
            "device-viewer-b",
            session_id,
            phase_five_input(session_id, "device-viewer-b", 2, 1),
        )
        .expect_err("non-controller input must fail closed");
    assert_eq!(denied.code, "live_input_controller_required");

    store
        .change_controller(
            "device-viewer-a",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .expect("acquire controller");
    let accepted = store
        .forward_input(
            "device-viewer-a",
            session_id,
            phase_five_input(session_id, "device-viewer-a", 3, 1),
        )
        .expect("forward first input");
    assert!(matches!(
        accepted.message,
        LiveControlMessage::InputEvent(_)
    ));

    let duplicate = store
        .forward_input(
            "device-viewer-a",
            session_id,
            phase_five_input(session_id, "device-viewer-a", 4, 1),
        )
        .expect_err("duplicate input sequence must fail");
    assert_eq!(duplicate.code, "live_input_sequence_invalid");
    store
        .forward_input(
            "device-viewer-a",
            session_id,
            phase_five_input(session_id, "device-viewer-a", 4, 2),
        )
        .expect("failed input must not consume control sequence");
}

#[test]
fn live_input_is_reliably_visible_only_to_session_members() {
    let session_id = "live:phase5-events";
    let store = phase_five_store(session_id);
    store
        .change_controller(
            "device-viewer-a",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .expect("acquire controller");
    store
        .forward_input(
            "device-viewer-a",
            session_id,
            phase_five_input(session_id, "device-viewer-a", 3, 1),
        )
        .expect("forward input");
    let (_, events) = store
        .wait_events_after(session_id, "device-source", 0, Duration::ZERO)
        .expect("source reads reliable events");
    assert!(events
        .iter()
        .any(|event| matches!(event.message, LiveControlMessage::InputEvent(_))));
    let denied = store
        .wait_events_after(session_id, "device-outsider", 0, Duration::ZERO)
        .expect_err("outsider must not read input events");
    assert_eq!(denied.code, "live_session_denied");
}

#[test]
fn live_controller_is_revoked_by_source_and_viewer_disconnect() {
    let session_id = "live:phase5-revoke";
    let store = phase_five_store(session_id);
    store
        .change_controller(
            "device-viewer-a",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .expect("acquire controller");
    store
        .change_controller(
            "device-source",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Revoke, 2),
        )
        .expect("source reclaims control");
    assert!(store
        .get(session_id)
        .expect("snapshot after reclaim")
        .session
        .controller_device
        .is_none());

    store
        .change_controller(
            "device-viewer-a",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 3),
        )
        .expect("reacquire controller");
    store
        .set_media_connected(session_id, "device-viewer-a", LiveDeviceRole::Viewer, true)
        .expect("connect viewer media");
    store
        .set_media_connected(session_id, "device-viewer-a", LiveDeviceRole::Viewer, false)
        .expect("disconnect viewer media");
    assert!(store
        .get(session_id)
        .expect("snapshot after disconnect")
        .session
        .controller_device
        .is_none());
}

#[test]
fn live_controller_lease_expiry_revokes_authority_and_emits_an_event() {
    let session_id = "live:phase5-expiry";
    let store = phase_five_store(session_id);
    store
        .change_controller(
            "device-viewer-a",
            session_id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .expect("acquire controller");
    store
        .state
        .lock()
        .expect("lock Phase 5 sessions")
        .get_mut(session_id)
        .expect("Phase 5 session")
        .controller_expires_at_ms = Some(0);

    let (_, events) = store
        .wait_events_after(session_id, "device-source", 0, Duration::ZERO)
        .expect("expire controller while reading events");
    assert!(store
        .get(session_id)
        .expect("snapshot after expiry")
        .session
        .controller_device
        .is_none());
    assert!(events.iter().any(|event| matches!(
        &event.message,
        LiveControlMessage::SessionState(state)
            if state.reason.as_deref() == Some("controller_expired")
    )));
}
