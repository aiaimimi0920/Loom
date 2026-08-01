//! Stable, language-neutral contracts for independently packaged Loom plugins.
//!
//! Keep v1 field names and wire semantics backwards compatible. New protocol
//! behavior must be negotiated explicitly instead of changing these envelopes.

use std::path::PathBuf;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const FRAMEWORK_PROTOCOL_VERSION: &str = "loom.framework.v1";
pub const ART_EXECUTION_REQUEST_SCHEMA: &str = "loom.art.execute.v1";
pub const ART_EXECUTION_RESPONSE_SCHEMA: &str = "loom.art.result.v1";
pub const ART_RUNTIME_PROTOCOL_VERSION: &str = "loom.art.runtime.v1";
pub const FRAMEWORK_AUTHORING_SCHEMA_VERSION: u32 = 1;
pub const PLUGIN_LOCKFILE_SCHEMA_VERSION: u32 = 1;

pub mod schemas {
    pub const FRAMEWORK_MANIFEST_V1: &str =
        include_str!("../../../protocol/schemas/framework-manifest.v1.schema.json");
    pub const FRAMEWORK_EXECUTE_REQUEST_V1: &str =
        include_str!("../../../protocol/schemas/framework-execute-request.v1.schema.json");
    pub const FRAMEWORK_EXECUTE_RESPONSE_V1: &str =
        include_str!("../../../protocol/schemas/framework-execute-response.v1.schema.json");
    pub const FRAMEWORK_AUTHORING_V1: &str =
        include_str!("../../../protocol/schemas/framework-authoring.v1.schema.json");
    pub const ART_RUNTIME_V1: &str =
        include_str!("../../../protocol/schemas/art-runtime.v1.schema.json");
}

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
    #[serde(default = "default_process_model")]
    pub process_model: String,
}

