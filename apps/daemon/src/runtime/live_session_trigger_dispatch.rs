// Trigger reservation, reliable audit publication, and Surface action dispatch.
fn live_trigger_idempotency_key(
    session_id: &str,
    epoch: u64,
    binding: &LiveTriggerBinding,
    observation_sequence: u64,
) -> String {
    let digest = Sha256::digest(format!(
        "{session_id}\0{epoch}\0{}\0{}\0{observation_sequence}",
        binding.binding_id, binding.condition_revision
    ));
    format!("trigger:{digest:x}")
}

fn remember_trigger_idempotency(record: &mut LiveSessionRecord, key: &str) -> bool {
    if !record.trigger_idempotency_keys.insert(key.to_owned()) {
        return false;
    }
    record.trigger_idempotency_order.push_back(key.to_owned());
    while record.trigger_idempotency_order.len() > LIVE_TRIGGER_IDEMPOTENCY_LIMIT {
        if let Some(expired) = record.trigger_idempotency_order.pop_front() {
            record.trigger_idempotency_keys.remove(&expired);
        }
    }
    true
}

fn forget_trigger_idempotency(record: &mut LiveSessionRecord, key: &str) {
    record.trigger_idempotency_keys.remove(key);
    record
        .trigger_idempotency_order
        .retain(|candidate| candidate != key);
}

fn push_trigger_audit(record: &mut LiveSessionRecord, audit: LiveTriggerAudit) {
    if let Some(existing) = record
        .trigger_audits
        .iter_mut()
        .find(|existing| existing.idempotency_key == audit.idempotency_key)
    {
        *existing = audit.clone();
    } else {
        record.trigger_audits.push_back(audit.clone());
    }
    while record.trigger_audits.len() > LIVE_TRIGGER_AUDIT_LIMIT {
        record.trigger_audits.pop_front();
    }
    push_live_event(
        record,
        LiveControlEnvelope {
            protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.to_owned(),
            session_id: record.session.session_id.clone(),
            epoch: record.epoch,
            sequence: 0,
            message: LiveControlMessage::TriggerEvent(audit),
        },
    );
}

fn dispatch_live_trigger(
    dispatch: LiveTriggerDispatch,
    live_sessions: &SharedLiveSessionStore,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_actions: &SharedSurfaceActionExecutor,
) -> LiveTriggerAudit {
    let mut audit = dispatch.audit;
    if let Err(error) = live_sessions.ensure_trigger_dispatch_pending(
        &dispatch.session_id,
        dispatch.epoch,
        &audit.idempotency_key,
    ) {
        audit.outcome = LiveTriggerOutcome::Failed;
        audit.reason = Some(truncate_live_reason(&error.message));
        audit.evaluated_at_ms = unix_time_millis();
        return audit;
    }
    let event = (|| {
        let store = surface_instances
            .lock()
            .map_err(|_| "Surface store is unavailable".to_owned())?;
        let instance = store
            .get(&dispatch.target.surface_instance_id)
            .ok_or_else(|| "trigger Surface instance was not found".to_owned())?;
        let attachment = instance
            .attachments
            .get(&dispatch.target.surface_attachment_id)
            .ok_or_else(|| "trigger Surface attachment was not found".to_owned())?;
        if attachment.descriptor.device_id != audit.authorized_by {
            return Err("trigger Surface attachment authorization changed".to_owned());
        }
        let snapshot = attachment
            .snapshot
            .as_ref()
            .ok_or_else(|| "trigger Surface attachment has no mounted snapshot".to_owned())?;
        Ok(SurfaceEvent {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: dispatch.target.surface_instance_id.clone(),
            attachment_id: dispatch.target.surface_attachment_id.clone(),
            event_id: audit.idempotency_key.clone(),
            node_id: dispatch.target.surface_node_id.clone(),
            event: dispatch.target.surface_event.clone(),
            action: Some(dispatch.target.surface_action.clone()),
            class: loom_protocol::SurfaceEventClass::Discrete,
            generation: instance.descriptor.generation,
            base_revision: snapshot.revision,
            payload: json!({
                "liveTrigger": {
                    "sessionId": dispatch.session_id,
                    "epoch": dispatch.epoch,
                    "bindingId": audit.binding_id,
                    "conditionRevision": audit.condition_revision,
                    "observationId": dispatch.observation.observation_id,
                    "observationSequence": dispatch.observation.sequence,
                    "sourceDeviceId": audit.source_device_id,
                    "observationSource": audit.observation_source,
                    "value": dispatch.observation.value,
                }
            }),
        })
    })();

    let event = event.and_then(|event| {
        let action = surface_actions
            .action_definition(
                &dispatch.target.surface_instance_id,
                &dispatch.target.surface_action,
            )
            .map_err(|error| error.to_string())?;
        validate_live_trigger_action_trust(&dispatch.observation, &action)?;
        Ok(event)
    });

    match event.and_then(|event| {
        surface_actions
            .submit(&dispatch.target.surface_instance_id, event)
            .map_err(|error| error.to_string())
    }) {
        Ok(ack) => {
            audit.action_request_id = Some(ack.request_id);
            audit.reason = Some(format!(
                "surface_action_{}",
                surface_action_status_name(&ack.status)
            ));
            if !ack.accepted
                || matches!(
                    ack.status,
                    SurfaceActionStatus::Cancelled
                        | SurfaceActionStatus::Failed
                        | SurfaceActionStatus::Interrupted
                        | SurfaceActionStatus::Unknown
                )
            {
                audit.outcome = LiveTriggerOutcome::Failed;
            }
        }
        Err(error) => {
            audit.outcome = LiveTriggerOutcome::Failed;
            audit.reason = Some(truncate_live_reason(&error));
        }
    }
    audit.evaluated_at_ms = unix_time_millis();
    audit
}

