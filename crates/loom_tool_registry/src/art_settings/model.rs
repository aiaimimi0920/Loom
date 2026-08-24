use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const ART_SETTINGS_FILE: &str = "art-user-settings.json";
pub(super) const ART_SETTINGS_SCHEMA_VERSION: u32 = 1;
pub(super) const MAX_ART_SETTINGS_FILE_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const MAX_ART_SETTINGS_DEPTH: usize = 32;
pub(super) const MAX_ART_SETTINGS_COUNT: usize = 4096;
pub(super) const MAX_ART_SETTING_ENTRIES: usize = 1024;
pub(super) const MAX_ART_SETTING_VALUE_BYTES: usize = 256 * 1024;
pub(super) const MAX_ART_SETTING_TEXT_BYTES: usize = 64 * 1024;

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
pub(super) struct ArtSettingsFile {
    #[serde(default = "default_schema_version")]
    pub(super) schema_version: u32,
    #[serde(default)]
    pub(super) arts: BTreeMap<String, ArtUserSettings>,
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
    #[error("Art settings schema version {actual} is unsupported; expected {expected}")]
    UnsupportedSchemaVersion { actual: u32, expected: u32 },
    #[error("invalid Art settings document: {0}")]
    InvalidDocument(String),
    #[error("Art settings store exceeds the {max_bytes}-byte limit")]
    StoreTooLarge { max_bytes: u64 },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
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
