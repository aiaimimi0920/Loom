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
    #[serde(default)]
    pub runtime_failures: CapabilityRuntimeFailureState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_digest: Option<String>,
    #[serde(default)]
    pub requested_permissions: Vec<String>,
    #[serde(default)]
    pub versions: Vec<CapabilityInstalledVersion>,
}

impl CapabilityPluginRecord {
    /// Returns the digest of the newest installed version by semantic version order.
    ///
    /// Records store their versions sorted by the version *string*, so the last element is the
    /// lexicographically largest one rather than the newest: `"1.9.0"` sorts above `"1.10.0"`.
    /// Callers that want "the newest version" have to compare parsed semver, and non-semver
    /// versions are skipped because they cannot be ordered against the rest.
    #[must_use]
    pub fn latest_semver_digest(&self) -> Option<&str> {
        self.versions
            .iter()
            .filter_map(|candidate| {
                semver::Version::parse(&candidate.version)
                    .ok()
                    .map(|version| (version, candidate.digest.as_str()))
            })
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, digest)| digest)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityRuntimeFailureState {
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_started_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_not_before_ms: Option<u64>,
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
