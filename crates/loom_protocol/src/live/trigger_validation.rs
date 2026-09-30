use super::{
    validation::{validate_live_identifier, validate_optional_reason, LiveProtocolError},
    LiveConditionOperator, LiveTriggerAudit, LiveTriggerBinding, LiveTriggerCondition,
    LIVE_MAX_TRIGGER_OPERAND,
};

pub(super) fn validate_trigger_binding(
    value: &LiveTriggerBinding,
) -> Result<(), LiveProtocolError> {
    for (identifier, field) in [
        (&value.binding_id, "binding_id"),
        (&value.observation_id, "observation_id"),
        (&value.authorized_by, "authorized_by"),
    ] {
        validate_live_identifier(identifier, field)?;
    }
    Ok(())
}

pub fn validate_trigger_condition(value: &LiveTriggerCondition) -> Result<(), LiveProtocolError> {
    validate_live_identifier(&value.condition_id, "condition_id")?;
    validate_live_identifier(&value.observation_id, "observation_id")?;
    if value.revision == 0 || value.stable_for_ms > 86_400_000 {
        return Err(LiveProtocolError::InvalidField("trigger_condition"));
    }
    let bytes = serde_json::to_vec(&value.operand)
        .map_err(|_| LiveProtocolError::InvalidField("trigger_operand"))?;
    if bytes.len() > LIVE_MAX_TRIGGER_OPERAND || !trigger_operand_shape_is_bounded(&value.operand) {
        return Err(LiveProtocolError::InvalidField("trigger_operand"));
    }
    let expected = trigger_operand_expected_value(&value.operand)?;
    if matches!(
        value.operator,
        LiveConditionOperator::GreaterThan
            | LiveConditionOperator::GreaterOrEqual
            | LiveConditionOperator::LessThan
            | LiveConditionOperator::LessOrEqual
    ) && expected.as_f64().is_none()
    {
        return Err(LiveProtocolError::InvalidField("trigger_operand_type"));
    }
    Ok(())
}

fn trigger_operand_expected_value(
    value: &serde_json::Value,
) -> Result<&serde_json::Value, LiveProtocolError> {
    let Some(object) = value.as_object() else {
        return Ok(value);
    };
    if !object.contains_key("path") && !object.contains_key("value") {
        return Ok(value);
    }
    if object.len() != 2 {
        return Err(LiveProtocolError::InvalidField("trigger_operand_selector"));
    }
    let path = object
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| path.starts_with('/') && path.len() <= 256)
        .ok_or(LiveProtocolError::InvalidField("trigger_operand_selector"))?;
    if path == "/" {
        return Err(LiveProtocolError::InvalidField("trigger_operand_selector"));
    }
    object
        .get("value")
        .ok_or(LiveProtocolError::InvalidField("trigger_operand_selector"))
}

fn trigger_operand_shape_is_bounded(root: &serde_json::Value) -> bool {
    let mut pending = vec![(root, 1usize)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes = nodes.saturating_add(1);
        if depth > 16 || nodes > 512 {
            return false;
        }
        match value {
            serde_json::Value::Array(values) => {
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            serde_json::Value::Object(values) => {
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    true
}

pub(super) fn validate_trigger_audit(value: &LiveTriggerAudit) -> Result<(), LiveProtocolError> {
    for (identifier, field) in [
        (&value.trigger_id, "trigger_id"),
        (&value.binding_id, "binding_id"),
        (&value.observation_id, "observation_id"),
        (&value.source_device_id, "source_device_id"),
        (&value.idempotency_key, "idempotency_key"),
        (&value.authorized_by, "authorized_by"),
    ] {
        validate_live_identifier(identifier, field)?;
    }
    if let Some(request_id) = value.action_request_id.as_deref() {
        validate_live_identifier(request_id, "action_request_id")?;
    }
    if value.condition_revision == 0
        || value.observation_sequence == 0
        || value.evaluated_at_ms == 0
    {
        return Err(LiveProtocolError::InvalidField("observation_sequence"));
    }
    validate_optional_reason(value.reason.as_deref())
}
