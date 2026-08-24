use serde_json::{json, Map, Value};

use super::{RunEventDraft, RunStoreError, RunStoreResult};

pub(super) const MAX_RUN_JSON_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_EVENT_FIELDS_JSON_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_RUN_STORE_JSON_DEPTH: usize = 64;
pub(super) const MAX_EVENT_BATCH: usize = 4_096;

pub(super) fn validated_run_identity(run: &Value) -> RunStoreResult<(&str, &str)> {
    let object = run
        .as_object()
        .ok_or_else(|| RunStoreError::InvalidRun("run must be an object".to_owned()))?;
    let run_id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RunStoreError::InvalidRun("id must be a non-empty string".to_owned()))?;
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RunStoreError::InvalidRun("status must be a non-empty string".to_owned()))?;
    Ok((run_id, status))
}

pub(super) fn serialized_run_document(run: &Value) -> RunStoreResult<String> {
    if !loom_security::json::value_is_within_depth(run, MAX_RUN_STORE_JSON_DEPTH) {
        return Err(RunStoreError::InvalidRun(format!(
            "run evidence exceeds the nesting limit of {MAX_RUN_STORE_JSON_DEPTH} levels"
        )));
    }
    let encoded = serde_json::to_string(run)?;
    if encoded.len() > MAX_RUN_JSON_BYTES {
        return Err(RunStoreError::InvalidRun(format!(
            "run evidence exceeds the {MAX_RUN_JSON_BYTES} byte limit"
        )));
    }
    Ok(encoded)
}

pub(super) fn validate_event_draft(event: &RunEventDraft) -> RunStoreResult<()> {
    if event.kind.trim().is_empty() {
        return Err(RunStoreError::InvalidEvent("kind is required".to_owned()));
    }
    let encoded = serde_json::to_vec(&event.fields)?;
    if encoded.len() > MAX_EVENT_FIELDS_JSON_BYTES {
        return Err(RunStoreError::InvalidEvent(format!(
            "fields exceed the {MAX_EVENT_FIELDS_JSON_BYTES} byte limit"
        )));
    }
    if event.fields.values().any(|value| {
        !loom_security::json::value_is_within_depth(
            value,
            MAX_RUN_STORE_JSON_DEPTH.saturating_sub(1),
        )
    }) {
        return Err(RunStoreError::InvalidEvent(format!(
            "fields exceed the nesting limit of {MAX_RUN_STORE_JSON_DEPTH} levels"
        )));
    }
    Ok(())
}

pub(super) fn validate_event_batch(events: &[RunEventDraft]) -> RunStoreResult<()> {
    if events.len() > MAX_EVENT_BATCH {
        return Err(RunStoreError::InvalidEvent(format!(
            "event batch exceeds the {MAX_EVENT_BATCH} item limit"
        )));
    }
    events.iter().try_for_each(validate_event_draft)
}

pub(super) fn event_value(
    sequence: u64,
    run_id: &str,
    kind: &str,
    fields: Map<String, Value>,
) -> Value {
    let mut event = Value::Object(fields);
    let target = event.as_object_mut().expect("event fields object");
    target.insert("sequence".to_owned(), json!(sequence));
    target.insert("kind".to_owned(), Value::String(kind.to_owned()));
    target.insert("run_id".to_owned(), Value::String(run_id.to_owned()));
    event
}

pub(super) fn interrupt_run(run: &mut Value) -> RunStoreResult<RunEventDraft> {
    let (_, status) = validated_run_identity(run)?;
    if status != "running" {
        return Err(RunStoreError::InvalidRun(
            "only running runs can be interrupted".to_owned(),
        ));
    }

    let object = run
        .as_object_mut()
        .expect("validated run must remain an object");
    object.insert("status".to_owned(), json!("failed"));
    object.insert(
        "error".to_owned(),
        json!({
            "code": "daemon_restarted",
            "message": "Run was interrupted by a daemon restart"
        }),
    );

    RunEventDraft::new(
        "run_interrupted",
        json!({
            "status": "failed",
            "error": { "code": "daemon_restarted" }
        }),
    )
}

pub(super) fn sequence_values(start: u64, count: usize) -> RunStoreResult<Vec<u64>> {
    let count = u64::try_from(count)
        .map_err(|_| RunStoreError::Integrity("event batch is too large".to_owned()))?;
    start
        .checked_add(count)
        .ok_or_else(|| RunStoreError::Integrity("event sequence overflow".to_owned()))?;
    Ok((1..=count).map(|offset| start + offset).collect())
}