fn surface_action_status_name(status: &SurfaceActionStatus) -> &'static str {
    match status {
        SurfaceActionStatus::Accepted => "accepted",
        SurfaceActionStatus::AwaitingConfirmation => "awaiting_confirmation",
        SurfaceActionStatus::Queued => "queued",
        SurfaceActionStatus::Running => "running",
        SurfaceActionStatus::CancelRequested => "cancel_requested",
        SurfaceActionStatus::Cancelled => "cancelled",
        SurfaceActionStatus::Succeeded => "succeeded",
        SurfaceActionStatus::Failed => "failed",
        SurfaceActionStatus::Interrupted => "interrupted",
        SurfaceActionStatus::Unknown => "unknown",
    }
}

fn truncate_live_reason(value: &str) -> String {
    value.chars().take(512).collect()
}

fn validate_live_trigger_action_trust(
    observation: &LiveObservation,
    action: &loom_protocol::SurfaceActionDefinition,
) -> std::result::Result<(), String> {
    if action.risk == loom_protocol::SurfaceActionRisk::High
        && observation.confidence != LiveObservationConfidence::Exact
    {
        return Err("high-risk Surface actions require an exact live observation".to_owned());
    }
    Ok(())
}

fn validate_live_trigger_target(
    target: &LiveTriggerTarget,
) -> std::result::Result<(), LiveRuntimeError> {
    for (value, field) in [
        (&target.surface_instance_id, "surface_instance_id"),
        (&target.surface_attachment_id, "surface_attachment_id"),
        (&target.surface_node_id, "surface_node_id"),
        (&target.surface_event, "surface_event"),
        (&target.surface_action, "surface_action"),
    ] {
        loom_protocol::validate_live_identifier(value, field)
            .map_err(|error| invalid_live_protocol(error.to_string()))?;
    }
    Ok(())
}

fn validate_live_observation_freshness(
    record: &LiveSessionRecord,
    observation: &LiveObservation,
) -> std::result::Result<(), LiveRuntimeError> {
    let now = unix_time_millis();
    if observation.observed_at_ms > now.saturating_add(LIVE_OBSERVATION_MAX_FUTURE_SKEW_MS) {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_future",
            "the live observation timestamp is too far in the future",
        ));
    }
    if now.saturating_sub(observation.observed_at_ms) > LIVE_OBSERVATION_MAX_AGE_MS {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_stale",
            "the live observation timestamp is too old",
        ));
    }
    if record
        .observations
        .get(&observation.observation_id)
        .is_some_and(|previous| previous.observed_at_ms > observation.observed_at_ms)
    {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_time_regressed",
            "the live observation timestamp moved backwards",
        ));
    }
    if observation
        .stable_since_ms
        .is_some_and(|stable_since| stable_since > observation.observed_at_ms)
    {
        return Err(LiveRuntimeError::new(
            409,
            "live_observation_stability_invalid",
            "the live observation stable timestamp exceeds its observation timestamp",
        ));
    }
    Ok(())
}
