// Ordering, membership, snapshot, and lifecycle helpers for live session state.
fn accept_control_sequence(
    record: &mut LiveSessionRecord,
    actor: &str,
    epoch: u64,
    sequence: u64,
) -> std::result::Result<(), LiveRuntimeError> {
    validate_control_sequence(record, actor, epoch, sequence)?;
    record
        .inbound_sequences
        .insert(actor.to_owned(), (epoch, sequence));
    Ok(())
}

fn validate_control_sequence(
    record: &LiveSessionRecord,
    actor: &str,
    epoch: u64,
    sequence: u64,
) -> std::result::Result<(), LiveRuntimeError> {
    if epoch != record.epoch {
        return Err(LiveRuntimeError::new(
            409,
            "live_control_epoch_invalid",
            "the control epoch does not match the live session",
        ));
    }
    let current = record
        .inbound_sequences
        .get(actor)
        .copied()
        .unwrap_or((epoch, 0));
    let expected = current.1.saturating_add(1);
    if current.0 != epoch || sequence != expected {
        return Err(LiveRuntimeError::new(
            409,
            "live_control_sequence_invalid",
            format!("expected control sequence {expected}, received {sequence}"),
        ));
    }
    Ok(())
}

fn snapshot(record: &LiveSessionRecord) -> LiveSessionRuntimeSnapshot {
    LiveSessionRuntimeSnapshot {
        session: record.session.clone(),
        epoch: record.epoch,
        last_frame_id: record.last_frame_id,
        published_frames: record.published_frames,
        relay_dropped_frames: record.relay_dropped_frames,
        buffered_frames: record.frames.len(),
        source_connected: record.source_connected,
        viewer_connections: record.viewer_connections.clone(),
        controller_expires_at_ms: record.controller_expires_at_ms,
        observations: record.observations.values().cloned().collect(),
        triggers: record
            .trigger_registrations
            .values()
            .map(LiveTriggerRegistrationSnapshot::from)
            .collect(),
        trigger_audits: record.trigger_audits.iter().cloned().collect(),
        closed: record.closed,
    }
}

fn active_record_mut<'a>(
    sessions: &'a mut BTreeMap<String, LiveSessionRecord>,
    session_id: &str,
) -> std::result::Result<&'a mut LiveSessionRecord, LiveRuntimeError> {
    let record = sessions
        .get_mut(session_id)
        .ok_or_else(|| not_found(session_id))?;
    if record.closed {
        return Err(not_found(session_id));
    }
    expire_controller(record);
    Ok(record)
}

fn ensure_member(
    record: &LiveSessionRecord,
    device_id: &str,
) -> std::result::Result<(), LiveRuntimeError> {
    if record.session.source_device_id == device_id
        || record
            .session
            .viewer_devices
            .iter()
            .any(|id| id == device_id)
    {
        Ok(())
    } else {
        Err(LiveRuntimeError::new(
            403,
            "live_session_denied",
            "the device is not attached to the live session",
        ))
    }
}

fn ensure_viewer(
    record: &LiveSessionRecord,
    device_id: &str,
) -> std::result::Result<(), LiveRuntimeError> {
    if record
        .session
        .viewer_devices
        .iter()
        .any(|id| id == device_id)
    {
        Ok(())
    } else {
        Err(LiveRuntimeError::new(
            403,
            "live_viewer_denied",
            "the device is not an attached viewer",
        ))
    }
}

fn expire_controller(record: &mut LiveSessionRecord) {
    if record
        .controller_expires_at_ms
        .is_some_and(|expires| expires <= unix_time_millis())
    {
        record.session.controller_device = None;
        record.controller_expires_at_ms = None;
        record.session.revision = record.session.revision.saturating_add(1);
        push_state_event(record, "controller_expired");
    }
}

fn has_newer_frame(record: &LiveSessionRecord, epoch: u64, frame_id: u64) -> bool {
    record.frames.back().is_some_and(|frame| {
        frame.epoch > epoch || (frame.epoch == epoch && frame.frame_id > frame_id)
    })
}

fn not_found(session_id: &str) -> LiveRuntimeError {
    LiveRuntimeError::new(
        404,
        "live_session_not_found",
        format!("live session `{session_id}` was not found"),
    )
}

fn unavailable() -> LiveRuntimeError {
    LiveRuntimeError::new(
        503,
        "live_runtime_unavailable",
        "the live session runtime is unavailable",
    )
}
