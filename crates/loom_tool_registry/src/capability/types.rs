use std::path::PathBuf;

use loom_protocol::PackageTrustStatus;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CAPABILITY_REGISTRY_SCHEMA_VERSION: u32 = 1;
pub const CAPABILITY_REGISTRY_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLifecycleStatus {
    InstalledDisabled,
    ApprovalRequired,
    Activating,
    Active,
    Faulted,
    Disabling,
    Upgrading,
    RollingBack,
    Uninstalling,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityInstalledVersion {
    pub version: String,
    pub digest: String,
    pub relative_path: String,
    pub trust_status: PackageTrustStatus,
    pub installed_at: String,
    #[serde(default)]
    pub requested_permissions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPluginRecord {
    pub qualified_id: String,
    pub publisher_id: String,
    pub package_id: String,
    pub name: String,
    pub description: String,
    pub enabled_intent: bool,
    pub status: CapabilityLifecycleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_digest: Option<String>,
    #[serde(default)]
    pub requested_permissions: Vec<String>,
    #[serde(default)]
    pub versions: Vec<CapabilityInstalledVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapabilityRegistryDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub plugins: Vec<CapabilityPluginRecord>,
}

impl Default for CapabilityRegistryDocument {
    fn default() -> Self {
        Self {
            schema_version: CAPABILITY_REGISTRY_SCHEMA_VERSION,
            plugins: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInstallReport {
    pub qualified_id: String,
    pub version: String,
    pub digest: String,
    pub package_dir: PathBuf,
    pub trust_status: PackageTrustStatus,
    pub installed_files: Vec<String>,
}

#[derive(Debug, Error)]
pub enum CapabilityInstallError {
    #[error("capability package is invalid: {0}")]
    InvalidPackage(String),
    #[error("capability registry is invalid: {0}")]
    InvalidRegistry(String),
    #[error("capability package was not found: {0}")]
    NotFound(String),
    #[error("capability package conflicts with immutable content: {0}")]
    Conflict(String),
    #[error("capability lifecycle state is invalid: {0}")]
    InvalidState(String),
    #[error("capability permissions require approval: {0}")]
    PermissionRequired(String),
    #[error("capability I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("capability JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub type CapabilityResult<T> = Result<T, CapabilityInstallError>;
