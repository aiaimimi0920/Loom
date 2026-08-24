use super::*;

pub(super) fn split_arguments(
    tool: &ToolDefinition,
    arguments: &Value,
) -> (Value, Value, Vec<String>) {
    let Some(object) = arguments.as_object() else {
        return (arguments.clone(), Value::Object(Map::new()), Vec::new());
    };
    let disabled = object
        .get("disabledParams")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if object.contains_key("inputs") || object.contains_key("params") {
        let inputs = object.get("inputs").cloned().unwrap_or_else(|| json!({}));
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        return (inputs, params, disabled);
    }

    let parameter_ids = tool
        .params
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|parameter| parameter.get("id").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let mut inputs = Map::new();
    let mut params = Map::new();
    for (key, value) in object {
        if key == "disabledParams" {
            continue;
        }
        if parameter_ids.contains(key.as_str()) {
            params.insert(key.clone(), value.clone());
        } else {
            inputs.insert(key.clone(), value.clone());
        }
    }
    (Value::Object(inputs), Value::Object(params), disabled)
}

fn selected_image_candidate_index(output: &Map<String, Value>, candidates: &[Value]) -> usize {
    if let Some(index) = output.get("selectedIndex").and_then(Value::as_u64) {
        return usize::try_from(index)
            .unwrap_or(usize::MAX)
            .min(candidates.len().saturating_sub(1));
    }
    let selected_id = output.get("selectedCandidate").and_then(Value::as_str);
    selected_id
        .and_then(|selected_id| {
            candidates.iter().position(|candidate| {
                candidate
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate_id| candidate_id == selected_id)
            })
        })
        .unwrap_or_default()
}

/// Ceilings the host applies to a framework's candidate array.
///
/// `normalize_framework_image_output` bounds the single output image — absolute path, inside an
/// execution output root, under `MAX_FRAMEWORK_IMAGE_OUTPUT_BYTES`, replaced by exactly one data
/// URL — but none of that reaches `response.candidates`, which the framework builds itself and the
/// host previously inserted verbatim. An image candidate normally carries a full data URL, and the
/// finished value is cloned through the store while its mutex is held on the Surface action path, so
/// an Art returning a large grid costs that memory on the interaction hot path. The item count
/// matches `MAX_MCP_IMAGE_CANDIDATES` on the MCP tool path; the byte budget spans the whole array
/// rather than a single item.
pub(super) const MAX_FRAMEWORK_CANDIDATES: usize = 64;
pub(super) const MAX_FRAMEWORK_CANDIDATE_BYTES: usize = 32 * 1024 * 1024;

/// Approximate what a candidate occupies, by summing the strings and keys it holds rather than
/// serializing it, so measuring copies nothing.
///
/// Nesting depth needs no guard: the value was deserialized from the framework's stdout, and
/// `serde_json` refuses input deeper than its own recursion limit, so the structure walked here is
/// already bounded.
fn candidate_value_bytes(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Array(items) => items.iter().map(candidate_value_bytes).sum(),
        Value::Object(entries) => entries
            .iter()
            .map(|(key, value)| key.len() + candidate_value_bytes(value))
            .sum(),
        _ => 0,
    }
}

/// Truncate a candidate array to the host's ceilings, reporting how many items were dropped.
///
/// Truncation keeps the leading items, which is where the selected candidate sits unless the Art
/// says otherwise, and returns the drop count so a consumer can tell a truncated grid from a short
/// one. One item larger than the entire byte budget still survives as the only item: dropping it
/// would leave a grid with no images at all, which is worse than honouring the budget exactly.
fn bound_framework_candidates(mut candidates: Vec<Value>) -> (Vec<Value>, usize) {
    let mut dropped = candidates.len().saturating_sub(MAX_FRAMEWORK_CANDIDATES);
    candidates.truncate(MAX_FRAMEWORK_CANDIDATES);
    let mut budget = MAX_FRAMEWORK_CANDIDATE_BYTES;
    let mut kept = 0;
    for candidate in &candidates {
        let bytes = candidate_value_bytes(candidate);
        if kept > 0 && bytes > budget {
            break;
        }
        budget = budget.saturating_sub(bytes);
        kept += 1;
    }
    dropped += candidates.len() - kept;
    candidates.truncate(kept);
    (candidates, dropped)
}

