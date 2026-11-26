// Synthetic logical clock verifies only that original grant lifetimes cannot be extended, without altering product time, network timeouts, or user clocks.
#[test]
fn local_art_trigger_lifecycle_preserves_deadlines_and_reclaims_expired_payload() {
    for grant in [false, true] {
        let t = TriggerFixture::new();
        t.ready();
        let original = {
            let state = t.f.live_sessions.lock_state().unwrap();
            let entry = state[ART_TRIGGER_SESSION]
                .local_art_triggers
                .0
                .get("local-art:test")
                .unwrap();
            (entry.grant_deadline, entry.delivery_deadline)
        };
        for _ in 0..3 {
            assert_eq!(t.inspect()["status"], "ready");
        }
        {
            let mut state = t.f.live_sessions.lock_state().unwrap();
            let entry = state
                .get_mut(ART_TRIGGER_SESSION)
                .unwrap()
                .local_art_triggers
                .0
                .get_mut("local-art:test")
                .unwrap();
            assert_eq!((entry.grant_deadline, entry.delivery_deadline), original);
            if grant {
                entry.grant_deadline = Instant::now();
            } else {
                entry.delivery_deadline = Some(Instant::now());
            }
        }
        assert_eq!(t.send(t.op("claim")).0, 409);
        let state = t.inspect();
        assert_eq!(state["status"], "expired");
        assert_eq!(
            state["reason"],
            if grant {
                "grant_expired"
            } else {
                "delivery_expired"
            }
        );
        let guard = t.f.live_sessions.lock_state().unwrap();
        let entry = guard[ART_TRIGGER_SESSION]
            .local_art_triggers
            .0
            .get("local-art:test")
            .unwrap();
        assert!(entry.payload.is_none() && entry.delivery_deadline.is_none());
        assert_eq!(
            entry.reply.delivery_id.as_deref(),
            state["deliveryId"].as_str()
        );
    }
}
#[test]
fn local_art_trigger_lifecycle_disconnect_stop_and_close_reject_late_ack() {
    let t = TriggerFixture::new();
    t.ready();
    let id = t.send(t.op("claim")).1["deliveryId"].clone();
    t.f.live_sessions
        .set_media_connected(
            ART_TRIGGER_SESSION,
            &t.f.device,
            LiveDeviceRole::Viewer,
            false,
        )
        .unwrap();
    assert_eq!(t.inspect()["reason"], "viewer_disconnected");
    t.f.live_sessions
        .set_media_connected(
            ART_TRIGGER_SESSION,
            &t.f.device,
            LiveDeviceRole::Viewer,
            true,
        )
        .unwrap();
    assert_eq!(t.send(t.op("claim")).0, 409);
    assert_eq!(t.ack(&id, "no_result").0, 409);
    assert_eq!(t.send(t.op("stop")).0, 200);
    assert_eq!(t.ack(&id, "no_result").0, 409);
    let mut close = phase_seven_close_envelope(ART_TRIGGER_SESSION, 4);
    if let LiveControlMessage::SessionEnd(end) = &mut close.message {
        end.ended_by_device_id = t.source.clone();
    }
    t.f.live_sessions.close(&t.source, close).unwrap();
    assert_eq!(t.send(t.op("inspect")).0, 404);
    let state = t.f.live_sessions.lock_state().unwrap();
    assert!(state[ART_TRIGGER_SESSION].local_art_triggers.0.is_empty());
}
#[test]
fn local_art_trigger_lifecycle_uses_original_source_freshness_confidence_and_stability() {
    let t = TriggerFixture::new();
    t.configure();
    t.observe(1, 0.);
    let low = t.publish(t.observe_value(2, 100., LiveObservationConfidence::Low));
    assert_eq!(low.0, 202, "{low:?}");
    assert_eq!(t.inspect()["status"], "idle");
    let mut stale = t.observe_value(3, 100., LiveObservationConfidence::Exact);
    stale["payload"]["observedAtMs"] = json!(unix_time_millis() - 31_000);
    stale["payload"]["stableSinceMs"] = json!(unix_time_millis() - 32_000);
    assert_eq!(t.publish(stale).0, 409);
    let mut unstable = t.observe_value(3, 100., LiveObservationConfidence::Exact);
    unstable["payload"]["state"] = json!("observing");
    assert_eq!(t.publish(unstable).0, 202);
    assert_eq!(t.inspect()["status"], "idle");
    let source_only = device(
        t.f.port,
        &t.f.token,
        "POST",
        &format!("/v1/live/sessions/{ART_TRIGGER_SESSION}/observations"),
        Some(json!({"envelope":t.observe_value(4, 100., LiveObservationConfidence::Exact)})),
    );
    assert_eq!(source_only.0, 403);
    t.observe(4, 100.);
    assert_eq!(t.inspect()["status"], "ready");
}
#[test]
fn local_art_trigger_lifecycle_overbudget_observation_is_not_truncated_or_queued() {
    let t = TriggerFixture::new();
    t.configure();
    t.observe(1, 0.);
    let mut large = t.observe_value(2, 100., LiveObservationConfidence::Exact);
    large["payload"]["value"]["padding"] = json!("x".repeat(20 * 1024));
    let accepted = t.publish(large);
    assert_eq!(
        accepted.0, 202,
        "original observation budget remains unchanged: {accepted:?}"
    );
    let reply = t.inspect();
    assert_eq!(reply["status"], "stopped");
    assert_eq!(reply["reason"], "delivery_limit");
    assert_eq!(reply["deliveryId"], Value::Null);
    assert_eq!(t.send(t.op("claim")).0, 409);
    t.observe(3, 0.);
    t.observe(4, 100.);
    assert_eq!(t.inspect()["status"], "stopped");
}
#[test]
fn local_art_trigger_lifecycle_shares_original_binding_limit_and_control_sequence() {
    let mut t = TriggerFixture::new();
    let first = t.configuration(1, 2);
    let mut invalid = first.clone();
    invalid["configuration"]["sequence"] = json!(9);
    assert_eq!(t.send(invalid).0, 409);
    assert_eq!(t.send(t.op("inspect")).0, 404);
    let binding = "binding:surface-budget";
    t.f.live_sessions
        .upsert_trigger_binding(
            &t.f.device,
            binding,
            true,
            LiveTriggerTarget {
                surface_instance_id: "surface:budget".into(),
                surface_attachment_id: "attachment:budget".into(),
                surface_node_id: "node:budget".into(),
                surface_event: "click".into(),
                surface_action: "action:budget".into(),
            },
            phase_seven_trigger_envelope(ART_TRIGGER_SESSION, 1, false, 0),
        )
        .unwrap();
    for index in 0..63 {
        t.scope["bindingId"] = json!(format!("local-art:budget:{index}"));
        let (status, reply) = t.send(t.configuration(1, index + 3));
        assert_eq!(status, 200, "{index}: {reply}");
    }
    t.scope["bindingId"] = json!("local-art:overflow");
    assert_eq!(t.send(t.configuration(1, 66)).0, 429);
    let state = t.f.live_sessions.lock_state().unwrap();
    let record = &state[ART_TRIGGER_SESSION];
    assert_eq!(
        record.local_art_triggers.len() + record.trigger_registrations.len(),
        64
    );
    assert_eq!(
        record.inbound_sequences[&t.f.device].1, 65,
        "failed admission must not consume sequence"
    );
}
#[test]
fn local_art_trigger_lifecycle_clock_rollback_fails_closed_without_a_system_clock_change() {
    let t = TriggerFixture::new();
    t.ready();
    {
        let mut state = t.f.live_sessions.lock_state().unwrap();
        let entry = state
            .get_mut(ART_TRIGGER_SESSION)
            .unwrap()
            .local_art_triggers
            .0
            .get_mut("local-art:test")
            .unwrap();
        entry.observed_wall = unix_time_millis() + 10_000;
    }
    assert_eq!(t.send(t.op("claim")).0, 409);
    assert_eq!(t.inspect()["reason"], "clock_regressed");
}
#[test]
fn local_art_trigger_lifecycle_wall_jump_expires_ready_and_claimed_before_monotonic_deadline() {
    for claimed in [false, true] {
        let t = TriggerFixture::new();
        t.ready();
        if claimed {
            assert_eq!(t.send(t.op("claim")).0, 200);
        }
        let id = {
            let mut state = t.f.live_sessions.lock_state().unwrap();
            let entry = state
                .get_mut(ART_TRIGGER_SESSION)
                .unwrap()
                .local_art_triggers
                .0
                .get_mut("local-art:test")
                .unwrap();
            let clock = Instant::now();
            let expiry = entry.delivery_expires_at_ms.unwrap();
            assert!(clock < entry.delivery_deadline.unwrap());
            assert!(expiry < entry.expires_at_ms);
            entry.expire(expiry, clock);
            assert_eq!(entry.reply.status, local_trigger::Status::Expired);
            assert_eq!(
                entry.reply.reason,
                Some(local_trigger::StopReason::DeliveryExpired)
            );
            assert!(!entry.reply.enabled && entry.payload.is_none());
            assert!(entry.delivery_deadline.is_none() && entry.delivery_expires_at_ms.is_none());
            entry.reply.delivery_id.clone().unwrap()
        };
        assert_eq!(t.ack(&json!(id), "no_result").0, 409);
        assert_eq!(t.send(t.op("claim")).0, 409);
    }
}
