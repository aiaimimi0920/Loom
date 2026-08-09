use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use loom_protocol::is_safe_package_id;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::credentials::{CredentialStore, CredentialValueType};
use crate::ToolDefinition;

const ART_SETTINGS_FILE: &str = "art-user-settings.json";
const ART_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtUpdateSource {
    pub store: String,
    pub art_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtUserSettings {
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub value_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub credential_bindings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ArtUpdateSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Default for ArtUserSettings {
    fn default() -> Self {
        Self {
            auto_update: true,
            defaults: BTreeMap::new(),
            value_bindings: BTreeMap::new(),
            credential_bindings: BTreeMap::new(),
            source: None,
            name: None,
            description: None,
        }
    }
}

const fn default_auto_update() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtSettingsFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    arts: BTreeMap<String, ArtUserSettings>,
}

impl Default for ArtSettingsFile {
    fn default() -> Self {
        Self {
            schema_version: ART_SETTINGS_SCHEMA_VERSION,
            arts: BTreeMap::new(),
        }
    }
}

const fn default_schema_version() -> u32 {
    ART_SETTINGS_SCHEMA_VERSION
}

#[derive(Debug, thiserror::Error)]
pub enum ArtSettingsError {
    #[error("invalid Art identity `{0}`")]
    InvalidArtId(String),
    #[error("invalid Art setting key `{0}`")]
    InvalidSettingKey(String),
    #[error("invalid credential name `{0}`")]
    InvalidCredentialName(String),
    #[error("invalid Art update source: {0}")]
    InvalidSource(String),
    #[error("Art parameter binding failed: {0}")]
    ParameterBinding(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct ArtSettingsStore {
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtParameterDefinition {
    pub id: String,
    pub label: String,
    pub parameter_type: String,
    pub required: bool,
    pub secret: bool,
    pub default: Option<Value>,
    pub options: Option<Value>,
    pub minimum: Option<Value>,
    pub maximum: Option<Value>,
    pub step: Option<Value>,
}

impl ArtSettingsStore {
    #[must_use]
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root.as_ref().join(ART_SETTINGS_FILE),
        }
    }

    pub fn get(&self, art_id: &str) -> Result<ArtUserSettings, ArtSettingsError> {
        validate_art_reference(art_id)?;
        Ok(self.read_file()?.arts.remove(art_id).unwrap_or_default())
    }

    pub fn list(&self) -> Result<BTreeMap<String, ArtUserSettings>, ArtSettingsError> {
        Ok(self.read_file()?.arts)
    }

    pub fn save(
        &self,
        art_id: &str,
        settings: ArtUserSettings,
    ) -> Result<ArtUserSettings, ArtSettingsError> {
        validate_art_reference(art_id)?;
        validate_settings(&settings)?;
        let mut file = self.read_file()?;
        file.arts.insert(art_id.to_owned(), settings.clone());
        self.write_file(&file)?;
        Ok(settings)
    }

    pub fn delete(&self, art_id: &str) -> Result<bool, ArtSettingsError> {
        validate_art_reference(art_id)?;
        let mut file = self.read_file()?;
        let deleted = file.arts.remove(art_id).is_some();
        if deleted {
            self.write_file(&file)?;
        }
        Ok(deleted)
    }

    fn read_file(&self) -> Result<ArtSettingsFile, ArtSettingsError> {
        if !self.path.exists() {
            return Ok(ArtSettingsFile::default());
        }
        Ok(serde_json::from_slice(&fs::read(&self.path)?)?)
    }

    fn write_file(&self, file: &ArtSettingsFile) -> Result<(), ArtSettingsError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let mut output = fs::File::create(&temporary)?;
        let mut bytes = serde_json::to_vec_pretty(file)?;
        bytes.push(b'\n');
        output.write_all(&bytes)?;
        output.sync_all()?;
        drop(output);
        crate::replace_registry_file(&temporary, &self.path)?;
        Ok(())
    }
}

#[must_use]
pub fn art_is_locally_authored(tool: &ToolDefinition) -> bool {
    tool.publisher_identity().is_none()
        && tool
            .metadata
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

    let metadata = tool
        .metadata
        .get_or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let metadata = metadata.as_object_mut().expect("metadata normalized");
    metadata.insert(
        "artUserSettings".to_owned(),
        serde_json::json!({
            "autoUpdate": settings.auto_update,
            "defaults": settings.defaults,
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
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(compat_defaults) = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artloomCompat"))
        .and_then(|compat| compat.get("defaults"))
        .and_then(Value::as_object)
    {
        defaults.extend(compat_defaults.clone());
    }
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
            .or_else(|| explicit.get("disabled_params"))
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
        .or_else(|| arguments.get("disabled_params"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
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

#[must_use]
pub fn art_parameter_definitions(tool: &ToolDefinition) -> Vec<ArtParameterDefinition> {
    tool.params
        .iter()
        .filter_map(|parameter| {
            let parameter = parameter.as_object()?;
            let id = parameter
                .get("id")
                .or_else(|| parameter.get("name"))
                .or_else(|| parameter.get("key"))
                .and_then(Value::as_str)?
                .trim();
            if id.is_empty() {
                return None;
            }
            let widget = parameter.get("widget").and_then(Value::as_str);
            let parameter_type = parameter
                .get("type")
                .or_else(|| parameter.get("dataType"))
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
                    .or_else(|| parameter.get("title"))
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

fn tool_value_bindings(tool: &ToolDefinition) -> BTreeMap<String, String> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.get("artUserSettings"))
        .and_then(|settings| settings.get("valueBindings"))
        .and_then(|bindings| serde_json::from_value(bindings.clone()).ok())
        .unwrap_or_default()
}

fn control_plane_root_for_tool(tool: &ToolDefinition) -> Option<PathBuf> {
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

fn manifest_parameter_defaults(tool: &ToolDefinition) -> Map<String, Value> {
    let mut defaults = Map::new();
    for parameter in &tool.params {
        let Some(parameter) = parameter.as_object() else {
            continue;
        };
        let Some(id) = parameter
            .get("id")
            .or_else(|| parameter.get("name"))
            .or_else(|| parameter.get("key"))
            .and_then(Value::as_str)
        else {
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

fn secret_parameter_ids(tool: &ToolDefinition) -> std::collections::BTreeSet<String> {
    tool.params
        .iter()
        .filter_map(Value::as_object)
        .filter(|parameter| parameter_is_secret(parameter))
        .filter_map(|parameter| {
            parameter
                .get("id")
                .or_else(|| parameter.get("name"))
                .or_else(|| parameter.get("key"))
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
            .or_else(|| parameter.get("dataType"))
            .or_else(|| parameter.get("data_type"))
            .and_then(Value::as_str)
            == Some("secret")
}

fn validate_art_reference(value: &str) -> Result<(), ArtSettingsError> {
    let valid = value
        .split_once('/')
        .map(|(publisher, id)| is_safe_package_id(publisher) && is_safe_package_id(id))
        .unwrap_or_else(|| is_safe_package_id(value));
    if valid {
        Ok(())
    } else {
        Err(ArtSettingsError::InvalidArtId(value.to_owned()))
    }
}

fn validate_settings(settings: &ArtUserSettings) -> Result<(), ArtSettingsError> {
    for key in settings
        .defaults
        .keys()
        .chain(settings.value_bindings.keys())
        .chain(settings.credential_bindings.keys())
    {
        if !is_safe_package_id(key) {
            return Err(ArtSettingsError::InvalidSettingKey(key.clone()));
        }
    }
    for credential in settings.value_bindings.values() {
        if !is_safe_package_id(credential) {
            return Err(ArtSettingsError::InvalidCredentialName(credential.clone()));
        }
    }
    for credential in settings.credential_bindings.values() {
        if !is_safe_package_id(credential) {
            return Err(ArtSettingsError::InvalidCredentialName(credential.clone()));
        }
    }
    if let Some(source) = &settings.source {
        if source.store.trim().is_empty() || !is_safe_package_id(&source.art_id) {
            return Err(ArtSettingsError::InvalidSource(source.store.clone()));
        }
        if let Some(identity) = &source.qualified_id {
            validate_art_reference(identity)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolExecution;

    fn temp_root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-art-settings-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn settings_default_to_auto_update_and_roundtrip_atomically() {
        let root = temp_root();
        let store = ArtSettingsStore::new(&root);
        assert!(store.get("sample").unwrap().auto_update);
        let settings = ArtUserSettings {
            auto_update: false,
            defaults: BTreeMap::from([("strength".to_owned(), serde_json::json!(0.8))]),
            value_bindings: BTreeMap::from([("quality".to_owned(), "image_quality".to_owned())]),
            credential_bindings: BTreeMap::from([(
                "cloudflare".to_owned(),
                "cloudflare_key".to_owned(),
            )]),
            ..ArtUserSettings::default()
        };
        store.save("sample", settings.clone()).unwrap();
        assert_eq!(store.get("sample").unwrap(), settings);
        assert!(!root.join("art-user-settings.json.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_authorship_requires_authoring_metadata_and_no_publisher() {
        let mut tool = ToolDefinition::new(
            "local",
            "Local",
            "",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        assert!(!art_is_locally_authored(&tool));
        tool.metadata = Some(serde_json::json!({ "authoring": { "origin": "local" } }));
        assert!(art_is_locally_authored(&tool));
        tool.metadata = Some(serde_json::json!({
            "authoring": { "origin": "local" },
            "packageSecurity": { "publisher": { "id": "neuro.official", "name": "Neuro" } }
        }));
        assert!(!art_is_locally_authored(&tool));
    }

    #[test]
    fn explicit_parameters_override_saved_and_manifest_defaults() {
        let mut tool = ToolDefinition::new(
            "sample",
            "Sample",
            "",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        tool.params = vec![
            serde_json::json!({ "id": "strength", "default": 0.2 }),
            serde_json::json!({ "id": "api_token", "type": "secret", "default": "must-not-merge" }),
        ];
        tool.metadata = Some(serde_json::json!({
            "artUserSettings": {
                "defaults": { "strength": 0.6, "quality": 90, "api_token": "must-not-merge" },
                "valueBindings": { "quality": "global_quality" }
            }
        }));
        let merged = merge_tool_arguments(
            &tool,
            serde_json::json!({ "inputs": { "image": "x" }, "params": { "strength": 0.9 } }),
        );
        assert_eq!(merged["params"]["strength"], 0.9);
        assert!(merged["params"].get("quality").is_none());
        assert!(merged["params"].get("api_token").is_none());
    }

    #[test]
    fn global_value_bindings_resolve_typed_values_and_explicit_params_win() {
        let root = temp_root();
        let art_dir = root
            .join("arts")
            .join("neuro.official")
            .join("sample")
            .join("versions")
            .join("1.0.0");
        fs::create_dir_all(&art_dir).unwrap();
        CredentialStore::new(&root)
            .upsert(crate::credentials::CredentialInput {
                name: "image_count".to_owned(),
                value: "3".to_owned(),
                value_type: CredentialValueType::Integer,
                scope: crate::credentials::CredentialScope::default(),
                expires_at: None,
            })
            .unwrap();
        let mut tool = ToolDefinition::new(
            "sample",
            "Sample",
            "",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        tool.params = vec![serde_json::json!({
            "id": "count",
            "type": "number",
            "minimum": 1,
            "maximum": 5,
            "default": 1
        })];
        tool.metadata = Some(serde_json::json!({
            "artPackage": { "dir": art_dir },
            "artUserSettings": {
                "defaults": { "count": 2 },
                "valueBindings": { "count": "image_count" }
            }
        }));
        let prepared = resolve_tool_value_bindings(
            &tool,
            merge_tool_arguments(&tool, serde_json::json!({ "params": {} })),
        )
        .unwrap();
        assert_eq!(prepared["params"]["count"], 3);
        let explicit = resolve_tool_value_bindings(
            &tool,
            merge_tool_arguments(&tool, serde_json::json!({ "params": { "count": 4 } })),
        )
        .unwrap();
        assert_eq!(explicit["params"]["count"], 4);
        let disabled = resolve_tool_value_bindings(
            &tool,
            merge_tool_arguments(
                &tool,
                serde_json::json!({ "params": {}, "disabledParams": ["count"] }),
            ),
        )
        .unwrap();
        assert!(disabled["params"].get("count").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn global_value_bindings_reject_missing_or_mismatched_values() {
        let root = temp_root();
        let art_dir = root
            .join("arts")
            .join("sample")
            .join("versions")
            .join("1.0.0");
        fs::create_dir_all(&art_dir).unwrap();
        CredentialStore::new(&root)
            .upsert(crate::credentials::CredentialInput {
                name: "label".to_owned(),
                value: "three".to_owned(),
                value_type: CredentialValueType::String,
                scope: crate::credentials::CredentialScope::default(),
                expires_at: None,
            })
            .unwrap();
        let mut tool = ToolDefinition::new(
            "sample",
            "Sample",
            "",
            ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
        );
        tool.params = vec![serde_json::json!({ "id": "count", "type": "integer" })];
        tool.metadata = Some(serde_json::json!({
            "artPackage": { "dir": art_dir },
            "artUserSettings": { "valueBindings": { "count": "label" } }
        }));
        assert!(matches!(
            resolve_tool_value_bindings(&tool, serde_json::json!({})),
            Err(ArtSettingsError::ParameterBinding(_))
        ));
        tool.metadata.as_mut().unwrap()["artUserSettings"]["valueBindings"]["count"] =
            serde_json::json!("missing");
        assert!(matches!(
            resolve_tool_value_bindings(&tool, serde_json::json!({})),
            Err(ArtSettingsError::ParameterBinding(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