fn default_process_model() -> String {
    "per_execution".to_owned()
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_processes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<PublisherIdentity>,
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
        let mut versions = vec![self.protocol_version.as_str()];
        for version in &self.supported_protocol_versions {
            if !versions.contains(&version.as_str()) {
                versions.push(version.as_str());
            }
        }
        versions
    }

    pub fn qualified_id(&self) -> String {
        match self.publisher.as_ref().map(|publisher| publisher.id.trim()) {
            Some(publisher) if !publisher.is_empty() => format!("{publisher}/{}", self.id),
            _ => self.id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteRequest {
    pub protocol_version: String,
    #[serde(default)]
    pub supported_protocol_versions: Vec<String>,
    pub framework_id: String,
    pub art_id: String,
    pub art_dir: PathBuf,
    pub inputs: Value,
    pub params: Value,
    pub disabled_params: Vec<String>,
    pub context: FrameworkExecutionContext,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionContext {
    pub request_id: String,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_version: Option<String>,
    #[serde(default)]
    pub granted_permissions: PermissionPolicy,
    #[serde(default)]
    pub credentials: Vec<CredentialGrant>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialGrant {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub error: Option<FrameworkExecuteError>,
    #[serde(default)]
    pub candidates: Vec<Value>,
    #[serde(default)]
    pub cache: Value,
    #[serde(default)]
    pub events: Vec<FrameworkExecutionEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ExecutionDiagnostics>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecuteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkExecutionEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDiagnostics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout_bytes: u64,
    #[serde(default)]
    pub stderr_bytes: u64,
    #[serde(default)]
    pub stdout_truncated: bool,
    #[serde(default)]
    pub stderr_truncated: bool,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub resource_limited: bool,
}

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolValidationError {
    #[error("package id is not safe: {0}")]
    UnsafePackageId(String),
    #[error("publisher id is not safe: {0}")]
    UnsafePublisherId(String),
    #[error("invalid semantic version `{value}`: {reason}")]
    InvalidVersion { value: String, reason: String },
    #[error("invalid host compatibility requirement `{value}`: {reason}")]
    InvalidCompatibility { value: String, reason: String },
    #[error("unsupported protocol; package advertises {advertised:?}")]
    UnsupportedProtocol { advertised: Vec<String> },
    #[error("unsupported Art execution schema")]
    UnsupportedArtExecutionSchema,
    #[error("authoring schema version {0} is not supported")]
    UnsupportedAuthoringSchema(u32),
    #[error("authoring field id is not safe: {0}")]
    UnsafeAuthoringField(String),
    #[error("authoring port name is not safe: {0}")]
    UnsafeAuthoringPort(String),
}

pub fn is_safe_package_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn is_safe_publisher_id(value: &str) -> bool {
    is_safe_package_id(value) && !value.starts_with('.') && !value.ends_with('.')
}

pub fn validate_framework_manifest_contract(
    manifest: &FrameworkPackageManifest,
) -> Result<(), ProtocolValidationError> {
    if !is_safe_package_id(&manifest.id) {
        return Err(ProtocolValidationError::UnsafePackageId(
            manifest.id.clone(),
        ));
    }
    if let Some(publisher) = &manifest.publisher {
        if !is_safe_publisher_id(&publisher.id) {
            return Err(ProtocolValidationError::UnsafePublisherId(
                publisher.id.clone(),
            ));
        }
    }
    Version::parse(manifest.version.trim()).map_err(|error| {
        ProtocolValidationError::InvalidVersion {
            value: manifest.version.clone(),
            reason: error.to_string(),
        }
    })?;
    validate_host_compatibility(&manifest.host_compatibility)?;
    negotiate_framework_protocol(manifest)?;
    if manifest.art_execution.request_schema != ART_EXECUTION_REQUEST_SCHEMA
        || manifest.art_execution.response_schema != ART_EXECUTION_RESPONSE_SCHEMA
    {
        return Err(ProtocolValidationError::UnsupportedArtExecutionSchema);
    }
    if let Some(authoring) = &manifest.authoring_schema {
        validate_authoring_schema(authoring)?;
    }
    Ok(())
}

pub fn negotiate_framework_protocol(
    manifest: &FrameworkPackageManifest,
) -> Result<&'static str, ProtocolValidationError> {
    if manifest
        .advertised_protocol_versions()
        .into_iter()
        .any(|version| version == FRAMEWORK_PROTOCOL_VERSION)
    {
        Ok(FRAMEWORK_PROTOCOL_VERSION)
    } else {
        Err(ProtocolValidationError::UnsupportedProtocol {
            advertised: manifest
                .advertised_protocol_versions()
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
        })
    }
}

pub fn validate_host_compatibility(
    compatibility: &HostCompatibility,
) -> Result<(), ProtocolValidationError> {
    for requirement in [&compatibility.minimum, &compatibility.maximum]
        .into_iter()
        .flatten()
    {
        VersionReq::parse(requirement).map_err(|error| {
            ProtocolValidationError::InvalidCompatibility {
                value: requirement.clone(),
                reason: error.to_string(),
            }
        })?;
    }
    Ok(())
}

pub fn validate_authoring_schema(
    schema: &FrameworkAuthoringSchema,
) -> Result<(), ProtocolValidationError> {
    if schema.schema_version != FRAMEWORK_AUTHORING_SCHEMA_VERSION {
        return Err(ProtocolValidationError::UnsupportedAuthoringSchema(
            schema.schema_version,
        ));
    }
    for field in &schema.fields {
        if !is_safe_package_id(&field.id) {
            return Err(ProtocolValidationError::UnsafeAuthoringField(
                field.id.clone(),
            ));
        }
    }
    for port in schema.inputs.iter().chain(&schema.outputs) {
        if !is_safe_package_id(&port.name) {
            return Err(ProtocolValidationError::UnsafeAuthoringPort(
                port.name.clone(),
            ));
        }
    }
    Ok(())
}

pub fn response_status_is_success(status: &str) -> bool {
    matches!(status, "success" | "ok" | "completed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> FrameworkPackageManifest {
        FrameworkPackageManifest {
            id: "example-framework".to_owned(),
            name: "Example".to_owned(),
            description: "Example framework".to_owned(),
            version: "1.2.3".to_owned(),
            protocol_version: FRAMEWORK_PROTOCOL_VERSION.to_owned(),
            supported_protocol_versions: Vec::new(),
            platforms: vec!["windows-x64".to_owned()],
            entry: FrameworkRuntimeEntry {
                kind: "process".to_owned(),
                command: "runtime/framework.exe".to_owned(),
                args: Vec::new(),
                process_model: default_process_model(),
            },
            permissions: Vec::new(),
            permission_policy: PermissionPolicy::default(),
            resources: ResourceLimits::default(),
            publisher: Some(PublisherIdentity {
                id: "example.vendor".to_owned(),
                ..PublisherIdentity::default()
            }),
            signature: None,
            host_compatibility: HostCompatibility {
                minimum: Some(">=0.1.0".to_owned()),
                maximum: None,
            },
            health_check: None,
            authoring_schema: None,
            dependencies: Vec::new(),
            art_execution: FrameworkArtExecutionContract::default(),
        }
    }

    #[test]
    fn v1_manifest_contract_is_accepted() {
        assert_eq!(validate_framework_manifest_contract(&manifest()), Ok(()));
    }

    #[test]
    fn supported_protocol_versions_can_negotiate_v1() {
        let mut manifest = manifest();
        manifest.protocol_version = "loom.framework.v2".to_owned();
        manifest.supported_protocol_versions = vec![FRAMEWORK_PROTOCOL_VERSION.to_owned()];
        assert_eq!(
            negotiate_framework_protocol(&manifest),
            Ok(FRAMEWORK_PROTOCOL_VERSION)
        );
    }

    #[test]
    fn invalid_publisher_and_semver_are_rejected() {
        let mut invalid = manifest();
        invalid.publisher.as_mut().expect("publisher").id = "../vendor".to_owned();
        assert!(matches!(
            validate_framework_manifest_contract(&invalid),
            Err(ProtocolValidationError::UnsafePublisherId(_))
        ));

        let mut invalid = manifest();
        invalid.version = "latest".to_owned();
        assert!(matches!(
            validate_framework_manifest_contract(&invalid),
            Err(ProtocolValidationError::InvalidVersion { .. })
        ));
    }

    #[test]
    fn old_manifest_shape_deserializes_with_secure_defaults() {
        let parsed: FrameworkPackageManifest = serde_json::from_value(serde_json::json!({
            "id": "script",
            "name": "Script",
            "description": "Script framework",
            "version": "0.1.0",
            "protocolVersion": "loom.framework.v1",
            "platforms": ["windows-x64"],
            "entry": {
                "kind": "process",
                "command": "runtime/script.exe",
                "args": []
            },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        }))
        .expect("legacy manifest");
        assert_eq!(parsed.entry.process_model, "per_execution");
        assert!(!parsed.permission_policy.process.spawn);
        assert!(parsed.publisher.is_none());
    }
}
