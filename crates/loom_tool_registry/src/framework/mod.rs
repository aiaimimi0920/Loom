//! Art execution frameworks treated as first-class, installable capabilities.
//! The facade preserves the original public API while lifecycle, storage,
//! catalog, readiness, and package-runtime concerns live in focused modules.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use loom_plugin_security::{
    canonical_package_digest, verify_package_signature, PluginSecurityError, TrustPolicy,
    TrustStore,
};
use loom_process::ProcessSpec;
use loom_protocol::{
    response_status_is_success, FrameworkExecuteResponse, PackageTrustStatus, PublisherTrustRecord,
};

use crate::{ToolDefinition, ToolExecution};

mod catalog;
mod dependencies;
mod model;
mod package_runtime;
mod permissions;
mod readiness;
mod registry_core;
mod registry_mutation;
mod storage;

pub use loom_protocol::{
    FrameworkArtExecutionContract, FrameworkAuthoringSchema, FrameworkPackageManifest,
    FrameworkRuntimeEntry, HealthCheck, HostCompatibility, PackageDependency, PackageSignature,
    PermissionPolicy, PublisherIdentity, ResourceLimits, FRAMEWORK_PROTOCOL_VERSION,
};

pub use model::{
    framework_id_for_execution, read_dependencies, ArtBinary, ArtDependencies,
    ArtMcpServerDependency, FrameworkStatus,
};
pub use package_runtime::FrameworkError;
pub use permissions::{
    enforce_framework_permission_policy, permission_enforcement_matrix, plugin_permission_mode,
    unsupported_permission_findings, unsupported_permission_findings_for, PluginPermissionMode,
};
pub use readiness::{
    framework_needs_runtime, framework_ready, framework_ready_in, resolve_framework_package_dir,
};
pub use registry_core::FrameworkRegistry;

pub(crate) use package_runtime::{is_valid_framework, is_valid_framework_reference};

use catalog::*;
use dependencies::*;
use model::{FrameworkActivationState, FrameworkInstallationState, FrameworkLifecycleJournal};
use package_runtime::{run_framework_self_test, unpack_runtime_zip};
use readiness::{
    framework_description, framework_local_id, framework_name, framework_storage_path,
    read_framework_manifest, validate_framework_manifest,
};
use storage::*;

#[cfg(test)]
use permissions::{enforce_framework_permission_mode, parse_plugin_permission_mode};

const FRAMEWORKS_FILE: &str = "frameworks.json";
const FRAMEWORK_MANIFEST_FILE: &str = "framework.manifest.json";
const PLUGIN_TRUST_STORE_FILE: &str = "plugin-trust.json";
const FRAMEWORK_ACTIVE_FILE: &str = "active.json";
const FRAMEWORK_VERSIONS_DIR: &str = "versions";
const FRAMEWORK_LIFECYCLE_FILE: &str = "lifecycle.json";
const FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX: &str = ".loom-delete-framework-";
const WINDOWS_X64_PLATFORM: &str = "windows-x64";
const FRAMEWORK_PACKAGE_CATALOG_ENV: &str = "LOOM_FRAMEWORK_PACKAGE_CATALOG_DIR";
const FRAMEWORK_PACKAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const FRAMEWORK_METADATA_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Subdir under the control-plane root holding installed framework packages:
/// `<control-plane>/frameworks/<id>/`.
const FRAMEWORK_PACKAGES_DIR: &str = "frameworks";

/// The four repo-owned framework package IDs. This is a catalog, not a closed
/// allowlist; third-party packages may use any ID accepted by
/// `is_valid_framework`.
pub const FRAMEWORK_IDS: [&str; 4] = ["process", "cloud_api", "mcp", "workflow"];

pub(crate) fn read_bounded_framework_metadata(path: &Path) -> std::io::Result<Vec<u8>> {
    storage::read_bounded_file(path, FRAMEWORK_METADATA_MAX_BYTES)
}

#[cfg(test)]
mod tests;
