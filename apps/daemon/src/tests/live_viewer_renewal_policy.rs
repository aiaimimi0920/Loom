// Renewal is a conditional reattachment, not permission to add a missing viewer or keep control.
#[test]
fn viewer_renewal_policy_rechecks_membership_and_closure_after_cursor_read() {
    for closed in [false, true] {
        let id = "live:renewal-race";
        let actor = "device-viewer";
        let store = phase_six_store(id);
        let before = store.get_for_member(id, actor).unwrap();
        let mut ack = live_viewer_envelope(id, actor);
        ack.sequence = before.requester_control.unwrap().control_sequence + 1;
        {
            let mut sessions = store.lock_state().unwrap();
            let record = sessions.get_mut(id).unwrap();
            if closed {
                record.closed = true;
            } else {
                record
                    .session
                    .viewer_devices
                    .retain(|member| member != actor);
            }
        }
        assert!(store
            .attach_viewer_with_policy(actor, ack.clone(), true)
            .is_err());
        let sessions = store.lock_state().unwrap();
        let record = &sessions[id];
        assert_eq!(record.session.revision, before.session.revision);
        if !closed {
            assert!(!record
                .session
                .viewer_devices
                .iter()
                .any(|member| member == actor));
        }
        assert_eq!(
            record.inbound_sequences[actor],
            (ack.epoch, ack.sequence - 1)
        );
    }
}

#[test]
fn viewer_renewal_policy_rejects_missing_member_without_mutating_cursors() {
    let id = "live:renewal-policy";
    let store = phase_six_store(id);
    let actor = "device:new-viewer";
    let before = store.get(id).unwrap();
    assert_eq!(
        store
            .attach_viewer_with_policy(actor, live_viewer_envelope(id, actor), true)
            .unwrap_err()
            .code,
        "live_viewer_renewal_unavailable"
    );
    let after = store.get(id).unwrap();
    assert_eq!(after.session.revision, before.session.revision);
    assert_eq!(after.session.viewer_devices, before.session.viewer_devices);
    assert!(!store.lock_state().unwrap()[id]
        .inbound_sequences
        .contains_key(actor));
    // Explicit first joins retain their original contract.
    store
        .attach_viewer(actor, live_viewer_envelope(id, actor))
        .unwrap();
}

#[test]
fn viewer_renewal_policy_rejects_own_controller_without_consuming_sequence() {
    let id = "live:renewal-control";
    let store = phase_five_store(id);
    let actor = "device-viewer-a";
    store
        .change_controller(
            actor,
            id,
            &phase_five_lease(LiveControlLeaseAction::Acquire, 2),
        )
        .unwrap();
    let before = store.get(id).unwrap();
    let mut ack = live_viewer_envelope(id, actor);
    ack.sequence = 3;
    assert_eq!(
        store
            .attach_viewer_with_policy(actor, ack, true)
            .unwrap_err()
            .code,
        "live_viewer_renewal_unavailable"
    );
    assert_eq!(
        store.get(id).unwrap().session.revision,
        before.session.revision
    );
    assert_eq!(
        store
            .get_for_member(id, actor)
            .unwrap()
            .requester_control
            .unwrap()
            .control_sequence,
        2
    );
    store
        .change_controller(
            actor,
            id,
            &phase_five_lease(LiveControlLeaseAction::Release, 3),
        )
        .unwrap();
    let mut ack = live_viewer_envelope(id, actor);
    ack.sequence = 4;
    let renewed = store.attach_viewer_with_policy(actor, ack, true).unwrap();
    assert!(renewed.session.controller_device.is_none());
    assert_eq!(
        renewed.session.viewer_devices,
        before.session.viewer_devices
    );
}

#[test]
fn viewer_renewal_policy_retains_epoch_validation() {
    let id = "live:renewal-epoch";
    let store = phase_six_store(id);
    let mut ack = live_viewer_envelope(id, "device-viewer");
    ack.sequence = 2;
    ack.epoch += 1;
    let before = store.get(id).unwrap();
    assert!(store
        .attach_viewer_with_policy("device-viewer", ack, true)
        .is_err());
    assert_eq!(
        store.get(id).unwrap().session.revision,
        before.session.revision
    );
}

#[test]
fn viewer_renewal_policy_request_is_optional_and_strictly_typed() {
    let mut body = serde_json::json!({"surfaceInstanceId":"surface:test", "attachmentId":"attachment:test",
        "envelope":live_viewer_envelope("live:test", "device-viewer")});
    let request: LiveViewerAttachRequest = serde_json::from_value(body.clone()).unwrap();
    assert!(!request.require_existing_membership);
    body["requireExistingMembership"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<LiveViewerAttachRequest>(body.clone())
            .unwrap()
            .require_existing_membership
    );
    body["requireExistingMembership"] = serde_json::json!("true");
    assert!(serde_json::from_value::<LiveViewerAttachRequest>(body).is_err());
}