/// The candidate keys every consumer reads, and the producer keys that may stand in for them.
///
/// The MCP tool path emits `{index, title, imageUrl, thumbnailUrl, sourcePageUrl, width, height}`
/// (`lib.rs`), and both consumers — the Hook canvas result strip and Hook's
/// `artDeliveryCandidates` — key each item on `imageUrl` and drop items without it. Framework Arts
/// author their own candidate objects and reach for the names their own runtime uses, so the host
/// normalizes them here instead of requiring every Art to know the wire shape.
const CANDIDATE_IMAGE_URL_SOURCES: &[&str] = &[
    "imageUrl",
    "image_url",
    "url",
    "src",
    "data",
    "dataUrl",
    "data_url",
    "thumbnailUrl",
    "thumbnail_url",
    "thumbnail",
];
const CANDIDATE_THUMBNAIL_URL_SOURCES: &[&str] =
    &["thumbnailUrl", "thumbnail_url", "thumbnail", "preview"];
const CANDIDATE_SOURCE_PAGE_URL_SOURCES: &[&str] = &[
    "sourcePageUrl",
    "source_page_url",
    "sourceUrl",
    "source_url",
    "pageUrl",
    "page_url",
];

fn first_candidate_string(item: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        item.get(*key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

/// Fill in the canonical candidate keys without discarding what the Art already sent.
///
/// Producer-specific keys are left in place: Hook's candidate strip also renders `thumbnail`, and
/// an Art may attach its own fields. Only the canonical keys are added, and only when they are
/// missing or empty, so an Art that already speaks the wire shape passes through unchanged. An
/// item with no usable image reference at all is left alone rather than given a fabricated one; the
/// consumers drop it exactly as before.
fn normalize_image_candidate_item(index: usize, candidate: Value) -> Value {
    let mut item = match candidate {
        Value::Object(item) => item,
        other => return other,
    };
    if let Some(image_url) = first_candidate_string(&item, CANDIDATE_IMAGE_URL_SOURCES) {
        item.insert("imageUrl".to_owned(), Value::String(image_url));
    }
    if let Some(thumbnail_url) = first_candidate_string(&item, CANDIDATE_THUMBNAIL_URL_SOURCES) {
        item.insert("thumbnailUrl".to_owned(), Value::String(thumbnail_url));
    }
    if let Some(source_page_url) = first_candidate_string(&item, CANDIDATE_SOURCE_PAGE_URL_SOURCES)
    {
        item.insert("sourcePageUrl".to_owned(), Value::String(source_page_url));
    }
    if !item.get("index").is_some_and(Value::is_u64) {
        item.insert("index".to_owned(), json!(index));
    }
    Value::Object(item)
}

fn insert_image_candidate_metadata(
    output: &mut Map<String, Value>,
    candidates: Vec<Value>,
    dropped: usize,
) {
    let selected_index = selected_image_candidate_index(output, &candidates);
    let candidates = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| normalize_image_candidate_item(index, candidate))
        .collect::<Vec<_>>();
    let metadata = output
        .entry("loomMetadata".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    metadata
        .as_object_mut()
        .expect("loomMetadata was normalized to an object")
        .insert(
            "candidates".to_owned(),
            json!({
                "kind": "image.candidates",
                "selectedIndex": selected_index,
                "droppedItems": dropped,
                "items": candidates,
            }),
        );
}

pub(super) fn response_to_tool_value(
    tool: &ToolDefinition,
    response: FrameworkExecuteResponse,
) -> Value {
    let (candidates, dropped_candidates) = bound_framework_candidates(response.candidates);
    let has_candidates = !candidates.is_empty();
    let has_image_candidates =
        has_candidates && tool.outputs.iter().any(is_image_output_definition);
    let has_cache = !response.cache.is_null();
    let has_execution_metadata = response.diagnostics.is_some() || !response.events.is_empty();
    if !has_candidates && !has_cache && !has_execution_metadata {
        return response.output;
    }
    let execution_metadata = has_execution_metadata.then(|| {
        json!({
            "diagnostics": response.diagnostics,
            "events": response.events,
        })
    });
    if let Value::Object(mut output) = response.output {
        if has_image_candidates {
            insert_image_candidate_metadata(&mut output, candidates, dropped_candidates);
        } else if has_candidates {
            output.insert("candidates".to_owned(), Value::Array(candidates));
        }
        if has_cache {
            output.insert("cache".to_owned(), response.cache);
        }
        if let Some(execution_metadata) = execution_metadata {
            output.insert("_loomExecution".to_owned(), execution_metadata);
        }
        Value::Object(output)
    } else {
        let mut result = Map::new();
        result.insert("output".to_owned(), response.output);
        if has_image_candidates {
            insert_image_candidate_metadata(&mut result, candidates, dropped_candidates);
        } else if has_candidates {
            result.insert("candidates".to_owned(), Value::Array(candidates));
        }
        if has_cache {
            result.insert("cache".to_owned(), response.cache);
        }
        if let Some(execution_metadata) = execution_metadata {
            result.insert("_loomExecution".to_owned(), execution_metadata);
        }
        Value::Object(result)
    }
}

pub(super) fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("loom-{}-{nanos}", std::process::id())
}
