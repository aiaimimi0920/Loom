use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPackageManifest {
    pub schema_version: u32,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub publisher: CapabilityPublisher,
    pub host_compatibility: CapabilityHostCompatibility,
    pub entrypoints: CapabilityEntrypoints,
    #[serde(default)]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub contributes: CapabilityContributions,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub resources: CapabilityResourceLimits,
    #[serde(default)]
    pub dependencies: Vec<CapabilityDependency>,
    pub signature: CapabilitySignature,
}

impl CapabilityPackageManifest {
    #[must_use]
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.publisher.id, self.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPublisher {
    pub id: String,
    pub key_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityApiRequirement {
    pub minimum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<String>,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub optional_features: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityHostCompatibility {
    pub loom_capability_api: CapabilityApiRequirement,
    pub hook_extension_api: CapabilityApiRequirement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_api: Option<CapabilityApiRequirement>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityEntrypoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<CapabilityServiceEntrypoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_ui: Option<CapabilitySurfaceEntrypoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityServiceEntrypoint {
    pub targets: BTreeMap<String, CapabilityTargetCommand>,
    pub process_model: CapabilityProcessModel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityProcessModel {
    OnDemand,
    Persistent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityTargetCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilitySurfaceEntrypoint {
    pub kind: String,
    pub manifest: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityContributions {
    #[serde(default)]
    pub commands: Vec<CapabilityCommandContribution>,
    #[serde(default)]
    pub shortcuts: Vec<CapabilityContribution>,
    #[serde(default)]
    pub menus: Vec<CapabilityContribution>,
    #[serde(default)]
    pub settings: Vec<CapabilityContribution>,
    #[serde(default)]
    pub data_types: Vec<CapabilityContribution>,
    #[serde(default)]
    pub renderers: Vec<CapabilityContribution>,
    #[serde(default)]
    pub unit_overlays: Vec<CapabilityContribution>,
    #[serde(default)]
    pub background_tasks: Vec<CapabilityContribution>,
    #[serde(default)]
    pub resource_providers: Vec<CapabilityContribution>,
    #[serde(default)]
    pub diagnostics: Vec<CapabilityContribution>,
    #[serde(default)]
    pub event_subscriptions: Vec<CapabilityContribution>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default)]
    pub requires_user_gesture: bool,
    #[serde(default = "default_true")]
    pub cancellable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityContribution {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityResourceLimits {
    #[serde(rename = "memoryMiB")]
    pub memory_mib: u64,
    pub max_processes: u32,
    pub timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_kib_per_minute: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDependency {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilitySignature {
    pub algorithm: String,
    pub key_id: String,
    pub file: String,
}
