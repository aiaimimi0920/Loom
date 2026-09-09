// Deterministic live-condition evaluation and Surface action dispatch.
const LIVE_OBSERVATION_MAX_AGE_MS: u64 = 30_000;
const LIVE_OBSERVATION_MAX_FUTURE_SKEW_MS: u64 = 5_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveTriggerTarget {
    surface_instance_id: String,
    surface_attachment_id: String,
    surface_node_id: String,
    surface_event: String,
    surface_action: String,
}

#[derive(Clone, Debug, Default)]
struct LiveTriggerConditionState {
    initialized: bool,
    last_match: bool,
    armed: bool,
    pending_offline_match: bool,
    last_skip_reason: Option<String>,
}

#[derive(Clone, Debug)]
struct LiveTriggerRegistration {
    binding: LiveTriggerBinding,
    condition: LiveTriggerCondition,
    target: LiveTriggerTarget,
    state: LiveTriggerConditionState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveTriggerRegistrationSnapshot {
    binding: LiveTriggerBinding,
    condition: LiveTriggerCondition,
    target: LiveTriggerTarget,
    armed: bool,
    last_match: bool,
    pending_offline_match: bool,
}

impl From<&LiveTriggerRegistration> for LiveTriggerRegistrationSnapshot {
    fn from(value: &LiveTriggerRegistration) -> Self {
        Self {
            binding: value.binding.clone(),
            condition: value.condition.clone(),
            target: value.target.clone(),
            armed: value.state.armed,
            last_match: value.state.last_match,
            pending_offline_match: value.state.pending_offline_match,
        }
    }
}

#[derive(Debug)]
struct LiveObservationPublishOutcome {
    event: LiveControlEnvelope,
    dispatches: Vec<LiveTriggerDispatch>,
}

#[derive(Clone, Debug)]
struct LiveTriggerDispatch {
    session_id: String,
    epoch: u64,
    audit: LiveTriggerAudit,
    target: LiveTriggerTarget,
    observation: LiveObservation,
}

impl LiveSessionStore {
    fn upsert_trigger_binding(
        &self,
        actor_device_id: &str,
        binding_id: &str,
        enabled: bool,
        target: LiveTriggerTarget,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        loom_protocol::validate_live_identifier(binding_id, "binding_id")
            .map_err(|error| invalid_live_protocol(error.to_string()))?;
        validate_live_trigger_target(&target)?;
        let LiveControlMessage::TriggerCondition(condition) = &envelope.message else {
            return Err(invalid_live_protocol(
                "live trigger registration requires trigger_condition",
            ));
        };
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, &envelope.session_id)?;
        ensure_viewer(record, actor_device_id)?;
        validate_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        if let Some(existing) = record.trigger_registrations.get(binding_id) {
            if condition.revision <= existing.condition.revision {
                return Err(LiveRuntimeError::new(
                    409,
                    "live_trigger_revision_invalid",
                    format!(
                        "trigger condition revision must exceed {}",
                        existing.condition.revision
                    ),
                ));
            }
        } else if record.trigger_registrations.len() >= loom_protocol::LIVE_MAX_TRIGGER_BINDINGS {
            return Err(LiveRuntimeError::new(
                429,
                "live_trigger_binding_limit",
                "the live trigger binding limit has been reached",
            ));
        }

        accept_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        let binding = LiveTriggerBinding {
            binding_id: binding_id.to_owned(),
            observation_id: condition.observation_id.clone(),
            condition_revision: condition.revision,
            authorized_by: actor_device_id.to_owned(),
            enabled,
        };
        let registration = LiveTriggerRegistration {
            binding: binding.clone(),
            condition: condition.clone(),
            target,
            state: LiveTriggerConditionState {
                armed: true,
                ..LiveTriggerConditionState::default()
            },
        };
        record
            .trigger_registrations
            .insert(binding_id.to_owned(), registration);
        if let Some(existing) = record
            .session
            .trigger_bindings
            .iter_mut()
            .find(|candidate| candidate.binding_id == binding_id)
        {
            *existing = binding;
        } else {
            record.session.trigger_bindings.push(binding);
        }
        record
            .session
            .trigger_bindings
            .sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
        record.session.revision = record.session.revision.saturating_add(1);
        record.session.last_seen_at_ms = unix_time_millis();
        push_live_event(record, envelope);
        push_state_event(record, "trigger_binding_updated");
        let result = snapshot(record);
        self.changed.notify_all();
        Ok(result)
    }

