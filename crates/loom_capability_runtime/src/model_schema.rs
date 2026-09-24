//! Bounded, reference-free output constraints for the process-scoped model broker.
use serde_json::Value;

pub(super) fn validate(schema: Option<&Value>) -> bool {
    schema.is_none_or(|schema| {
        serde_json::to_vec(schema).is_ok_and(|bytes| bytes.len() <= 32 * 1024)
            && validate_node(schema, 0, &mut 2048)
    })
}

fn validate_node(schema: &Value, depth: usize, remaining: &mut usize) -> bool {
    if depth > 12 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let Some(fields) = schema.as_object() else {
        return false;
    };
    !fields.is_empty()
        && fields.iter().all(|(key, value)| match key.as_str() {
            "type" => matches!(
                value.as_str(),
                Some("object" | "array" | "string" | "integer" | "number" | "boolean" | "null")
            ),
            "properties" => value.as_object().is_some_and(|fields| {
                fields.len() <= 128
                    && fields.iter().all(|(name, value)| {
                        name.len() <= 128 && validate_node(value, depth + 1, remaining)
                    })
            }),
            "prefixItems" => value.as_array().is_some_and(|items| {
                items.len() <= 128
                    && items
                        .iter()
                        .all(|item| validate_node(item, depth + 1, remaining))
            }),
            "items" => validate_node(value, depth + 1, remaining),
            "required" => value.as_array().is_some_and(|items| {
                items.len() <= 128
                    && items
                        .iter()
                        .all(|item| item.as_str().is_some_and(|name| name.len() <= 128))
            }),
            "additionalProperties" => value.as_bool() == Some(false),
            "minItems" | "maxItems" | "const" => value.as_u64().is_some_and(|value| value <= 128),
            "minLength" | "maxLength" => value.as_u64().is_some_and(|value| value <= 65_536),
            // References, regexes, and combinators can introduce I/O or unbounded grammar expansion.
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn permits_bounded_indexed_text_without_schema_references() {
        assert!(validate(Some(&json!({
            "type": "object", "additionalProperties": false, "required": ["translations"],
            "properties": { "translations": {
                "type": "array", "minItems": 1, "maxItems": 1,
                "prefixItems": [{ "type": "array", "minItems": 2, "maxItems": 2,
                    "prefixItems": [{ "const": 0 }, { "type": "string", "minLength": 1 }] }]
            } }
        }))));
        assert!(validate(None));
    }

    #[test]
    fn rejects_external_references_regex_and_excessive_expansion() {
        for schema in [
            json!({ "$ref": "https://example.com/schema.json" }),
            json!({ "type": "string", "pattern": "(a+)+$" }),
            json!({ "type": "array", "maxItems": 129 }),
            json!({ "type": "object", "additionalProperties": true }),
            json!({ "oneOf": [{ "type": "string" }] }),
        ] {
            assert!(!validate(Some(&schema)));
        }
        let mut nested = json!({ "type": "string" });
        for _ in 0..14 {
            nested = json!({ "type": "array", "items": nested });
        }
        assert!(!validate(Some(&nested)));
    }
}
