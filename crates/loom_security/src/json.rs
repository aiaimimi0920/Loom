//! Limits for JSON that arrives from outside the host.
//!
//! Untrusted JSON is dangerous in two ways that a schema check does not catch: it can be
//! large enough to exhaust memory while being copied between processes, and it can be nested
//! deeply enough to exhaust the stack in any consumer that walks it recursively. Both limits
//! belong in one place so that the daemon, the tool registry and the framework runtime host
//! reject the same payloads.
//!
//! The depth walk is itself bounded: it stops descending as soon as the budget is exceeded,
//! so checking a hostile value costs at most `max_depth` frames.

use serde_json::Value;

/// Byte limit for a single Surface action argument.
pub const MAX_SURFACE_ARGUMENT_BYTES: usize = 65_536;

/// Nesting limit for a single Surface action argument.
pub const MAX_SURFACE_ARGUMENT_DEPTH: usize = 16;

/// Byte limit for a fully resolved MCP argument object.
pub const MAX_RESOLVED_ARGUMENT_BYTES: usize = 262_144;

/// Nesting limit for a fully resolved MCP argument object.
pub const MAX_RESOLVED_ARGUMENT_DEPTH: usize = 24;

/// Byte limit for one JSON document read from a framework process on stdout.
pub const MAX_PROCESS_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Nesting limit for one JSON document read from a framework process on stdout.
pub const MAX_PROCESS_RESPONSE_DEPTH: usize = 32;

/// Report whether `value` nests no deeper than `max_depth` container levels.
///
/// Depth is measured on the values *inside* containers: a scalar has depth 0, `{"a": 1}` has
/// depth 1, and an empty container has depth 0 because it holds nothing. The walk
/// short-circuits on the first branch that exceeds the budget, so the recursion never goes
/// deeper than `max_depth + 1` frames regardless of how deep the value actually is.
pub fn value_is_within_depth(value: &Value, max_depth: usize) -> bool {
    depth_within(value, 0, max_depth)
}

fn depth_within(value: &Value, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| depth_within(value, depth + 1, max_depth)),
        Value::Object(values) => values
            .values()
            .all(|value| depth_within(value, depth + 1, max_depth)),
        _ => true,
    }
}

/// Reject a value that is either too large when encoded or nested too deeply.
///
/// `label` names the value in the error message so the caller does not have to wrap the
/// result; it is expected to be a fixed description such as `Surface argument \`symbol\``.
pub fn ensure_within_limits(
    value: &Value,
    label: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<(), String> {
    let encoded =
        serde_json::to_vec(value).map_err(|error| format!("cannot encode {label}: {error}"))?;
    if encoded.len() > max_bytes {
        return Err(format!("{label} exceeds the {max_bytes} byte limit"));
    }
    if !value_is_within_depth(value, max_depth) {
        return Err(format!(
            "{label} exceeds the nesting limit of {max_depth} levels"
        ));
    }
    Ok(())
}

/// Parse untrusted JSON text and apply both limits to the result.
///
/// The byte limit is checked against the raw text before parsing so a hostile document is
/// rejected without allocating a `Value` for it.
pub fn parse_within_limits(
    text: &str,
    label: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<Value, String> {
    if text.len() > max_bytes {
        return Err(format!("{label} exceeds the {max_bytes} byte limit"));
    }
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("cannot parse {label}: {error}"))?;
    if !value_is_within_depth(&value, max_depth) {
        return Err(format!(
            "{label} exceeds the nesting limit of {max_depth} levels"
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn nested_arrays(levels: usize) -> String {
        let mut text = String::with_capacity(levels * 2 + 1);
        for _ in 0..levels {
            text.push('[');
        }
        text.push('1');
        for _ in 0..levels {
            text.push(']');
        }
        text
    }

    #[test]
    fn scalars_and_shallow_containers_are_within_any_budget() {
        assert!(value_is_within_depth(&json!(1), 0));
        assert!(value_is_within_depth(&json!("text"), 0));
        assert!(value_is_within_depth(&Value::Null, 0));
        // An empty container holds no values, so there is nothing below the budget to exceed.
        assert!(value_is_within_depth(&json!([]), 0));
        assert!(value_is_within_depth(&json!({}), 0));
        assert!(!value_is_within_depth(&json!([1]), 0));
        assert!(value_is_within_depth(&json!({"a": 1}), 1));
        assert!(!value_is_within_depth(&json!({"a": {"b": 1}}), 1));
    }

    #[test]
    fn depth_is_measured_across_both_container_kinds() {
        let value = json!({"a": [{"b": [1]}]});
        assert!(value_is_within_depth(&value, 4));
        assert!(!value_is_within_depth(&value, 3));
    }

    #[test]
    fn a_deeply_nested_document_is_rejected_without_overflowing_the_stack() {
        let text = nested_arrays(96);
        let error = parse_within_limits(
            &text,
            "framework response",
            MAX_PROCESS_RESPONSE_BYTES,
            MAX_PROCESS_RESPONSE_DEPTH,
        )
        .expect_err("nesting limit");
        assert!(error.contains("nesting limit"), "unexpected error: {error}");
    }

    #[test]
    fn oversized_text_is_rejected_before_parsing() {
        let text = format!("\"{}\"", "a".repeat(64));
        let error = parse_within_limits(&text, "Surface argument", 16, 4).expect_err("byte limit");
        assert!(error.contains("byte limit"), "unexpected error: {error}");
    }

    #[test]
    fn valid_documents_round_trip() {
        let value = parse_within_limits(
            r#"{"symbol":"MSFT","points":[1,2,3]}"#,
            "Surface argument",
            MAX_SURFACE_ARGUMENT_BYTES,
            MAX_SURFACE_ARGUMENT_DEPTH,
        )
        .expect("parse");
        assert_eq!(value["symbol"], json!("MSFT"));
        assert!(ensure_within_limits(
            &value,
            "Surface argument",
            MAX_SURFACE_ARGUMENT_BYTES,
            MAX_SURFACE_ARGUMENT_DEPTH
        )
        .is_ok());
    }

    #[test]
    fn ensure_within_limits_reports_both_failures() {
        let value = json!({"blob": "x".repeat(128)});
        let error = ensure_within_limits(&value, "Surface argument", 32, 8).expect_err("bytes");
        assert!(error.contains("byte limit"), "unexpected error: {error}");

        let mut deep = json!(1);
        for _ in 0..12 {
            deep = json!([deep]);
        }
        let error = ensure_within_limits(&deep, "Surface argument", 4096, 8).expect_err("depth");
        assert!(error.contains("nesting limit"), "unexpected error: {error}");
    }
}
