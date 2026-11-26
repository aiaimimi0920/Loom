// Surface and local Art share a single condition state machine; the target governs only delivery, never reinterpreting observed evidence.
fn evaluate_live_trigger_state(
    condition: &LiveTriggerCondition,
    state: &mut LiveTriggerConditionState,
    observation: &LiveObservation,
    authorizer_online: bool,
) -> Option<(Option<String>, bool)> {
    let reason = trigger_safety_skip_reason(condition, observation);
    let matches = reason.is_none() && trigger_condition_matches(condition, observation);
    if let Some(reason) = reason {
        if condition.rearm {
            state.armed = true;
        }
        state.initialized = true;
        state.last_match = false;
        state.pending_offline_match = false;
        if state.last_skip_reason.as_deref() != Some(reason) {
            state.last_skip_reason = Some(reason.to_owned());
            return Some((Some(reason.to_owned()), false));
        }
        return None;
    }
    if !matches {
        if condition.rearm {
            state.armed = true;
        }
        state.initialized = true;
        state.last_match = false;
        state.pending_offline_match = false;
        state.last_skip_reason = None;
        return None;
    }
    let reason = if condition.rising_edge && !state.initialized {
        state.initialized = true;
        state.last_match = true;
        Some("rising_edge_not_observed")
    } else if !authorizer_online {
        state.initialized = true;
        state.last_match = false;
        state.pending_offline_match = true;
        Some("authorizer_offline")
    } else {
        None
    };
    if let Some(reason) = reason {
        if state.last_skip_reason.as_deref() != Some(reason) {
            state.last_skip_reason = Some(reason.to_owned());
            return Some((Some(reason.to_owned()), false));
        }
        return None;
    }
    let fire = state.armed && !(condition.rising_edge && state.last_match);
    state.initialized = true;
    state.last_match = true;
    state.pending_offline_match = false;
    state.last_skip_reason = None;
    if fire {
        state.armed = false;
        Some((None, true))
    } else {
        None
    }
}
