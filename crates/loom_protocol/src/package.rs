//! Framework package identity, permissions, resources, and authoring metadata.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ART_EXECUTION_REQUEST_SCHEMA, ART_EXECUTION_RESPONSE_SCHEMA, FRAMEWORK_AUTHORING_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherIdentity {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSignature {
    pub algorithm: String,
    pub key_id: String,
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSignatureDocument {
    #[serde(default = "default_signature_schema_version")]
    pub schema_version: u32,
    pub algorithm: String,
    pub key_id: String,
    pub digest_algorithm: String,
    pub digest: String,
    pub signature: String,
    pub public_key: String,
}

const fn default_signature_schema_version() -> u32 {
    1
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageTrustStatus {
    Trusted,
    Verified,
    #[default]
    Unsigned,
    Invalid,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherTrustRecord {
    pub publisher_id: String,
    pub key_id: String,
    pub public_key: String,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCompatibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkRuntimeEntry {
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub process_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkArtExecutionContract {
    pub request_schema: String,
    pub response_schema: String,
}

impl Default for FrameworkArtExecutionContract {
    fn default() -> Self {
        Self {
            request_schema: ART_EXECUTION_REQUEST_SCHEMA.to_owned(),
            response_schema: ART_EXECUTION_RESPONSE_SCHEMA.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPermission {
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub allow_localhost: bool,
    #[serde(default)]
    pub allow_private_networks: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemPermission {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPermission {
    #[serde(default)]
    pub spawn: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_processes: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPolicy {
    #[serde(default)]
    pub network: NetworkPermission,
    #[serde(default)]
    pub filesystem: FilesystemPermission,
    #[serde(default)]
    pub process: ProcessPermission,
    #[serde(default)]
    pub gpu: bool,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub credentials: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(rename = "memoryMiB", default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_processes: Option<u32>,
    #[serde(rename = "stdoutMiB", default, skip_serializing_if = "Option::is_none")]
    pub stdout_mib: Option<u64>,
    #[serde(rename = "stderrMiB", default, skip_serializing_if = "Option::is_none")]
    pub stderr_mib: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    #[serde(default = "default_health_command")]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_health_timeout")]
    pub timeout_seconds: u64,
}

fn default_health_command() -> String {
    "self_test".to_owned()
}

const fn default_health_timeout() -> u64 {
    15
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoringOption {
    pub value: Value,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoringField {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default)]
    pub options: Vec<AuthoringOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoringPort {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub port_type: String,
    pub execution_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub expose_port: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkAuthoringSchema {
    #[serde(default = "default_authoring_schema_version")]
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<AuthoringField>,
    #[serde(default)]
    pub inputs: Vec<AuthoringPort>,
    #[serde(default)]
    pub outputs: Vec<AuthoringPort>,
}

const fn default_authoring_schema_version() -> u32 {
    FRAMEWORK_AUTHORING_SCHEMA_VERSION
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDependency {
    pub kind: String,
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkPackageManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub protocol_version: String,
    #[serde(default)]
    pub supported_protocol_versions: Vec<String>,
    pub platforms: Vec<String>,
    pub entry: FrameworkRuntimeEntry,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub permission_policy: PermissionPolicy,
    #[serde(default)]
    pub resources: ResourceLimits,
    pub publisher: PublisherIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignature>,
    #[serde(default)]
    pub host_compatibility: HostCompatibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_schema: Option<FrameworkAuthoringSchema>,
    #[serde(default)]
    pub dependencies: Vec<PackageDependency>,
    pub art_execution: FrameworkArtExecutionContract,
}

impl FrameworkPackageManifest {
    pub fn advertised_protocol_versions(&self) -> Vec<&str> {
        let capacity = self.supported_protocol_versions.len().saturating_add(1);
        let mut versions = Vec::with_capacity(capacity);
        let mut seen = HashSet::with_capacity(capacity);
        for version in std::iter::once(self.protocol_version.as_str())
            .chain(self.supported_protocol_versions.iter().map(String::as_str))
        {
            if seen.insert(version) {
                versions.push(version);
            }
        }
        versions
    }

    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.publisher.id.trim(), self.id)
    }
}
