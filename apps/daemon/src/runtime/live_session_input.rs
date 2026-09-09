// Reliable remote-input forwarding after Loom authority and ordering checks.
impl LiveSessionStore {
    fn forward_input(
        &self,
        actor_device_id: &str,
        session_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveControlEnvelope, LiveRuntimeError> {
        let LiveControlMessage::InputEvent(input) = &envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "live input forwarding requires input_event",
            ));
        };
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        ensure_viewer(record, actor_device_id)?;
        if record.session.controller_device.as_deref() != Some(actor_device_id) {
            return Err(LiveRuntimeError::new(
                403,
                "live_input_controller_required",
                "only the active controller may send remote input",
            ));
        }
        if input.source_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_input_identity_mismatch",
                "the input source must match the authenticated controller",
            ));
        }
        ensure_input_capability(record, &input.kind)?;
        validate_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        validate_input_sequence(
            record,
            actor_device_id,
            envelope.epoch,
            input.input_sequence,
        )?;

        record.inbound_sequences.insert(
            actor_device_id.to_owned(),
            (envelope.epoch, envelope.sequence),
        );
        record.inbound_input_sequences.insert(
            actor_device_id.to_owned(),
            (envelope.epoch, input.input_sequence),
        );
        let mut forwarded = envelope;
        push_live_event(record, forwarded.clone());
        forwarded.sequence = record.next_event_sequence.saturating_sub(1);
        self.changed.notify_all();
        Ok(forwarded)
    }
}

fn validate_input_sequence(
    record: &LiveSessionRecord,
    actor: &str,
    epoch: u64,
    sequence: u64,
) -> std::result::Result<(), LiveRuntimeError> {
    let current = record
        .inbound_input_sequences
        .get(actor)
        .copied()
        .unwrap_or((epoch, 0));
    let expected = current.1.saturating_add(1);
    if current.0 != epoch || sequence != expected {
        return Err(LiveRuntimeError::new(
            409,
            "live_input_sequence_invalid",
            format!("expected input sequence {expected}, received {sequence}"),
        ));
    }
    Ok(())
}

fn ensure_input_capability(
    record: &LiveSessionRecord,
    input: &loom_protocol::LiveInputKind,
) -> std::result::Result<(), LiveRuntimeError> {
    use loom_protocol::{LiveInputKind, LiveInteractionCapability as Capability};

    let required = match input {
        LiveInputKind::MouseMove(_) => Capability::PointerMove,
        LiveInputKind::MouseButton(_) => Capability::PointerButton,
        LiveInputKind::Wheel(_) => Capability::Wheel,
        LiveInputKind::Key(_) => Capability::Keyboard,
        LiveInputKind::Text(_) => Capability::Text,
        LiveInputKind::Focus(_) => Capability::Focus,
        LiveInputKind::Cancel => Capability::Cancel,
    };
    let supports_double_click = !matches!(
        input,
        LiveInputKind::MouseButton(value) if value.click_count == 2
    ) || record
        .session
        .interaction_capabilities
        .contains(&Capability::DoubleClick);
    if record.session.interaction_capabilities.contains(&required) && supports_double_click {
        Ok(())
    } else {
        Err(LiveRuntimeError::new(
            409,
            "live_input_capability_unavailable",
            format!("the source did not advertise the required {required:?} capability"),
        ))
    }
}