    fn ensure_trigger_dispatch_pending(
        &self,
        session_id: &str,
        epoch: u64,
        idempotency_key: &str,
    ) -> std::result::Result<(), LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        if record.epoch != epoch || !record.pending_trigger_dispatches.contains(idempotency_key) {
            return Err(LiveRuntimeError::new(
                409,
                "live_trigger_dispatch_stale",
                "the live trigger dispatch is no longer reserved",
            ));
        }
        Ok(())
    }

    fn finalize_trigger_dispatch(
        &self,
        session_id: &str,
        epoch: u64,
        audit: LiveTriggerAudit,
    ) -> std::result::Result<(), LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        if record.epoch != epoch
            || !record
                .pending_trigger_dispatches
                .remove(&audit.idempotency_key)
        {
            return Err(LiveRuntimeError::new(
                409,
                "live_trigger_dispatch_stale",
                "the live trigger dispatch finalization is no longer reserved",
            ));
        }
        if audit.outcome == LiveTriggerOutcome::Failed {
            forget_trigger_idempotency(record, &audit.idempotency_key);
            if let Some(registration) = record.trigger_registrations.get_mut(&audit.binding_id) {
                if registration.condition.rearm {
                    registration.state.armed = true;
                    registration.state.last_match = false;
                }
            }
        }
        push_trigger_audit(record, audit);
        self.changed.notify_all();
        Ok(())
    }
}

fn evaluate_live_triggers(
    record: &mut LiveSessionRecord,
    observation: &LiveObservation,
) -> Vec<LiveTriggerDispatch> {
    let source_device_id = record.session.source_device_id.clone();
    let session_id = record.session.session_id.clone();
    let epoch = record.epoch;
    let viewer_connections = record.viewer_connections.clone();
    let mut evaluated = Vec::new();

    for registration in record.trigger_registrations.values_mut() {
        if !registration.binding.enabled
            || registration.condition.observation_id != observation.observation_id
        {
            continue;
        }
        let reason = trigger_safety_skip_reason(&registration.condition, observation);
        let matches =
            reason.is_none() && trigger_condition_matches(&registration.condition, observation);
        let state = &mut registration.state;
        if let Some(reason) = reason {
            if registration.condition.rearm {
                state.armed = true;
            }
            state.initialized = true;
            state.last_match = false;
            state.pending_offline_match = false;
            if state.last_skip_reason.as_deref() != Some(reason) {
                state.last_skip_reason = Some(reason.to_owned());
                evaluated.push((registration.clone(), Some(reason.to_owned()), false));
            }
            continue;
        }
        if !matches {
            if registration.condition.rearm {
                state.armed = true;
            }
            state.initialized = true;
            state.last_match = false;
            state.pending_offline_match = false;
            state.last_skip_reason = None;
            continue;
        }
        if registration.condition.rising_edge && !state.initialized {
            state.initialized = true;
            state.last_match = true;
            let reason = "rising_edge_not_observed";
            if state.last_skip_reason.as_deref() != Some(reason) {
                state.last_skip_reason = Some(reason.to_owned());
                evaluated.push((registration.clone(), Some(reason.to_owned()), false));
            }
            continue;
        }
        let authorizer_online = viewer_connections
            .get(&registration.binding.authorized_by)
            .copied()
            .unwrap_or_default()
            > 0;
        if !authorizer_online {
            state.initialized = true;
            state.last_match = false;
            state.pending_offline_match = true;
            let reason = "authorizer_offline";
            if state.last_skip_reason.as_deref() != Some(reason) {
                state.last_skip_reason = Some(reason.to_owned());
                evaluated.push((registration.clone(), Some(reason.to_owned()), false));
            }
            continue;
        }
        if !state.armed || (registration.condition.rising_edge && state.last_match) {
            state.initialized = true;
            state.last_match = true;
            state.pending_offline_match = false;
            state.last_skip_reason = None;
            continue;
        }
        state.initialized = true;
        state.last_match = true;
        state.armed = false;
        state.pending_offline_match = false;
        state.last_skip_reason = None;
        evaluated.push((registration.clone(), None, true));
    }

    let mut dispatches = Vec::new();
    for (registration, reason, should_fire) in evaluated {
        let idempotency_key = live_trigger_idempotency_key(
            &session_id,
            epoch,
            &registration.binding,
            observation.sequence,
        );
        let audit = LiveTriggerAudit {
            trigger_id: format!("trigger:{}", &idempotency_key[8..32]),
            binding_id: registration.binding.binding_id.clone(),
            condition_revision: registration.condition.revision,
            observation_id: observation.observation_id.clone(),
            observation_sequence: observation.sequence,
            source_device_id: source_device_id.clone(),
            observation_source: observation.source,
            idempotency_key: idempotency_key.clone(),
            outcome: if should_fire {
                LiveTriggerOutcome::Fired
            } else {
                LiveTriggerOutcome::Skipped
            },
            evaluated_at_ms: unix_time_millis(),
            authorized_by: registration.binding.authorized_by.clone(),
            action_request_id: None,
            reason,
        };
        if !should_fire {
            push_trigger_audit(record, audit);
            continue;
        }
        if !remember_trigger_idempotency(record, &idempotency_key) {
            continue;
        }
        record
            .pending_trigger_dispatches
            .insert(idempotency_key.clone());
        let mut reserved = audit.clone();
        reserved.reason = Some("surface_action_dispatch_reserved".to_owned());
        push_trigger_audit(record, reserved);
        dispatches.push(LiveTriggerDispatch {
            session_id: session_id.clone(),
            epoch,
            audit,
            target: registration.target,
            observation: observation.clone(),
        });
    }
    dispatches
}

