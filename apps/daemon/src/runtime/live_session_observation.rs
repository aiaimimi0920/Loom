// Source-authenticated semantic observations and exact per-element ordering.
impl LiveSessionStore {
    fn publish_observation(
        &self,
        actor_device_id: &str,
        session_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveObservationPublishOutcome, LiveRuntimeError> {
        let LiveControlMessage::Observation(observation) = &envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "live observation publishing requires observation",
            ));
        };
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        if record.session.source_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_observation_source_required",
                "only the live source can publish observations",
            ));
        }
        ensure_observation_capability(record, observation)?;
        if !record
            .observations
            .contains_key(&observation.observation_id)
            && record.observations.len() >= LIVE_OBSERVATION_LIMIT
        {
            return Err(LiveRuntimeError::new(
                429,
                "live_observation_limit",
                "the live observation limit has been reached",
            ));
        }
        validate_observation_sequence(record, observation, envelope.epoch)?;
        validate_live_observation_freshness(record, observation)?;
        validate_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;

        record.inbound_sequences.insert(
            actor_device_id.to_owned(),
            (envelope.epoch, envelope.sequence),
        );
        record.inbound_observation_sequences.insert(
            observation.observation_id.clone(),
            (envelope.epoch, observation.sequence),
        );
        record
            .observations
            .insert(observation.observation_id.clone(), observation.clone());
        record.session.last_seen_at_ms = unix_time_millis();
        let observation = observation.clone();
        let mut event = envelope;
        event.sequence = record.next_event_sequence;
        push_live_event(record, event.clone());
        let dispatches = evaluate_live_triggers(record, &observation);
        self.changed.notify_all();
        Ok(LiveObservationPublishOutcome { event, dispatches })
    }
}

fn validate_observation_sequence(
    record: &LiveSessionRecord,
    observation: &LiveObservation,
    epoch: u64,
) -> std::result::Result<(), LiveRuntimeError> {
    if epoch != record.epoch {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_epoch_invalid",
            "the observation epoch does not match the live session",
        ));
    }
    let current = record
        .inbound_observation_sequences
        .get(&observation.observation_id)
        .copied()
        .unwrap_or((epoch, 0));
    let expected = current.1.saturating_add(1);
    if current.0 != epoch || observation.sequence != expected {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_sequence_invalid",
            format!(
                "expected observation sequence {expected}, received {}",
                observation.sequence
            ),
        ));
    }
    Ok(())
}

fn ensure_observation_capability(
    record: &LiveSessionRecord,
    observation: &LiveObservation,
) -> std::result::Result<(), LiveRuntimeError> {
    let required = match observation.source {
        LiveObservationSource::UiAutomation => Some(LiveObservationCapability::UiaTree),
        LiveObservationSource::AppAdapter => Some(LiveObservationCapability::Adapter),
        LiveObservationSource::Vision => Some(LiveObservationCapability::Vision),
        LiveObservationSource::Unknown => None,
    };
    if required.is_some_and(|capability| {
        !record
            .session
            .observation_capabilities
            .contains(&capability)
    }) {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_capability_missing",
            "the live session did not advertise the observation source",
        ));
    }
    Ok(())
}
