use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::metadata::tool_value_bindings;
use super::model::ArtSettingsError;
use super::parameters::{
    art_parameter_definitions, credential_value_type_matches_parameter, validate_parameter_value,
};
use crate::credentials::CredentialStore;
use crate::ToolDefinition;

pub fn resolve_tool_value_bindings(
    tool: &ToolDefinition,
    arguments: Value,
) -> Result<Value, ArtSettingsError> {
    let bindings = tool_value_bindings(tool);
    if bindings.is_empty() {
        return Ok(arguments);
    }
    let control_plane_root = control_plane_root_for_tool(tool).ok_or_else(|| {
        ArtSettingsError::ParameterBinding(
            "cannot locate the Loom control-plane root for this installed Art".to_owned(),
        )
    })?;
    let resolved = CredentialStore::new(control_plane_root)
        .global_values_for_bindings(&bindings)
        .map_err(|error| ArtSettingsError::ParameterBinding(error.to_string()))?;
    let definitions = art_parameter_definitions(tool)
        .into_iter()
        .map(|parameter| (parameter.id.clone(), parameter))
        .collect::<BTreeMap<_, _>>();
    let mut arguments = arguments.as_object().cloned().ok_or_else(|| {
        ArtSettingsError::ParameterBinding("Art arguments must be a JSON object".to_owned())
    })?;
    let disabled = arguments
        .get("disabledParams")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let wrapped = arguments.contains_key("params") || arguments.contains_key("inputs");
    let target = if wrapped {
        let params = arguments
            .entry("params".to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        params.as_object_mut().ok_or_else(|| {
            ArtSettingsError::ParameterBinding("Art params must be a JSON object".to_owned())
        })?
    } else {
        &mut arguments
    };
    for (id, resolved_value) in resolved {
        if target.contains_key(&id) || disabled.contains(&id) {
            continue;
        }
        let parameter = definitions.get(&id).ok_or_else(|| {
            ArtSettingsError::ParameterBinding(format!("Art does not define parameter `{id}`"))
        })?;
        if parameter.secret {
            return Err(ArtSettingsError::ParameterBinding(format!(
                "secret parameter `{id}` must use credentialBindings"
            )));
        }
        if !credential_value_type_matches_parameter(parameter, resolved_value.value_type) {
            return Err(ArtSettingsError::ParameterBinding(format!(
                "global value `{}` has type `{:?}`, which does not match `{}`",
                bindings.get(&id).map(String::as_str).unwrap_or_default(),
                resolved_value.value_type,
                parameter.parameter_type
            )));
        }
        validate_parameter_value(parameter, &resolved_value.value)
            .map_err(ArtSettingsError::ParameterBinding)?;
        target.insert(id, resolved_value.value);
    }
    Ok(Value::Object(arguments))
}

pub(crate) fn control_plane_root_for_tool(tool: &ToolDefinition) -> Option<PathBuf> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artPackage"))
        .and_then(|package| package.get("dir"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .and_then(|art_dir| {
            art_dir.ancestors().find_map(|ancestor| {
                ancestor
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("arts"))
                    .then(|| ancestor.parent().map(Path::to_path_buf))
                    .flatten()
            })
        })
        .or_else(|| std::env::var_os("LOOM_CONTROL_PLANE_ROOT").map(PathBuf::from))
}
