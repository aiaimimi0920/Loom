// Bounded reliable control history and authenticated long-poll delivery.
impl LiveSessionStore {
    #[cfg(test)]
    fn events_after(
        &self,
        session_id: &str,
        after: u64,
    ) -> std::result::Result<(bool, Vec<LiveControlEnvelope>), LiveRuntimeError> {
        let sessions = self.lock_state()?;
        let record = sessions
            .get(session_id)
            .ok_or_else(|| not_found(session_id))?;
        Ok(collect_events_after(record, after))
    }

    fn wait_events_after(
        &self,
        session_id: &str,
        actor_device_id: &str,
        after: u64,
        timeout: Duration,
    ) -> std::result::Result<(bool, Vec<LiveControlEnvelope>), LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        {
            let record = active_record_mut(&mut sessions, session_id)?;
            ensure_member(record, actor_device_id)?;
            if has_newer_event(record, after) || timeout.is_zero() {
                return Ok(collect_events_after(record, after));
            }
        }
        let (mut sessions, _) = self
            .changed
            .wait_timeout_while(sessions, timeout, |sessions| {
                let Some(record) = sessions.get_mut(session_id) else {
                    return false;
                };
                expire_controller(record);
                !record.closed && !has_newer_event(record, after)
            })
            .map_err(|_| unavailable())?;
        let record = active_record_mut(&mut sessions, session_id)?;
        ensure_member(record, actor_device_id)?;
        Ok(collect_events_after(record, after))
    }
}

fn push_state_event(record: &mut LiveSessionRecord, reason: &str) {
    let event = LiveControlEnvelope {
        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
        session_id: record.session.session_id.clone(),
        epoch: record.epoch,
        sequence: 0,
        message: LiveControlMessage::SessionState(LiveSessionState {
            revision: record.session.revision,
            visibility: record.session.visibility_state,
            viewers: record.session.viewer_devices.clone(),
            controller_device_id: record.session.controller_device.clone(),
            reason: Some(reason.to_owned()),
        }),
    };
    push_live_event(record, event);
}

fn push_live_event(record: &mut LiveSessionRecord, mut event: LiveControlEnvelope) {
    event.sequence = record.next_event_sequence;
    record.next_event_sequence = record.next_event_sequence.saturating_add(1);
    record.events.push_back(event);
    while record.events.len() > LIVE_CONTROL_HISTORY_LIMIT {
        record.events.pop_front();
    }
}

fn collect_events_after(
    record: &LiveSessionRecord,
    after: u64,
) -> (bool, Vec<LiveControlEnvelope>) {
    let oldest = record
        .events
        .front()
        .map(|event| event.sequence)
        .unwrap_or(after);
    let reset = after.saturating_add(1) < oldest;
    let events = record
        .events
        .iter()
        .filter(|event| reset || event.sequence > after)
        .cloned()
        .collect();
    (reset, events)
}

fn has_newer_event(record: &LiveSessionRecord, after: u64) -> bool {
    record
        .events
        .back()
        .is_some_and(|event| event.sequence > after)
}
