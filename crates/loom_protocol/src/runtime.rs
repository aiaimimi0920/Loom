//! Art runtime and resolved dependency lockfile contracts.

use serde::{Deserialize, Serialize};

use crate::PLUGIN_LOCKFILE_SCHEMA_VERSION;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtRuntimeManifest {
    pub protocol_version: String,
    pub entry: ArtRuntimeEntry,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtRuntimeEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLockfile {
    #[serde(default = "default_lockfile_schema_version")]
    pub schema_version: u32,
    pub package_id: String,
    pub package_version: String,
    #[serde(default)]
    pub resolved: Vec<ResolvedDependency>,
}

const fn default_lockfile_schema_version() -> u32 {
    PLUGIN_LOCKFILE_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDependency {
    pub kind: String,
    pub id: String,
    pub version: String,
    pub sha256: String,
}