fn trigger_safety_skip_reason(
    condition: &LiveTriggerCondition,
    observation: &LiveObservation,
) -> Option<&'static str> {
    match observation.state {
        LiveObservationState::Unknown => return Some("observation_unknown"),
        LiveObservationState::Stale => return Some("observation_stale"),
        LiveObservationState::Error => return Some("observation_error"),
        LiveObservationState::Stable | LiveObservationState::Triggered => {}
        LiveObservationState::Detected | LiveObservationState::Observing => {
            return Some("observation_not_stable")
        }
    }
    if observation.source == LiveObservationSource::Unknown {
        return Some("observation_source_unknown");
    }
    if confidence_rank(observation.confidence) < confidence_rank(condition.minimum_confidence) {
        return Some("observation_confidence_too_low");
    }
    let Some(stable_since_ms) = observation.stable_since_ms else {
        return Some("observation_stability_missing");
    };
    if observation.observed_at_ms.saturating_sub(stable_since_ms)
        < u64::from(condition.stable_for_ms)
    {
        return Some("observation_not_stable_long_enough");
    }
    None
}

fn confidence_rank(value: LiveObservationConfidence) -> u8 {
    match value {
        LiveObservationConfidence::Low => 0,
        LiveObservationConfidence::Medium => 1,
        LiveObservationConfidence::High => 2,
        LiveObservationConfidence::Exact => 3,
    }
}

fn trigger_condition_matches(
    condition: &LiveTriggerCondition,
    observation: &LiveObservation,
) -> bool {
    let Some(current) = observation.value.as_ref() else {
        return false;
    };
    let (current, expected) = if let Some(selector) = condition.operand.as_object() {
        if selector.len() == 2 && selector.contains_key("path") && selector.contains_key("value") {
            let Some(path) = selector.get("path").and_then(Value::as_str) else {
                return false;
            };
            let Some(selected) = current.pointer(path) else {
                return false;
            };
            (selected, &selector["value"])
        } else {
            (current, &condition.operand)
        }
    } else {
        (current, &condition.operand)
    };
    match condition.operator {
        LiveConditionOperator::Equals => current == expected,
        LiveConditionOperator::NotEquals => current != expected,
        LiveConditionOperator::Contains => match (current, expected) {
            (Value::String(value), Value::String(needle)) => value.contains(needle),
            (Value::Array(values), expected) => values.contains(expected),
            _ => false,
        },
        operator => compare_trigger_numbers(current, expected, operator),
    }
}

fn compare_trigger_numbers(
    current: &Value,
    expected: &Value,
    operator: LiveConditionOperator,
) -> bool {
    let (Some(current), Some(expected)) = (current.as_f64(), expected.as_f64()) else {
        return false;
    };
    if !current.is_finite() || !expected.is_finite() {
        return false;
    }
    match operator {
        LiveConditionOperator::GreaterThan => current > expected,
        LiveConditionOperator::GreaterOrEqual => current >= expected,
        LiveConditionOperator::LessThan => current < expected,
        LiveConditionOperator::LessOrEqual => current <= expected,
        _ => false,
    }
}
