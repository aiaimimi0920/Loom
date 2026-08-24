// Supported MCP input-schema validation and argument normalization.
fn normalize_arguments(
    arguments: &Value,
    schema: &Value,
    argument_aliases: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<Value, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "MCP tool arguments must be a JSON object".to_owned())?;
    validate_supported_tool_schema(schema)?;
    let properties = schema.get("properties").and_then(Value::as_object);
    let rejects_undeclared = schema.get("additionalProperties") == Some(&Value::Bool(false));
    let mut normalized = Map::new();
    for (name, value) in arguments {
        let property_schema = properties.and_then(|properties| properties.get(name));
        if rejects_undeclared && property_schema.is_none() {
            continue;
        }
        let aliases = argument_aliases.get(name);
        normalized.insert(
            name.clone(),
            normalize_argument(name, value, property_schema, aliases)?,
        );
    }
    if let Some(required) = schema.get("required") {
        let required = required.as_array().ok_or_else(|| {
            "MCP tool input schema required must be an array of strings".to_owned()
        })?;
        for name in required {
            let name = name.as_str().ok_or_else(|| {
                "MCP tool input schema required must contain only strings".to_owned()
            })?;
            if !normalized.contains_key(name) {
                return Err(format!("MCP tool argument `{name}` is required"));
            }
        }
    }
    Ok(Value::Object(normalized))
}

fn validate_supported_tool_schema(schema: &Value) -> Result<(), String> {
    let schema = schema
        .as_object()
        .ok_or_else(|| "MCP tool input schema must be a JSON object".to_owned())?;
    for unsupported in [
        "$ref",
        "allOf",
        "anyOf",
        "oneOf",
        "not",
        "if",
        "then",
        "else",
        "patternProperties",
        "dependentSchemas",
        "unevaluatedProperties",
    ] {
        if schema.contains_key(unsupported) {
            return Err(format!(
                "MCP tool input schema feature `{unsupported}` is not supported"
            ));
        }
    }
    if let Some(schema_type) = schema.get("type") {
        let object_type = schema_type == "object"
            || schema_type
                .as_array()
                .is_some_and(|types| types.iter().any(|value| value == "object"));
        if !object_type {
            return Err("MCP tool input schema root type must be object".to_owned());
        }
    }
    if let Some(properties) = schema.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| "MCP tool input schema properties must be an object".to_owned())?;
        for (name, property) in properties {
            let property = property.as_object().ok_or_else(|| {
                format!("MCP tool input schema property `{name}` must be an object")
            })?;
            if ["$ref", "allOf", "anyOf", "oneOf", "not"]
                .iter()
                .any(|keyword| property.contains_key(*keyword))
            {
                return Err(format!(
                    "MCP tool input schema property `{name}` uses an unsupported composed schema"
                ));
            }
            if ["object", "array"].iter().any(|schema_type| {
                schema_type_matches(&Value::Object(property.clone()), schema_type)
            }) {
                return Err(format!(
                    "MCP tool input schema property `{name}` uses unsupported nested type"
                ));
            }
        }
    }
    Ok(())
}

fn normalize_argument(
    name: &str,
    value: &Value,
    schema: Option<&Value>,
    aliases: Option<&BTreeMap<String, String>>,
) -> Result<Value, String> {
    let mut value = value.clone();
    if let (Some(raw), Some(aliases)) = (value.as_str().map(str::trim), aliases) {
        if let Some((_, canonical)) = aliases
            .iter()
            .find(|(alias, _)| alias.eq_ignore_ascii_case(raw))
        {
            value = Value::String(canonical.clone());
        }
    }
    let Some(schema) = schema else {
        return Ok(value);
    };
    if value.is_null() {
        return schema_type_matches(schema, "null")
            .then_some(Value::Null)
            .ok_or_else(|| format!("MCP tool argument `{name}` must not be null"));
    }
    if schema_type_matches(schema, "integer") {
        value = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            .map(Value::from)
            .ok_or_else(|| format!("MCP tool argument `{name}` must be an integer"))?;
    } else if schema_type_matches(schema, "number") {
        value = value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            .map(Value::from)
            .ok_or_else(|| format!("MCP tool argument `{name}` must be a number"))?;
    } else if schema_type_matches(schema, "boolean") {
        value = value
            .as_bool()
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                        "1" | "true" | "yes" | "on" => Some(true),
                        "0" | "false" | "no" | "off" => Some(false),
                        _ => None,
                    })
            })
            .map(Value::from)
            .ok_or_else(|| format!("MCP tool argument `{name}` must be a boolean"))?;
    } else if schema_type_matches(schema, "string") && !value.is_string() {
        return Err(format!("MCP tool argument `{name}` must be a string"));
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let canonical = value.as_str().and_then(|raw| {
            values
                .iter()
                .filter_map(Value::as_str)
                .find(|candidate| candidate.eq_ignore_ascii_case(raw))
        });
        if let Some(canonical) = canonical {
            value = Value::String(canonical.to_owned());
        } else if !values.contains(&value) {
            return Err(format!(
                "MCP tool argument `{name}` is not one of the declared enum values"
            ));
        }
    }
    Ok(value)
}

fn schema_type_matches(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(actual)) => actual == expected,
        Some(Value::Array(actual)) => actual.iter().any(|value| value.as_str() == Some(expected)),
        _ => false,
    }
}
