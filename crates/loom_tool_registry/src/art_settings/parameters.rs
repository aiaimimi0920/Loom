use serde_json::Value;

use super::model::ArtParameterDefinition;
use crate::credentials::CredentialValueType;
use crate::ToolDefinition;

#[must_use]
pub fn art_parameter_definitions(tool: &ToolDefinition) -> Vec<ArtParameterDefinition> {
    tool.params
        .iter()
        .filter_map(|parameter| {
            let parameter = parameter.as_object()?;
            let id = parameter.get("id").and_then(Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }
            let widget = parameter.get("widget").and_then(Value::as_str);
            let parameter_type = parameter
                .get("type")
                .or_else(|| parameter.get("data_type"))
                .and_then(Value::as_str)
                .or_else(|| match widget {
                    Some("slider" | "number") => Some("number"),
                    Some("checkbox" | "toggle") => Some("boolean"),
                    Some("select" | "enum") => Some("enum"),
                    _ => None,
                })
                .unwrap_or("string")
                .trim()
                .to_ascii_lowercase();
            let secret = parameter
                .get("secret")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || parameter_type == "secret";
            Some(ArtParameterDefinition {
                id: id.to_owned(),
                label: parameter
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
                parameter_type,
                required: parameter
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                secret,
                default: (!secret)
                    .then(|| parameter.get("default").cloned())
                    .flatten(),
                options: parameter.get("options").cloned(),
                minimum: parameter
                    .get("minimum")
                    .or_else(|| parameter.get("min"))
                    .cloned(),
                maximum: parameter
                    .get("maximum")
                    .or_else(|| parameter.get("max"))
                    .cloned(),
                step: parameter.get("step").cloned(),
            })
        })
        .collect()
}

#[must_use]
pub fn credential_value_type_matches_parameter(
    parameter: &ArtParameterDefinition,
    value_type: CredentialValueType,
) -> bool {
    if parameter.secret {
        return value_type == CredentialValueType::String;
    }
    match parameter.parameter_type.as_str() {
        "number" => matches!(
            value_type,
            CredentialValueType::Number | CredentialValueType::Integer
        ),
        "integer" => value_type == CredentialValueType::Integer,
        "boolean" => value_type == CredentialValueType::Boolean,
        "json" => value_type == CredentialValueType::Json,
        _ => value_type == CredentialValueType::String,
    }
}

pub fn validate_parameter_value(
    parameter: &ArtParameterDefinition,
    value: &Value,
) -> Result<(), String> {
    let type_matches = if parameter.secret {
        value.is_string()
    } else {
        match parameter.parameter_type.as_str() {
            "number" => value.is_number(),
            "integer" => value
                .as_number()
                .is_some_and(|number| number.is_i64() || number.is_u64()),
            "boolean" => value.is_boolean(),
            "json" => true,
            _ => value.is_string(),
        }
    };
    if !type_matches {
        return Err(format!(
            "parameter `{}` must be `{}`",
            parameter.id, parameter.parameter_type
        ));
    }
    if let Some(options) = parameter.options.as_ref().and_then(Value::as_array) {
        let allowed = options.iter().any(|option| {
            option
                .as_object()
                .and_then(|object| object.get("value"))
                .unwrap_or(option)
                == value
        });
        if !allowed {
            return Err(format!(
                "parameter `{}` is not an allowed option",
                parameter.id
            ));
        }
    }
    if let Some(actual) = value.as_f64() {
        if parameter
            .minimum
            .as_ref()
            .and_then(Value::as_f64)
            .is_some_and(|minimum| actual < minimum)
        {
            return Err(format!("parameter `{}` is below its minimum", parameter.id));
        }
        if parameter
            .maximum
            .as_ref()
            .and_then(Value::as_f64)
            .is_some_and(|maximum| actual > maximum)
        {
            return Err(format!("parameter `{}` exceeds its maximum", parameter.id));
        }
        if let Some(step) = parameter.step.as_ref().and_then(Value::as_f64) {
            if step > 0.0 {
                let base = parameter
                    .minimum
                    .as_ref()
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let offset = (actual - base) / step;
                if (offset - offset.round()).abs() > 1e-9 {
                    return Err(format!(
                        "parameter `{}` does not match its step",
                        parameter.id
                    ));
                }
            }
        }
    }
    Ok(())
}
