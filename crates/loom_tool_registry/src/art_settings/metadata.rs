use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::model::ArtUserSettings;
use crate::ToolDefinition;

#[must_use]
pub fn art_is_locally_authored(tool: &ToolDefinition) -> bool {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("authoring"))
        .is_some_and(Value::is_object)
}

pub fn apply_settings_metadata(tool: &mut ToolDefinition, settings: &ArtUserSettings) {
    if art_is_locally_authored(tool) {
        if let Some(name) = settings
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            tool.name = name.to_owned();
        }
        if let Some(description) = &settings.description {
            tool.description = description.clone();
        }
    }

    let secret_parameters = secret_parameter_ids(tool);
    let metadata = tool
        .metadata
        .get_or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let metadata = metadata.as_object_mut().expect("metadata normalized");
    let mut defaults = settings.defaults.clone();
    for secret in secret_parameters {
        defaults.remove(&secret);
    }
    metadata.insert(
        "artUserSettings".to_owned(),
        serde_json::json!({
            "autoUpdate": settings.auto_update,
            "defaults": defaults,
            "valueBindings": settings.value_bindings,
            "credentialBindings": settings.credential_bindings,
        }),
    );
}

#[must_use]
pub fn merge_tool_arguments(tool: &ToolDefinition, arguments: Value) -> Value {
    let mut defaults = manifest_parameter_defaults(tool);
    let secret_parameters = secret_parameter_ids(tool);
    let value_binding_ids = tool_value_bindings(tool)
        .into_keys()
        .collect::<BTreeSet<_>>();
    if let Some(user_defaults) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("defaults"))
        .and_then(Value::as_object)
    {
        defaults.extend(user_defaults.clone());
    }
    defaults.retain(|id, _| !secret_parameters.contains(id) && !value_binding_ids.contains(id));
    if defaults.is_empty() {
        return arguments;
    }

    let Some(mut explicit) = arguments.as_object().cloned() else {
        return arguments;
    };
    if explicit.contains_key("params") || explicit.contains_key("inputs") {
        let explicit_params = explicit
            .get("params")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        defaults.extend(explicit_params);
        for disabled in explicit
            .get("disabledParams")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            defaults.remove(disabled);
        }
        explicit.insert("params".to_owned(), Value::Object(defaults));
        Value::Object(explicit)
    } else {
        defaults.extend(explicit);
        Value::Object(defaults)
    }
}

pub(super) fn tool_value_bindings(tool: &ToolDefinition) -> BTreeMap<String, String> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("valueBindings"))
        .and_then(|bindings| serde_json::from_value(bindings.clone()).ok())
        .unwrap_or_default()
}

fn manifest_parameter_defaults(tool: &ToolDefinition) -> Map<String, Value> {
    let mut defaults = Map::new();
    for parameter in &tool.params {
        let Some(parameter) = parameter.as_object() else {
            continue;
        };
        let Some(id) = parameter.get("id").and_then(Value::as_str) else {
            continue;
        };
        if parameter_is_secret(parameter) {
            continue;
        }
        if let Some(value) = parameter.get("default") {
            defaults.insert(id.to_owned(), value.clone());
        }
    }
    defaults
}

fn secret_parameter_ids(tool: &ToolDefinition) -> BTreeSet<String> {
    tool.params
        .iter()
        .filter_map(Value::as_object)
        .filter(|parameter| parameter_is_secret(parameter))
        .filter_map(|parameter| {
            parameter
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn parameter_is_secret(parameter: &Map<String, Value>) -> bool {
    parameter
        .get("secret")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || parameter
            .get("type")
            .or_else(|| parameter.get("data_type"))
            .and_then(Value::as_str)
            == Some("secret")
}
