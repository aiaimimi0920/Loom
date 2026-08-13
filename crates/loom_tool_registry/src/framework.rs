//! Art execution frameworks treated as first-class, installable capabilities.
//! Each Art belongs to exactly one framework and can only run when that
//! framework is installed and ready. Loom publishes four repo-owned framework
//! packages, but safe third-party framework IDs are also supported. Command,
//! script, and Python Arts share the package-backed `process` framework.
//!
//! Unified model (per product decision): all frameworks share the same
//! package-backed installed/ready state. No optional framework is compiled or
//! installed into a fresh control plane by default. A framework becomes
//! available only after its package manifest and runtime have been installed.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
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

pub use loom_protocol::{
    FrameworkArtExecutionContract, FrameworkAuthoringSchema, FrameworkPackageManifest,
    FrameworkRuntimeEntry, HealthCheck, HostCompatibility, PackageDependency, PackageSignature,
    PermissionPolicy, PublisherIdentity, ResourceLimits, FRAMEWORK_PROTOCOL_VERSION,
};

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
/// Subdir under the control-plane root holding installed framework packages:
/// `<control-plane>/frameworks/<id>/`.
const FRAMEWORK_PACKAGES_DIR: &str = "frameworks";

/// The four repo-owned framework package IDs. This is a catalog, not a closed
/// allowlist; third-party packages may use any ID accepted by
/// `is_valid_framework`.
pub const FRAMEWORK_IDS: [&str; 4] = ["process", "cloud_api", "mcp", "workflow"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkInstallationState {
    pub version: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkActivationState {
    pub active: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkLifecycleJournal {
    old_activation: Option<FrameworkActivationState>,
    next_activation: FrameworkActivationState,
    target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkStatus {
    pub id: String,
    pub qualified_id: String,
    pub name: String,
    pub description: String,
    /// Whether the user has installed/enabled this framework.
    pub installed: bool,
    /// Whether an installed framework package is enabled for execution.
    pub enabled: bool,
    /// Whether the framework's runtime is actually available (probed).
    pub ready: bool,
    pub ready_detail: String,
    /// Version read from the installed package manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Directory containing the installed package, when installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<PublisherIdentity>,
    #[serde(default)]
    pub permission_policy: PermissionPolicy,
    #[serde(default)]
    pub declared_permissions: Vec<String>,
    #[serde(default)]
    pub resources: ResourceLimits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_schema: Option<FrameworkAuthoringSchema>,
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPermissionMode {
    #[default]
    Audit,
    Strict,
}

pub fn plugin_permission_mode() -> Result<PluginPermissionMode, String> {
    parse_plugin_permission_mode(std::env::var("LOOM_PLUGIN_PERMISSION_MODE").ok().as_deref())
}

fn parse_plugin_permission_mode(value: Option<&str>) -> Result<PluginPermissionMode, String> {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "audit" => Ok(PluginPermissionMode::Audit),
        "strict" => Ok(PluginPermissionMode::Strict),
        value => Err(format!(
            "invalid LOOM_PLUGIN_PERMISSION_MODE `{value}`; expected audit or strict"
        )),
    }
}

pub fn permission_enforcement_matrix() -> BTreeMap<&'static str, &'static str> {
    let memory_and_process_count = if cfg!(windows) {
        "windows-job-enforced"
    } else {
        "declared-only"
    };
    BTreeMap::from([
        ("packageContainment", "enforced"),
        ("writableStateSeparation", "enforced"),
        ("processTree", "enforced"),
        ("timeoutAndOutput", "enforced"),
        ("memoryAndProcessCount", memory_and_process_count),
        ("credentials", "brokered"),
        ("hostHttp", "policy-enforced"),
        ("directNetwork", "not-os-enforced"),
        ("arbitraryFilesystem", "not-os-enforced"),
        ("gpu", "not-os-enforced"),
        ("clipboard", "not-os-enforced"),
    ])
}

pub fn unsupported_permission_findings(manifest: &FrameworkPackageManifest) -> Vec<String> {
    unsupported_permission_findings_for(&manifest.permissions, &manifest.permission_policy)
}

pub fn unsupported_permission_findings_for(
    permissions: &[String],
    permission_policy: &PermissionPolicy,
) -> Vec<String> {
    let declared = permissions
        .iter()
        .map(|permission| permission.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    if declared
        .iter()
        .any(|permission| permission.starts_with("network."))
        || !permission_policy.network.domains.is_empty()
        || permission_policy.network.allow_localhost
        || permission_policy.network.allow_private_networks
    {
        findings.push("direct_network".to_owned());
    }
    if declared
        .iter()
        .any(|permission| permission.starts_with("file.") || permission.starts_with("filesystem."))
        || !permission_policy.filesystem.read.is_empty()
        || !permission_policy.filesystem.write.is_empty()
    {
        findings.push("arbitrary_filesystem".to_owned());
    }
    if permission_policy.gpu || declared.iter().any(|permission| permission == "gpu") {
        findings.push("gpu".to_owned());
    }
    if permission_policy.clipboard
        || declared
            .iter()
            .any(|permission| permission.starts_with("clipboard"))
    {
        findings.push("clipboard".to_owned());
    }
    findings
}

pub fn enforce_framework_permission_policy(
    manifest: &FrameworkPackageManifest,
) -> Result<(), String> {
    let mode = plugin_permission_mode()?;
    enforce_framework_permission_mode(manifest, mode)
}

fn enforce_framework_permission_mode(
    manifest: &FrameworkPackageManifest,
    mode: PluginPermissionMode,
) -> Result<(), String> {
    let findings = unsupported_permission_findings(manifest);
    if mode == PluginPermissionMode::Strict && !findings.is_empty() {
        return Err(format!(
            "strict plugin permission mode cannot OS-enforce: {}",
            findings.join(", ")
        ));
    }
    Ok(())
}

/// The framework id that an execution belongs to (same mapping as
/// `execution_type_name`, exposed for readiness checks).
pub fn framework_id_for_execution(execution: &ToolExecution) -> &str {
    match execution {
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
        ToolExecution::FrameworkArt { framework } => framework,
    }
}

/// A third-party binary an art needs (installed in phase 1 to the art dir).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtBinary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An art's dependency manifest, carried under `metadata.dependencies`. The
/// `framework` field defaults to the execution-derived framework when absent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtDependencies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binaries: Vec<ArtBinary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arts: Vec<String>,
}

/// Read an art's dependency manifest from `metadata.dependencies`, defaulting
/// `framework` to the one derived from its execution kind.
pub fn read_dependencies(tool: &ToolDefinition) -> ArtDependencies {
    let mut deps: ArtDependencies = tool
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("dependencies"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    if deps.framework.is_none() {
        deps.framework = Some(framework_id_for_execution(&tool.execution).to_owned());
    }
    deps
}

fn framework_name(id: &str) -> &'static str {
    match id {
        "process" => "脚本",
        "cloud_api" => "云端",
        "mcp" => "MCP",
        "workflow" => "流程",
        _ => "第三方 Art 框架",
    }
}

fn framework_description(id: &str) -> &'static str {
    match id {
        "process" => "通过统一的本地进程边界运行命令、脚本或 Python Art。",
        "cloud_api" => "调用云端 HTTP API 处理图像。",
        "mcp" => "通过 MCP 服务器调用工具。",
        "workflow" => "把一条工作流封装成单一节点执行。",
        _ => "由外部插件包提供的 Art 执行框架。",
    }
}

/// Whether a framework needs an external package/runtime before it is ready.
/// All optional Art frameworks are package-backed. The helper remains public
/// for callers that need to distinguish package installation from ambient host
/// dependencies.
pub fn framework_needs_runtime(id: &str) -> bool {
    is_valid_framework(id)
}

/// Probe whether a framework package's runtime is available. `runtime_root` is
/// `<control-plane>/frameworks`. The package must contain a valid
/// `framework.manifest.json` and the manifest's process entry must exist.
pub fn framework_ready(id: &str) -> (bool, String) {
    framework_ready_in(id, None)
}

/// Readiness probe that also checks a control-plane runtime dir. `runtime_root`
/// points at `<control-plane>/frameworks`; the framework's own package
/// lives at `<runtime_root>/<id>/`.
pub fn framework_ready_in(id: &str, runtime_root: Option<&Path>) -> (bool, String) {
    if !is_valid_framework_reference(id) {
        return (false, "框架 ID 无效".to_owned());
    }

    let Some(root) = runtime_root else {
        return (false, "未提供框架包目录".to_owned());
    };
    let package_dir = match resolve_framework_package_dir(root, id) {
        Some(package_dir) => package_dir,
        None => return (false, "未找到活动框架包".to_owned()),
    };
    let manifest_path = package_dir.join(FRAMEWORK_MANIFEST_FILE);
    let manifest = match read_framework_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(detail) => return (false, detail),
    };
    if manifest.id != framework_local_id(id) {
        return (
            false,
            format!("框架包 ID 不匹配：期望 {id}，实际 {}", manifest.id),
        );
    }
    if let Err(error) = loom_protocol::negotiate_framework_protocol(&manifest) {
        return (false, format!("不支持的框架协议：{error}"));
    }
    if !manifest
        .platforms
        .iter()
        .any(|platform| platform == WINDOWS_X64_PLATFORM)
    {
        return (false, "框架包不支持 windows-x64".to_owned());
    }
    if manifest.entry.kind != "process" || manifest.entry.command.trim().is_empty() {
        return (false, "框架包缺少有效的进程入口".to_owned());
    }
    let command_path = Path::new(&manifest.entry.command);
    if command_path.is_absolute()
        || command_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return (false, "框架入口必须位于框架包目录内".to_owned());
    }
    let entry_path = package_dir.join(command_path);
    if !entry_path.is_file() {
        return (false, format!("框架入口不存在：{}", entry_path.display()));
    }
    if let Err(error) = enforce_framework_permission_policy(&manifest) {
        return (false, format!("框架权限策略拒绝执行：{error}"));
    }
    let trust_store_path = root.parent().unwrap_or(root).join(PLUGIN_TRUST_STORE_FILE);
    let trust_store = match TrustStore::load(&trust_store_path) {
        Ok(store) => store,
        Err(error) => return (false, format!("无法读取插件信任库：{error}")),
    };
    let trust_status = match verify_package_signature(
        &package_dir,
        Some(&manifest.publisher),
        manifest.signature.as_ref(),
        &trust_store,
    ) {
        Ok(status) => status,
        Err(error) => return (false, format!("框架包签名验证失败：{error}")),
    };
    if let Err(error) = TrustPolicy::from_env().enforce(trust_status) {
        return (false, format!("框架包信任策略拒绝执行：{error}"));
    }
    let control_plane_root = root.parent().unwrap_or(root);
    if let Err(error) = verify_framework_lockfile(control_plane_root, &package_dir, &manifest) {
        return (false, format!("框架包锁文件验证失败：{error}"));
    }
    (
        true,
        format!("已安装框架包 {} {}", manifest.name, manifest.version),
    )
}

pub fn resolve_framework_package_dir(runtime_root: &Path, id: &str) -> Option<PathBuf> {
    if !is_valid_framework_reference(id) {
        return None;
    }
    if id.contains('/') {
        return framework_storage_path(id)
            .map(|path| runtime_root.join(path))
            .as_deref()
            .and_then(resolve_framework_package_root);
    }
    let mut matches = Vec::new();
    for publisher in fs::read_dir(runtime_root).ok()? {
        let publisher = publisher.ok()?;
        if !publisher.path().is_dir() {
            continue;
        }
        let candidate = publisher.path().join(id);
        if let Some(package) = resolve_framework_package_root(&candidate) {
            let manifest = read_framework_manifest(&package.join(FRAMEWORK_MANIFEST_FILE)).ok()?;
            if manifest.id == id {
                matches.push(package);
            }
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}

fn resolve_framework_package_root(package_root: &Path) -> Option<PathBuf> {
    let active_path = package_root.join(FRAMEWORK_ACTIVE_FILE);
    if active_path.is_file() {
        let activation: FrameworkActivationState =
            serde_json::from_slice(&fs::read(active_path).ok()?).ok()?;
        if !is_safe_framework_version_path(&activation.active) {
            return None;
        }
        let relative = Path::new(&activation.active);
        let resolved = package_root.join(relative);
        return resolved
            .join(FRAMEWORK_MANIFEST_FILE)
            .is_file()
            .then_some(resolved);
    }
    None
}

fn framework_storage_path(reference: &str) -> Option<PathBuf> {
    if let Some((publisher, id)) = reference.split_once('/') {
        if publisher.contains('/')
            || !loom_protocol::is_safe_publisher_id(publisher)
            || !is_valid_framework(id)
        {
            return None;
        }
        Some(Path::new(publisher).join(id))
    } else {
        None
    }
}

fn framework_local_id(reference: &str) -> &str {
    reference
        .rsplit_once('/')
        .map(|(_, id)| id)
        .unwrap_or(reference)
}

fn read_framework_manifest(path: &Path) -> Result<FrameworkPackageManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("无法读取框架包清单 {}：{error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("框架包清单无效 {}：{error}", path.display()))
}

fn validate_framework_manifest(
    manifest: &FrameworkPackageManifest,
    package_dir: &Path,
) -> Result<(), FrameworkError> {
    let invalid = |reason: String| FrameworkError::InvalidPackage {
        id: manifest.id.clone(),
        reason,
    };
    loom_protocol::validate_framework_manifest_contract(manifest)
        .map_err(|error| invalid(error.to_string()))?;
    if !manifest
        .platforms
        .iter()
        .any(|platform| platform == WINDOWS_X64_PLATFORM)
    {
        return Err(invalid("windows-x64 is not supported".to_owned()));
    }
    if manifest.entry.kind != "process" {
        return Err(invalid("entry.kind must be process".to_owned()));
    }
    if manifest.entry.command.trim().is_empty() {
        return Err(invalid("entry.command is required".to_owned()));
    }
    let command_path = Path::new(&manifest.entry.command);
    if command_path.is_absolute()
        || command_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(invalid(
            "entry.command must be a relative package path".to_owned(),
        ));
    }
    if !package_dir.join(command_path).is_file() {
        return Err(invalid(format!(
            "entry.command does not exist: {}",
            manifest.entry.command
        )));
    }
    Ok(())
}

/// Tracks which framework packages the user has installed, persisted to
/// `<control-plane>/frameworks.json`. `root` also anchors installed framework
/// packages under `<root>/frameworks/<publisher>/<id>/`.
#[derive(Debug, Clone)]
pub struct FrameworkRegistry {
    root: PathBuf,
    path: PathBuf,
}

impl FrameworkRegistry {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let registry = Self {
            path: root.join(FRAMEWORKS_FILE),
            root,
        };
        let _ = registry.recover_uninstall_tombstones();
        let _ = registry.recover_lifecycle_journals();
        let _ = crate::install::recover_art_uninstall_tombstones(&registry.root);
        let _ = crate::install::recover_art_lifecycle(&registry.root);
        let _ = crate::dependency::RuntimeRegistry::new(&registry.root).prune_stale();
        registry
    }

    /// Directory holding this framework's active immutable package version:
    /// `<root>/frameworks/<publisher>/<id>/versions/<version-digest>/`.
    pub fn runtime_dir(&self, id: &str) -> PathBuf {
        resolve_framework_package_dir(&self.root.join(FRAMEWORK_PACKAGES_DIR), id)
            .unwrap_or_else(|| self.package_root(id))
    }

    fn package_root(&self, reference: &str) -> PathBuf {
        let storage_key = self
            .resolve_state_key(reference)
            .ok()
            .flatten()
            .unwrap_or_else(|| reference.to_owned());
        self.root.join(FRAMEWORK_PACKAGES_DIR).join(
            framework_storage_path(&storage_key)
                .unwrap_or_else(|| Path::new(".unresolved").join(framework_local_id(reference))),
        )
    }

    fn activation_path(&self, id: &str) -> PathBuf {
        self.package_root(id).join(FRAMEWORK_ACTIVE_FILE)
    }

    fn activation(&self, id: &str) -> Option<FrameworkActivationState> {
        serde_json::from_slice(&fs::read(self.activation_path(id)).ok()?).ok()
    }

    fn resolve_state_key(&self, reference: &str) -> Result<Option<String>, FrameworkError> {
        let states = self.installation_states();
        if states.contains_key(reference) {
            return Ok(Some(reference.to_owned()));
        }
        if reference.contains('/') {
            return Ok(None);
        }
        let matches = states
            .keys()
            .filter(|key| framework_local_id(key) == reference)
            .cloned()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [only] => Ok(Some(only.clone())),
            _ => Err(FrameworkError::AmbiguousFramework(reference.to_owned())),
        }
    }

    fn write_activation(
        &self,
        id: &str,
        activation: &FrameworkActivationState,
    ) -> Result<(), FrameworkError> {
        let path = self.activation_path(id);
        let parent = path
            .parent()
            .ok_or_else(|| FrameworkError::RuntimeUnavailable {
                id: id.to_owned(),
                reason: "activation path has no parent".to_owned(),
            })?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(activation)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        crate::replace_registry_file(&temporary, &path)?;
        Ok(())
    }

    fn lifecycle_path(&self, reference: &str) -> PathBuf {
        self.package_root(reference).join(FRAMEWORK_LIFECYCLE_FILE)
    }

    fn write_lifecycle_journal(
        &self,
        reference: &str,
        journal: &FrameworkLifecycleJournal,
    ) -> Result<(), FrameworkError> {
        let path = self.lifecycle_path(reference);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(journal)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        crate::replace_registry_file(&temporary, &path)?;
        Ok(())
    }

    fn clear_lifecycle_journal(&self, reference: &str) {
        let _ = fs::remove_file(self.lifecycle_path(reference));
    }

    fn recover_lifecycle_journals(&self) -> Result<(), FrameworkError> {
        let root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        if !root.is_dir() {
            return Ok(());
        }
        let mut package_roots = Vec::new();
        for first in fs::read_dir(&root)? {
            let first = first?.path();
            if !first.is_dir() {
                continue;
            }
            for second in fs::read_dir(&first).into_iter().flatten().flatten() {
                let second = second.path();
                if second.is_dir() && second.join(FRAMEWORK_LIFECYCLE_FILE).is_file() {
                    package_roots.push(second);
                }
            }
        }
        for package_root in package_roots {
            let journal_path = package_root.join(FRAMEWORK_LIFECYCLE_FILE);
            let journal: FrameworkLifecycleJournal =
                match serde_json::from_slice(&fs::read(&journal_path)?) {
                    Ok(journal) => journal,
                    Err(_) => {
                        let _ = fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                        continue;
                    }
                };
            if !framework_lifecycle_journal_is_safe(&journal) {
                let _ = fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                continue;
            }
            let activation_path = package_root.join(FRAMEWORK_ACTIVE_FILE);
            let current = serde_json::from_slice::<FrameworkActivationState>(
                &fs::read(&activation_path).unwrap_or_default(),
            )
            .ok();
            if current.as_ref() != Some(&journal.next_activation) {
                if let Some(old) = &journal.old_activation {
                    let temporary = activation_path.with_extension("json.tmp");
                    let mut bytes = serde_json::to_vec_pretty(old)?;
                    bytes.push(b'\n');
                    fs::write(&temporary, bytes)?;
                    crate::replace_registry_file(&temporary, &activation_path)?;
                } else {
                    let _ = fs::remove_file(&activation_path);
                }
                let target = package_root.join(&journal.target);
                if target.exists() {
                    let _ = remove_framework_tree(&target);
                }
            }
            let _ = fs::remove_file(journal_path);
        }
        Ok(())
    }

    fn recover_uninstall_tombstones(&self) -> Result<(), FrameworkError> {
        let root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        if !root.is_dir() {
            return Ok(());
        }
        let mut parents = Vec::new();
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if path.is_dir()
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(loom_protocol::is_safe_publisher_id)
            {
                parents.push(path);
            }
        }
        let installed = self.installation_states();
        for parent in parents {
            for entry in fs::read_dir(&parent)? {
                let tombstone = entry?.path();
                if !tombstone.is_dir() {
                    continue;
                }
                let Some(original_name) = uninstall_tombstone_original_name(
                    &tombstone,
                    FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX,
                ) else {
                    continue;
                };
                let Some(publisher) = parent.file_name().and_then(OsStr::to_str) else {
                    continue;
                };
                let reference = format!("{publisher}/{original_name}");
                if !is_valid_framework_reference(&reference) {
                    continue;
                }
                let live = parent.join(&original_name);
                if installed.contains_key(&reference) && !live.exists() {
                    fs::rename(&tombstone, &live)?;
                } else {
                    remove_framework_tree(&tombstone)?;
                }
            }
        }
        Ok(())
    }

    pub fn trust_store_path(&self) -> PathBuf {
        self.root.join(PLUGIN_TRUST_STORE_FILE)
    }

    pub fn trust_store(&self) -> Result<TrustStore, FrameworkError> {
        Ok(TrustStore::load(&self.trust_store_path())?)
    }

    pub fn trust_publisher(&self, record: PublisherTrustRecord) -> Result<(), FrameworkError> {
        let mut store = self.trust_store()?;
        store.trust(record);
        store.write_atomic(&self.trust_store_path())?;
        Ok(())
    }

    pub fn revoke_publisher(
        &self,
        publisher_id: &str,
        key_id: &str,
    ) -> Result<bool, FrameworkError> {
        let mut store = self.trust_store()?;
        let changed = store.revoke(publisher_id, key_id);
        if changed {
            store.write_atomic(&self.trust_store_path())?;
        }
        Ok(changed)
    }

    pub fn set_trust_policy(&self, policy: TrustPolicy) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        store.set_policy(policy);
        store.write_atomic(&self.trust_store_path())?;
        Ok(store)
    }

    pub fn trust_publisher_directory(
        &self,
        publisher_id: &str,
        records: impl IntoIterator<Item = PublisherTrustRecord>,
    ) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        store.untrust_publisher_id(publisher_id);
        store.trust_publisher_id(publisher_id.to_owned());
        for record in records {
            store.trust(record);
        }
        store.write_atomic(&self.trust_store_path())?;
        Ok(store)
    }

    pub fn untrust_publisher(&self, publisher_id: &str) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        if store.untrust_publisher_id(publisher_id) {
            store.write_atomic(&self.trust_store_path())?;
        }
        Ok(store)
    }

    /// The set of installed framework ids. A persisted state entry is not
    /// enough by itself: the package manifest must also be present.
    pub fn installed_ids(&self) -> BTreeSet<String> {
        self.installation_states()
            .into_iter()
            .filter_map(|(id, _)| {
                if !is_valid_framework_reference(&id) {
                    return None;
                }
                self.package_manifest(&id)
                    .map(|manifest| manifest.qualified_id())
            })
            .collect()
    }

    /// Whether a specific framework is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.resolve_state_key(id)
            .ok()
            .flatten()
            .is_some_and(|key| self.package_manifest(&key).is_some())
    }

    /// Whether an installed framework package is enabled for execution.
    pub fn is_enabled(&self, id: &str) -> bool {
        let Some(key) = self.resolve_state_key(id).ok().flatten() else {
            return false;
        };
        self.package_manifest(&key).is_some()
            && self
                .installation_states()
                .get(&key)
                .is_some_and(|state| state.enabled)
    }

    /// Readiness of a framework, probing its installed package manifest and
    /// process entry. Disabled or uninstalled packages are never ready.
    pub fn readiness(&self, id: &str) -> (bool, String) {
        let key = match self.resolve_state_key(id) {
            Ok(Some(key)) if self.package_manifest(&key).is_some() => key,
            Err(error) => return (false, error.to_string()),
            _ => return (false, "未安装".to_owned()),
        };
        if !self.is_installed(&key) {
            return (false, "未安装".to_owned());
        }
        if !self.is_enabled(&key) {
            return (false, "已禁用".to_owned());
        }
        let runtime_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        framework_ready_in(&key, Some(&runtime_root))
    }

    /// Full status for the host catalog plus any installed third-party
    /// framework packages.
    pub fn statuses(&self) -> Vec<FrameworkStatus> {
        let installed = self.installed_ids();
        let installed_local_ids = installed
            .iter()
            .map(|id| framework_local_id(id).to_owned())
            .collect::<BTreeSet<_>>();
        let mut ids = installed;
        ids.extend(
            FRAMEWORK_IDS
                .iter()
                .filter(|id| !installed_local_ids.contains(**id))
                .map(|id| (*id).to_owned()),
        );
        ids.into_iter().map(|id| self.status_of(&id)).collect()
    }

    /// Install a framework package from the configured store. The package
    /// manifest and process entry must be present before the state is saved.
    pub fn install(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.install_with_runtime_fetcher(id, &default_runtime_fetcher)
    }

    /// Install variant with an injectable package fetcher (a closure returning
    /// the framework package zip bytes for a framework id). Testable without
    /// the network.
    pub fn install_with_runtime_fetcher<F>(
        &self,
        id: &str,
        fetch_runtime: &F,
    ) -> Result<FrameworkStatus, FrameworkError>
    where
        F: Fn(&str) -> Result<Vec<u8>, FrameworkError>,
    {
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let package = fetch_runtime(id)?;
        self.install_framework_package_zip(&package, Some(id))
    }

    /// Install a framework package supplied as a ZIP. The ZIP must contain a
    /// root `framework.manifest.json` and the manifest's process entry.
    pub fn install_framework_package_from_zip(
        &self,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        self.install_framework_package_zip(zip_bytes, None)
    }

    /// Upgrade a package by replacing its installed directory with a fully
    /// validated new ZIP. Installation and upgrade share the same atomic path
    /// so a bad package cannot leave a half-written runtime behind.
    pub fn upgrade_framework_package_from_zip(
        &self,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        self.install_framework_package_zip(zip_bytes, None)
    }

    /// Upgrade a specific installed framework package and reject a ZIP whose
    /// manifest belongs to another framework.
    pub fn upgrade_framework_package(
        &self,
        id: &str,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        self.install_framework_package_zip(zip_bytes, Some(&key))
    }

    /// Enable an installed framework package.
    pub fn enable(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.set_enabled(id, true)
    }

    /// Disable an installed framework package.
    pub fn disable(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.set_enabled(id, false)
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let mut installed = self.installation_states();
        let state = installed
            .get_mut(&key)
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        state.enabled = enabled;
        if let Some(manifest) = self.package_manifest(&key) {
            state.version = manifest.version;
        }
        self.write_installed(&installed)?;
        Ok(self.status_of(&key))
    }

    fn install_framework_package_zip(
        &self,
        zip_bytes: &[u8],
        expected_id: Option<&str>,
    ) -> Result<FrameworkStatus, FrameworkError> {
        let staging = self.staging_dir(framework_local_id(expected_id.unwrap_or("package")));
        let result = (|| {
            unpack_runtime_zip(expected_id.unwrap_or("package"), zip_bytes, &staging)?;
            let manifest = read_framework_manifest(&staging.join(FRAMEWORK_MANIFEST_FILE))
                .map_err(|reason| FrameworkError::InvalidPackage {
                    id: expected_id.unwrap_or("package").to_owned(),
                    reason,
                })?;
            if !is_valid_framework(&manifest.id) {
                return Err(FrameworkError::UnknownFramework(manifest.id));
            }
            if let Some(expected_id) = expected_id {
                if manifest.id != expected_id && manifest.qualified_id() != expected_id {
                    return Err(FrameworkError::InvalidPackage {
                        id: expected_id.to_owned(),
                        reason: format!("manifest id is {}", manifest.id),
                    });
                }
            }
            validate_framework_manifest(&manifest, &staging)?;
            enforce_framework_permission_policy(&manifest).map_err(|reason| {
                FrameworkError::InvalidPackage {
                    id: manifest.qualified_id(),
                    reason,
                }
            })?;
            let resolved_dependencies =
                resolve_framework_dependencies(&self.root, &manifest, &staging)?;
            let trust_store = self.trust_store()?;
            let trust_status = verify_package_signature(
                &staging,
                Some(&manifest.publisher),
                manifest.signature.as_ref(),
                &trust_store,
            )?;
            TrustPolicy::from_env().enforce(trust_status)?;
            run_framework_self_test(&manifest, &staging)?;

            let storage_key = manifest.qualified_id();
            let packages_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
            fs::create_dir_all(&packages_root)?;
            let package_root = self.package_root(&storage_key);
            let versions_root = package_root.join(FRAMEWORK_VERSIONS_DIR);
            fs::create_dir_all(&versions_root)?;
            let digest = canonical_package_digest(
                &staging,
                manifest
                    .signature
                    .as_ref()
                    .map(|signature| signature.file.as_str()),
            )?;
            let version_dir = format!(
                "{}-{}",
                sanitize_version_for_path(&manifest.version),
                &digest[..12]
            );
            let active_relative = Path::new(FRAMEWORK_VERSIONS_DIR).join(version_dir);
            let active_relative_text = active_relative.to_string_lossy().replace('\\', "/");
            let target = package_root.join(&active_relative);
            let target_created = if target.exists() {
                fs::remove_dir_all(&staging)?;
                false
            } else {
                fs::rename(&staging, &target)?;
                true
            };
            set_framework_tree_readonly(&target, true)?;
            register_framework_runtimes(&self.root, &manifest, &target)?;
            if let Err(error) = write_framework_lockfile(
                &package_root,
                &storage_key,
                &manifest.version,
                &digest,
                resolved_dependencies,
            ) {
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                return Err(error);
            }

            let old_activation = self.activation(&storage_key);
            let previous = old_activation
                .as_ref()
                .and_then(|activation| {
                    (activation.active != active_relative_text).then(|| activation.active.clone())
                })
                .or_else(|| {
                    old_activation
                        .as_ref()
                        .and_then(|activation| activation.previous.clone())
                });
            let activation = FrameworkActivationState {
                active: active_relative_text,
                previous,
            };
            self.write_lifecycle_journal(
                &storage_key,
                &FrameworkLifecycleJournal {
                    old_activation: old_activation.clone(),
                    next_activation: activation.clone(),
                    target: active_relative.to_string_lossy().replace('\\', "/"),
                },
            )?;
            if let Err(error) = self.write_activation(&storage_key, &activation) {
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                self.clear_lifecycle_journal(&storage_key);
                return Err(error);
            }

            let mut installed = self.installation_states();
            installed.insert(
                storage_key.clone(),
                FrameworkInstallationState {
                    version: manifest.version.clone(),
                    enabled: true,
                },
            );
            if let Err(error) = self.write_installed(&installed) {
                if let Some(old_activation) = old_activation {
                    let _ = self.write_activation(&storage_key, &old_activation);
                } else {
                    let _ = fs::remove_file(self.activation_path(&storage_key));
                }
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                self.clear_lifecycle_journal(&storage_key);
                return Err(error);
            }
            prune_framework_versions(&package_root, &activation)?;
            let _ = crate::dependency::RuntimeRegistry::new(&self.root).prune_stale();
            self.clear_lifecycle_journal(&storage_key);
            Ok(self.status_of(&storage_key))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    fn staging_dir(&self, id: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.root.join(format!(".loom-framework-{id}-{nonce}"))
    }

    pub fn rollback(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let activation = self
            .activation(&key)
            .ok_or_else(|| FrameworkError::NoRollback { id: id.to_owned() })?;
        if !framework_activation_is_safe(&activation) {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "activation state contains an unsafe version path".to_owned(),
            });
        }
        let previous = activation
            .previous
            .clone()
            .ok_or_else(|| FrameworkError::NoRollback { id: id.to_owned() })?;
        let next = FrameworkActivationState {
            active: previous,
            previous: Some(activation.active.clone()),
        };
        let target = self.package_root(&key).join(&next.active);
        if !target.join(FRAMEWORK_MANIFEST_FILE).is_file() {
            return Err(FrameworkError::NoRollback { id: id.to_owned() });
        }
        let manifest =
            read_framework_manifest(&target.join(FRAMEWORK_MANIFEST_FILE)).map_err(|reason| {
                FrameworkError::InvalidPackage {
                    id: key.clone(),
                    reason,
                }
            })?;
        if manifest.qualified_id() != key && manifest.id != key {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "rollback package identity does not match the installed publisher"
                    .to_owned(),
            });
        }
        let trust_store = self.trust_store()?;
        let trust_status = verify_package_signature(
            &target,
            Some(&manifest.publisher),
            manifest.signature.as_ref(),
            &trust_store,
        )?;
        TrustPolicy::from_env().enforce(trust_status)?;
        let digest = canonical_package_digest(
            &target,
            manifest
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )?;
        if !next.active.ends_with(&digest[..12]) {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "rollback package digest does not match its immutable version path"
                    .to_owned(),
            });
        }
        enforce_framework_permission_policy(&manifest).map_err(|reason| {
            FrameworkError::InvalidPackage {
                id: key.clone(),
                reason,
            }
        })?;
        run_framework_self_test(&manifest, &target)?;
        self.write_lifecycle_journal(
            &key,
            &FrameworkLifecycleJournal {
                old_activation: Some(activation.clone()),
                next_activation: next.clone(),
                target: next.active.clone(),
            },
        )?;
        self.write_activation(&key, &next)?;
        let mut installed = self.installation_states();
        if let Some(state) = installed.get_mut(&key) {
            state.version = manifest.version;
        }
        if let Err(error) = self.write_installed(&installed) {
            let _ = self.write_activation(&key, &activation);
            self.clear_lifecycle_journal(&key);
            return Err(error);
        }
        self.clear_lifecycle_journal(&key);
        Ok(self.status_of(&key))
    }

    /// Mark a framework uninstalled and remove any downloaded runtime. Errors on
    /// an unknown id.
    pub fn uninstall(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let package_root = self.package_root(&key);
        let tombstone = if package_root.exists() {
            let tombstone =
                uninstall_tombstone_path(&package_root, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)?;
            fs::rename(&package_root, &tombstone)?;
            Some(tombstone)
        } else {
            None
        };
        let mut installed = self.installation_states();
        installed.remove(&key);
        if let Err(error) = self.write_installed(&installed) {
            if let Some(tombstone) = &tombstone {
                let _ = fs::rename(tombstone, &package_root);
            }
            return Err(error);
        }
        if let Some(tombstone) = tombstone {
            remove_framework_tree(&tombstone)?;
        }
        let _ = crate::dependency::RuntimeRegistry::new(&self.root).prune_stale();
        Ok(self.status_of(framework_local_id(&key)))
    }

    fn status_of(&self, id: &str) -> FrameworkStatus {
        let manifest = self.package_manifest(id);
        let state_key = self.resolve_state_key(id).ok().flatten();
        let state = state_key
            .as_ref()
            .and_then(|key| self.installation_states().get(key).cloned());
        let installed = state.is_some() && manifest.is_some();
        let enabled = installed && state.as_ref().map(|value| value.enabled).unwrap_or(false);
        let (name, description, version) = match &manifest {
            Some(manifest) => (
                manifest.name.clone(),
                manifest.description.clone(),
                Some(manifest.version.clone()),
            ),
            None => (
                framework_name(framework_local_id(id)).to_owned(),
                framework_description(framework_local_id(id)).to_owned(),
                None,
            ),
        };
        let (ready, ready_detail) = if !installed {
            (false, "未安装".to_owned())
        } else if !enabled {
            (false, "已禁用".to_owned())
        } else {
            self.readiness(state_key.as_deref().unwrap_or(id))
        };
        let trust_status = manifest
            .as_ref()
            .and_then(|manifest| {
                self.trust_store().ok().and_then(|trust_store| {
                    verify_package_signature(
                        &self.runtime_dir(id),
                        Some(&manifest.publisher),
                        manifest.signature.as_ref(),
                        &trust_store,
                    )
                    .ok()
                })
            })
            .unwrap_or_default();
        FrameworkStatus {
            id: manifest
                .as_ref()
                .map(|manifest| manifest.id.clone())
                .unwrap_or_else(|| framework_local_id(id).to_owned()),
            qualified_id: manifest
                .as_ref()
                .map(FrameworkPackageManifest::qualified_id)
                .unwrap_or_else(|| id.to_owned()),
            name,
            description,
            installed,
            enabled,
            ready,
            ready_detail,
            version,
            runtime_dir: installed.then(|| self.runtime_dir(state_key.as_deref().unwrap_or(id))),
            publisher: manifest.as_ref().map(|value| value.publisher.clone()),
            permission_policy: manifest
                .as_ref()
                .map(|value| value.permission_policy.clone())
                .unwrap_or_default(),
            declared_permissions: manifest
                .as_ref()
                .map(|value| value.permissions.clone())
                .unwrap_or_default(),
            resources: manifest
                .as_ref()
                .map(|value| value.resources.clone())
                .unwrap_or_default(),
            authoring_schema: manifest.and_then(|value| value.authoring_schema),
            trust_status,
        }
    }

    fn package_manifest(&self, id: &str) -> Option<FrameworkPackageManifest> {
        let manifest =
            read_framework_manifest(&self.runtime_dir(id).join(FRAMEWORK_MANIFEST_FILE)).ok()?;
        ((manifest.id == id || manifest.qualified_id() == id)
            && loom_protocol::negotiate_framework_protocol(&manifest).is_ok()
            && manifest
                .platforms
                .iter()
                .any(|platform| platform == WINDOWS_X64_PLATFORM))
        .then_some(manifest)
    }

    fn installation_states(&self) -> BTreeMap<String, FrameworkInstallationState> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return BTreeMap::new();
        };
        serde_json::from_str::<BTreeMap<String, FrameworkInstallationState>>(&text)
            .unwrap_or_default()
    }

    fn write_installed(
        &self,
        installed: &BTreeMap<String, FrameworkInstallationState>,
    ) -> Result<(), FrameworkError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(installed)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, format!("{text}\n"))?;
        crate::replace_registry_file(&temporary, &self.path)?;
        Ok(())
    }
}

fn uninstall_tombstone_path(live: &Path, prefix: &str) -> Result<PathBuf, FrameworkError> {
    let parent = live
        .parent()
        .ok_or_else(|| FrameworkError::InvalidPackage {
            id: live.display().to_string(),
            reason: "package root has no parent".to_owned(),
        })?;
    let name =
        live.file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| FrameworkError::InvalidPackage {
                id: live.display().to_string(),
                reason: "package root has no UTF-8 name".to_owned(),
            })?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(parent.join(format!("{prefix}{name}--{nonce}")))
}

fn uninstall_tombstone_original_name(path: &Path, prefix: &str) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_prefix(prefix)?;
    let (original, nonce) = name.rsplit_once("--")?;
    (!original.is_empty()
        && nonce.bytes().all(|byte| byte.is_ascii_digit())
        && is_valid_framework(original))
    .then(|| original.to_owned())
}

fn set_framework_tree_readonly(path: &Path, readonly: bool) -> Result<(), FrameworkError> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            set_framework_tree_readonly(&entry?.path(), readonly)?;
        }
    }
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        permissions.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn remove_framework_tree(path: &Path) -> Result<(), FrameworkError> {
    if path.exists() {
        set_framework_tree_readonly(path, false)?;
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn is_safe_framework_version_path(value: &str) -> bool {
    let path = Path::new(value);
    if path.is_absolute() {
        return false;
    }
    let mut components = path.components();
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(Component::Normal(root)), Some(Component::Normal(_)), None)
            if root == OsStr::new(FRAMEWORK_VERSIONS_DIR)
    )
}

fn framework_activation_is_safe(activation: &FrameworkActivationState) -> bool {
    is_safe_framework_version_path(&activation.active)
        && activation
            .previous
            .as_deref()
            .is_none_or(is_safe_framework_version_path)
}

fn framework_lifecycle_journal_is_safe(journal: &FrameworkLifecycleJournal) -> bool {
    is_safe_framework_version_path(&journal.target)
        && framework_activation_is_safe(&journal.next_activation)
        && journal
            .old_activation
            .as_ref()
            .is_none_or(framework_activation_is_safe)
}

fn framework_history_limit() -> usize {
    std::env::var("LOOM_PLUGIN_VERSION_HISTORY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(2)
}

fn prune_framework_versions(
    package_root: &Path,
    activation: &FrameworkActivationState,
) -> Result<(), FrameworkError> {
    let versions_root = package_root.join(FRAMEWORK_VERSIONS_DIR);
    if !versions_root.is_dir() {
        return Ok(());
    }
    let keep_limit = framework_history_limit();
    let mut entries = fs::read_dir(&versions_root)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    let active = package_root.join(&activation.active);
    let previous = activation
        .previous
        .as_ref()
        .map(|path| package_root.join(path));
    let mut retained = 0usize;
    for entry in entries {
        let path = entry.path();
        let pinned = path == active || previous.as_ref().is_some_and(|previous| *previous == path);
        if pinned || retained < keep_limit.saturating_sub(2) {
            if !pinned {
                retained += 1;
            }
            continue;
        }
        remove_framework_tree(&path)?;
    }
    Ok(())
}

fn sanitize_version_for_path(version: &str) -> String {
    let sanitized = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_owned()
    } else {
        sanitized
    }
}

fn resolve_framework_dependencies(
    control_plane_root: &Path,
    manifest: &FrameworkPackageManifest,
    staging: &Path,
) -> Result<Vec<loom_protocol::ResolvedDependency>, FrameworkError> {
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let registry = crate::dependency::RuntimeRegistry::new(control_plane_root);
    let mut candidates = registry
        .list()
        .map_err(|reason| FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        })?;
    let python = staging.join("python-embed");
    if python.is_dir() {
        let version = std::env::var("LOOM_PYTHON_RUNTIME_VERSION")
            .ok()
            .filter(|version| semver::Version::parse(version).is_ok())
            .unwrap_or_else(|| "3.12.0".to_owned());
        let sha256 = canonical_package_digest(&python, None)?;
        candidates.push(crate::dependency::PackageCandidate {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version,
            sha256,
            path: python,
        });
    }
    crate::dependency::resolve_dependencies(&manifest.dependencies, &candidates).map_err(|reason| {
        FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        }
    })
}

fn register_framework_runtimes(
    control_plane_root: &Path,
    manifest: &FrameworkPackageManifest,
    package_dir: &Path,
) -> Result<(), FrameworkError> {
    let python = package_dir.join("python-embed");
    if !python.is_dir() {
        return Ok(());
    }
    let version = std::env::var("LOOM_PYTHON_RUNTIME_VERSION")
        .ok()
        .filter(|version| semver::Version::parse(version).is_ok())
        .unwrap_or_else(|| "3.12.0".to_owned());
    let sha256 = canonical_package_digest(&python, None)?;
    crate::dependency::RuntimeRegistry::new(control_plane_root)
        .register(crate::dependency::PackageCandidate {
            kind: "runtime".to_owned(),
            id: "loom.runtime.python".to_owned(),
            version,
            sha256,
            path: python,
        })
        .map_err(|reason| FrameworkError::InvalidPackage {
            id: manifest.id.clone(),
            reason,
        })
}

fn write_framework_lockfile(
    package_root: &Path,
    qualified_id: &str,
    version: &str,
    package_digest: &str,
    resolved: Vec<loom_protocol::ResolvedDependency>,
) -> Result<(), FrameworkError> {
    let lockfile = loom_protocol::PluginLockfile {
        schema_version: loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION,
        package_id: qualified_id.to_owned(),
        package_version: version.to_owned(),
        resolved,
    };
    let locks = package_root.join("locks");
    fs::create_dir_all(&locks)?;
    let path = locks.join(format!("{package_digest}.json"));
    let temporary = path.with_extension("json.tmp");
    let mut bytes = serde_json::to_vec_pretty(&lockfile)?;
    bytes.push(b'\n');
    fs::write(&temporary, bytes)?;
    crate::replace_registry_file(&temporary, &path)?;
    Ok(())
}

fn verify_framework_lockfile(
    control_plane_root: &Path,
    package_dir: &Path,
    manifest: &FrameworkPackageManifest,
) -> Result<(), String> {
    let versions_root = package_dir
        .parent()
        .ok_or_else(|| "framework package has no versions directory".to_owned())?;
    if versions_root.file_name() != Some(OsStr::new(FRAMEWORK_VERSIONS_DIR)) {
        return Err("framework package is not an immutable versioned install".to_owned());
    }
    let package_root = versions_root
        .parent()
        .ok_or_else(|| "version directory has no package root".to_owned())?;
    let digest = canonical_package_digest(
        package_dir,
        manifest
            .signature
            .as_ref()
            .map(|signature| signature.file.as_str()),
    )
    .map_err(|error| error.to_string())?;
    let lockfile_path = package_root.join("locks").join(format!("{digest}.json"));
    let lockfile: loom_protocol::PluginLockfile = serde_json::from_slice(
        &fs::read(&lockfile_path)
            .map_err(|error| format!("cannot read {}: {error}", lockfile_path.display()))?,
    )
    .map_err(|error| format!("invalid lockfile JSON: {error}"))?;
    if lockfile.schema_version != loom_protocol::PLUGIN_LOCKFILE_SCHEMA_VERSION
        || lockfile.package_id != manifest.qualified_id()
        || lockfile.package_version != manifest.version
    {
        return Err("lockfile identity, version, or schema is invalid".to_owned());
    }

    let candidates = crate::dependency::RuntimeRegistry::new(control_plane_root)
        .list()?
        .into_iter()
        .filter(|candidate| candidate.path.is_dir())
        .collect::<Vec<_>>();
    let mut locked = BTreeSet::new();
    for resolved in &lockfile.resolved {
        let key = (resolved.kind.clone(), resolved.id.clone());
        if !locked.insert(key.clone()) {
            return Err(format!(
                "dependency `{}/{}` appears more than once in the lockfile",
                key.0, key.1
            ));
        }
        let declared = manifest
            .dependencies
            .iter()
            .find(|dependency| dependency.kind == resolved.kind && dependency.id == resolved.id)
            .ok_or_else(|| {
                format!(
                    "lockfile contains undeclared dependency `{}/{}`",
                    resolved.kind, resolved.id
                )
            })?;
        let requirement = semver::VersionReq::parse(&declared.version)
            .map_err(|error| format!("invalid dependency requirement: {error}"))?;
        let version = semver::Version::parse(&resolved.version)
            .map_err(|error| format!("invalid locked dependency version: {error}"))?;
        if !requirement.matches(&version)
            || declared
                .sha256
                .as_deref()
                .is_some_and(|expected| !expected.eq_ignore_ascii_case(&resolved.sha256))
        {
            return Err(format!(
                "locked dependency `{}` no longer satisfies the manifest",
                resolved.id
            ));
        }
        if !candidates.iter().any(|candidate| {
            candidate.kind == resolved.kind
                && candidate.id == resolved.id
                && candidate.version == resolved.version
                && candidate.sha256.eq_ignore_ascii_case(&resolved.sha256)
        }) {
            return Err(format!(
                "locked dependency `{}` is unavailable or has changed",
                resolved.id
            ));
        }
    }
    for dependency in &manifest.dependencies {
        if !dependency.optional
            && !locked.contains(&(dependency.kind.clone(), dependency.id.clone()))
        {
            return Err(format!(
                "required dependency `{}` is missing from the lockfile",
                dependency.id
            ));
        }
    }
    Ok(())
}

/// Resolve the runtime download URL for a framework. Uses the art store base
/// (`LOOM_ART_STORE_URL`, overridable per-framework by
/// `LOOM_FRAMEWORK_RUNTIME_URL`), fetching `<store>/frameworks/<id>.zip`.
fn framework_runtime_url(id: &str) -> Option<String> {
    if let Ok(explicit) = std::env::var("LOOM_FRAMEWORK_RUNTIME_URL") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }
    let store = std::env::var("LOOM_ART_STORE_URL").ok()?;
    let store = store.trim().trim_end_matches('/');
    if store.is_empty() {
        return None;
    }
    Some(format!("{store}/frameworks/{id}.zip"))
}

fn configured_framework_package_path(id: &str) -> Option<PathBuf> {
    let root = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)?;
    let root = PathBuf::from(root);
    (!root.as_os_str().is_empty()).then(|| root.join(format!("{id}.zip")))
}

fn packaged_framework_catalog_roots(executable: &Path) -> Vec<PathBuf> {
    let Some(executable_dir) = executable.parent() else {
        return Vec::new();
    };
    let mut roots = vec![executable_dir.join("packages").join("frameworks")];
    if let Some(release_root) = executable_dir.parent() {
        roots.push(release_root.join("packages").join("frameworks"));
    }
    roots
}

fn packaged_framework_package_path(id: &str) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    packaged_framework_catalog_roots(&executable)
        .into_iter()
        .map(|root| root.join(format!("{id}.zip")))
        .find(|path| path.is_file())
}

fn read_framework_package_from_catalog(
    id: &str,
    package_path: &Path,
) -> Result<Vec<u8>, FrameworkError> {
    let metadata =
        fs::metadata(package_path).map_err(|error| FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "cannot read local package `{}`: {error}",
                package_path.display()
            ),
        })?;
    if !metadata.is_file() {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!("local package is not a file: {}", package_path.display()),
        });
    }
    if metadata.len() > FRAMEWORK_PACKAGE_MAX_BYTES {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "local package exceeds {FRAMEWORK_PACKAGE_MAX_BYTES} bytes: {}",
                package_path.display()
            ),
        });
    }
    let bytes = fs::read(package_path).map_err(|error| FrameworkError::RuntimeDownloadFailed {
        id: id.to_owned(),
        reason: format!(
            "cannot read local package `{}`: {error}",
            package_path.display()
        ),
    })?;
    let checksum_path = package_path.with_extension("zip.sha256");
    let checksum = fs::read_to_string(&checksum_path).map_err(|error| {
        FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "cannot read local package checksum `{}`: {error}",
                checksum_path.display()
            ),
        }
    })?;
    let mut fields = checksum.split_whitespace();
    let expected_hash = fields
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let expected_name = fields.next();
    let package_name = package_path.file_name().and_then(OsStr::to_str);
    if expected_hash.is_none() || expected_name != package_name || fields.next().is_some() {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "invalid local package checksum: {}",
                checksum_path.display()
            ),
        });
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(expected_hash.expect("validated checksum hash")) {
        return Err(FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!(
                "local package checksum mismatch: {}",
                package_path.display()
            ),
        });
    }
    Ok(bytes)
}

/// Load a framework package from an explicit local catalog, a configured
/// network store, or the package catalog next to a formal Loom release.
fn default_runtime_fetcher(id: &str) -> Result<Vec<u8>, FrameworkError> {
    if let Some(package_path) = configured_framework_package_path(id) {
        return read_framework_package_from_catalog(id, &package_path);
    }
    let Some(url) = framework_runtime_url(id) else {
        if let Some(package_path) = packaged_framework_package_path(id) {
            return read_framework_package_from_catalog(id, &package_path);
        }
        return Err(FrameworkError::RuntimeSourceMissing { id: id.to_owned() });
    };
    let policy = crate::network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..crate::network_policy::OutboundPolicy::default()
    };
    let client = crate::network_policy::secure_client(
        "Loom/0.1 Framework Runtime Fetch",
        std::time::Duration::from_secs(600),
        policy.clone(),
    )
    .map_err(|error| FrameworkError::RuntimeDownloadFailed {
        id: id.to_owned(),
        reason: error,
    })?;
    crate::network_policy::get_bounded(&client, &url, &policy, FRAMEWORK_PACKAGE_MAX_BYTES as usize)
        .map_err(|error| FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: error,
        })
}

fn run_framework_self_test(
    manifest: &FrameworkPackageManifest,
    package_dir: &Path,
) -> Result<(), FrameworkError> {
    let Some(health_check) = &manifest.health_check else {
        return Ok(());
    };
    let command_path = Path::new(&manifest.entry.command);
    let executable =
        loom_process::executable_path_within(package_dir, command_path).map_err(|reason| {
            FrameworkError::RuntimeUnavailable {
                id: manifest.id.clone(),
                reason,
            }
        })?;
    let mut process = ProcessSpec::new(executable);
    process.args = manifest.entry.args.clone();
    process.args.push("--loom-health-check".to_owned());
    process.args.push(health_check.command.clone());
    process.args.extend(health_check.args.clone());
    process.current_dir = Some(package_dir.to_path_buf());
    process.limits.timeout = std::time::Duration::from_secs(health_check.timeout_seconds.max(1));
    process.limits.stdout_bytes = 1024 * 1024;
    process.limits.stderr_bytes = 1024 * 1024;
    process.limits.memory_bytes = manifest
        .resources
        .memory_mib
        .and_then(|value| usize::try_from(value.saturating_mul(1024 * 1024)).ok())
        .or(process.limits.memory_bytes);
    process.limits.max_processes = manifest
        .resources
        .max_processes
        .or(process.limits.max_processes);
    let output = loom_process::run_with_input(&process, b"").map_err(|error| {
        FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: format!("framework self-test failed: {error}"),
        }
    })?;
    if !output.status.success() {
        return Err(FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: format!(
                "framework self-test exited with {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let response: FrameworkExecuteResponse =
        serde_json::from_slice(&output.stdout).map_err(|error| {
            FrameworkError::RuntimeUnavailable {
                id: manifest.id.clone(),
                reason: format!("framework self-test returned invalid JSON: {error}"),
            }
        })?;
    if !response_status_is_success(&response.status.to_ascii_lowercase()) {
        return Err(FrameworkError::RuntimeUnavailable {
            id: manifest.id.clone(),
            reason: response
                .error
                .map(|error| format!("{}: {}", error.code, error.message))
                .unwrap_or_else(|| "framework self-test returned failure".to_owned()),
        });
    }
    Ok(())
}

/// Unpack a framework runtime zip into `runtime_dir`, replacing any prior
/// install. Rejects entries with unsafe (traversal) paths.
fn unpack_runtime_zip(
    id: &str,
    zip_bytes: &[u8],
    runtime_dir: &Path,
) -> Result<(), FrameworkError> {
    let fail = |reason: String| FrameworkError::RuntimeUnpackFailed {
        id: id.to_owned(),
        reason,
    };
    if runtime_dir.exists() {
        fs::remove_dir_all(runtime_dir)?;
    }
    fs::create_dir_all(runtime_dir)?;
    crate::secure_zip::extract_zip_securely(zip_bytes, runtime_dir)
        .map_err(|error| fail(error.to_string()))?;
    Ok(())
}

pub(crate) fn is_valid_framework(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
        && !id.ends_with('.')
}

pub(crate) fn is_valid_framework_reference(reference: &str) -> bool {
    is_valid_framework(reference)
        || reference.split_once('/').is_some_and(|(publisher, id)| {
            !publisher.contains('/')
                && loom_protocol::is_safe_publisher_id(publisher)
                && is_valid_framework(id)
        })
}

#[derive(Debug, thiserror::Error)]
pub enum FrameworkError {
    #[error("unknown framework `{0}`")]
    UnknownFramework(String),
    #[error("framework id `{0}` matches multiple publishers; use a qualified id")]
    AmbiguousFramework(String),
    #[error("framework `{0}` is not installed")]
    FrameworkNotInstalled(String),
    #[error("framework `{id}` has no previous version available for rollback")]
    NoRollback { id: String },
    #[error("invalid framework package `{id}`: {reason}")]
    InvalidPackage { id: String, reason: String },
    #[error("framework `{id}` has no available package source (ship packages/frameworks with Loom, or set LOOM_FRAMEWORK_PACKAGE_CATALOG_DIR, LOOM_ART_STORE_URL, or LOOM_FRAMEWORK_RUNTIME_URL)")]
    RuntimeSourceMissing { id: String },
    #[error("framework `{id}` runtime download failed: {reason}")]
    RuntimeDownloadFailed { id: String, reason: String },
    #[error("framework `{id}` runtime unpack failed: {reason}")]
    RuntimeUnpackFailed { id: String, reason: String },
    #[error("framework `{id}` runtime installed but still not runnable: {reason}")]
    RuntimeUnavailable { id: String, reason: String },
    #[error("frameworks store io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("frameworks store serialize error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("framework package security error: {0}")]
    Security(#[from] PluginSecurityError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ROOT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "loom-frameworks-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            TEMP_ROOT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn official_framework_names_match_ui_vocabulary() {
        assert_eq!(framework_name("cloud_api"), "云端");
        assert_eq!(framework_name("mcp"), "MCP");
        assert_eq!(framework_name("process"), "脚本");
        assert_eq!(framework_name("workflow"), "流程");
    }

    #[test]
    fn starts_with_no_frameworks_installed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let installed = registry.installed_ids();
        assert!(installed.is_empty());
        for id in FRAMEWORK_IDS {
            assert!(!registry.is_installed(id));
        }
        for status in registry.statuses() {
            assert!(!status.installed);
            assert!(!status.enabled);
            assert!(!status.ready);
            assert!(status.version.is_none());
            assert!(status.runtime_dir.is_none());
        }
        // All optional frameworks, including the former built-in kinds, are
        // absent from a fresh control plane.
        assert!(!installed.contains("process"));
        assert!(!installed.contains("mcp"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_and_uninstall_roundtrip() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let status = registry
            .install_with_runtime_fetcher("mcp", &|_id| Ok(fake_framework_package_zip("mcp")))
            .expect("install mcp");
        assert!(status.installed);
        assert!(status.enabled);
        assert!(status.ready, "mcp package entry should be ready");
        assert_eq!(status.version.as_deref(), Some("0.1.0"));
        assert!(status.runtime_dir.is_some());
        assert!(registry.is_installed("mcp"));

        registry.uninstall("mcp").expect("uninstall mcp");
        assert!(!registry.is_installed("mcp"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn third_party_framework_package_is_dynamic_and_lifecycle_managed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let id = "third-party-echo";
        let installed = registry
            .install_framework_package_from_zip(&fake_framework_package_zip(id))
            .expect("install third-party framework");

        assert_eq!(installed.id, id);
        assert!(installed.installed);
        assert!(installed.ready);
        assert!(registry.statuses().iter().any(|status| status.id == id));

        let disabled = registry.disable(id).expect("disable third-party framework");
        assert!(!disabled.ready);
        let enabled = registry.enable(id).expect("enable third-party framework");
        assert!(enabled.ready);
        let removed = registry
            .uninstall(id)
            .expect("uninstall third-party framework");
        assert!(!removed.installed);
        assert!(!registry.installed_ids().contains(id));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn statuses_cover_all_four_frameworks() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let statuses = registry.statuses();
        assert_eq!(statuses.len(), 4);
        for id in FRAMEWORK_IDS {
            let status = statuses.iter().find(|status| status.id == id).unwrap();
            assert!(!status.installed, "{id} should not be installed by default");
            assert!(!status.enabled, "{id} should not be enabled by default");
            assert!(!status.ready, "{id} should not be ready by default");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_framework_rejected() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        assert!(registry.install("nope").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn packaged_catalog_discovery_reaches_release_root_from_runtime_sidecar() {
        let executable = Path::new("C:/Loom/runtime/loom-daemon.exe");
        let roots = packaged_framework_catalog_roots(executable);

        assert_eq!(
            roots,
            vec![
                PathBuf::from("C:/Loom/runtime/packages/frameworks"),
                PathBuf::from("C:/Loom/packages/frameworks"),
            ]
        );
    }

    #[test]
    fn local_framework_catalog_requires_matching_sha256_sidecar() {
        let root = temp_root();
        let catalog = root.join("catalog");
        std::fs::create_dir_all(&catalog).expect("catalog directory");
        let package_path = catalog.join("process.zip");
        let package = b"independent-framework-package";
        std::fs::write(&package_path, package).expect("framework package");
        let hash = format!("{:x}", Sha256::digest(package));
        std::fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{hash}  process.zip\n"),
        )
        .expect("framework checksum");

        assert_eq!(
            read_framework_package_from_catalog("process", &package_path)
                .expect("verified local framework package"),
            package
        );

        std::fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{}  process.zip\n", "0".repeat(64)),
        )
        .expect("tampered framework checksum");
        let error = read_framework_package_from_catalog("process", &package_path)
            .expect_err("checksum mismatch must fail");
        assert!(error.to_string().contains("checksum mismatch"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_dependencies_defaults_framework_from_execution() {
        let tool = ToolDefinition {
            id: "art-a".to_owned(),
            name: "A".to_owned(),
            description: "d".to_owned(),
            enabled: true,
            execution: ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
            inputs: vec![],
            outputs: vec![],
            params: vec![],
            metadata: None,
        };
        let deps = read_dependencies(&tool);
        assert_eq!(deps.framework.as_deref(), Some("process"));
        assert!(deps.binaries.is_empty());
    }

    #[test]
    fn read_dependencies_parses_metadata_manifest() {
        let tool = ToolDefinition {
            id: "art-b".to_owned(),
            name: "B".to_owned(),
            description: "d".to_owned(),
            enabled: true,
            execution: ToolExecution::FrameworkArt {
                framework: "process".to_owned(),
            },
            inputs: vec![],
            outputs: vec![],
            params: vec![],
            metadata: Some(serde_json::json!({
                "dependencies": {
                    "framework": "process",
                    "binaries": [{ "name": "pingo.exe", "sha256": "abc" }],
                    "arts": ["dep-art-1"]
                }
            })),
        };
        let deps = read_dependencies(&tool);
        assert_eq!(deps.framework.as_deref(), Some("process"));
        assert_eq!(deps.binaries.len(), 1);
        assert_eq!(deps.binaries[0].name, "pingo.exe");
        assert_eq!(deps.arts, vec!["dep-art-1"]);
    }

    fn fake_framework_package_zip(id: &str) -> Vec<u8> {
        fake_framework_package_zip_with_version(id, "0.1.0")
    }

    fn fake_framework_package_zip_with_version(id: &str, version: &str) -> Vec<u8> {
        fake_framework_package_zip_with_identity(id, version, Some("publisher.test"))
    }

    fn fake_framework_package_zip_with_identity(
        id: &str,
        version: &str,
        publisher: Option<&str>,
    ) -> Vec<u8> {
        use std::io::Write;
        let command = match id {
            "process" => "runtime/loom-framework-process.exe",
            "cloud_api" => "runtime/loom-framework-cloud-api.exe",
            "mcp" => "runtime/loom-framework-mcp.exe",
            "workflow" => "runtime/loom-framework-workflow.exe",
            _ => "runtime/loom-framework-third-party.exe",
        };
        let mut manifest = serde_json::json!({
            "id": id,
            "name": format!("{id} test framework"),
            "description": "test framework",
            "version": version,
            "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
            "platforms": [WINDOWS_X64_PLATFORM],
            "entry": {
                "kind": "process",
                "command": command,
                "args": ["--stdio"],
                "processModel": "per_execution"
            },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        });
        if let Some(publisher) = publisher {
            manifest.as_object_mut().expect("manifest object").insert(
                "publisher".to_owned(),
                serde_json::json!({ "id": publisher, "name": publisher }),
            );
        }
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file(FRAMEWORK_MANIFEST_FILE, opts).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.start_file(command, opts).unwrap();
            writer.write_all(b"MZ-fake-framework").unwrap();
            if id == "process" {
                writer.start_file("python-embed/python.exe", opts).unwrap();
                writer.write_all(b"MZ-fake-python").unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    fn signed_framework_package_zip(
        id: &str,
        version: &str,
        publisher: &str,
        key: &loom_plugin_security::SigningKeyDocument,
    ) -> Vec<u8> {
        use std::io::Write;
        let package = temp_root().join("signed-package");
        let command = "runtime/loom-framework-third-party.exe";
        std::fs::create_dir_all(package.join("runtime")).expect("runtime directory");
        let manifest = serde_json::json!({
            "id": id,
            "name": format!("{id} signed test framework"),
            "description": "signed test framework",
            "version": version,
            "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
            "platforms": [WINDOWS_X64_PLATFORM],
            "entry": {
                "kind": "process",
                "command": command,
                "args": ["--stdio"],
                "processModel": "per_execution"
            },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            },
            "publisher": { "id": publisher, "keyId": key.key_id.clone() },
            "signature": {
                "algorithm": "ed25519",
                "keyId": key.key_id.clone(),
                "file": "signature.json"
            }
        });
        std::fs::write(
            package.join(FRAMEWORK_MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .expect("manifest");
        std::fs::write(package.join(command), b"MZ-signed-framework").expect("runtime");
        loom_plugin_security::sign_package(&package, "signature.json", key).expect("sign package");

        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for relative in [FRAMEWORK_MANIFEST_FILE, command, "signature.json"] {
                writer.start_file(relative, options).unwrap();
                writer
                    .write_all(&std::fs::read(package.join(relative)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        std::fs::remove_dir_all(package.parent().expect("package parent")).ok();
        bytes
    }

    #[test]
    fn publisher_namespace_prevents_framework_id_takeover() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let id = "shared-framework";
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
                id,
                "0.1.0",
                Some("publisher.alpha"),
            ))
            .expect("install alpha");
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip_with_identity(
                id,
                "0.1.0",
                Some("publisher.beta"),
            ))
            .expect("install beta");

        assert!(registry.is_installed("publisher.alpha/shared-framework"));
        assert!(registry.is_installed("publisher.beta/shared-framework"));
        assert!(!registry.is_installed(id), "bare id must be ambiguous");
        assert_ne!(
            registry.runtime_dir("publisher.alpha/shared-framework"),
            registry.runtime_dir("publisher.beta/shared-framework")
        );
        let error = registry
            .upgrade_framework_package(
                "publisher.beta/shared-framework",
                &fake_framework_package_zip_with_identity(id, "0.2.0", Some("publisher.alpha")),
            )
            .expect_err("publisher alpha must not upgrade publisher beta");
        assert!(matches!(error, FrameworkError::InvalidPackage { .. }));
        registry
            .uninstall("publisher.alpha/shared-framework")
            .expect("uninstall alpha");
        assert!(registry.is_installed("publisher.beta/shared-framework"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_package_install_uses_package_directory_and_replaces_version() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);

        let first = registry
            .install_with_runtime_fetcher("process", &|_id| {
                Ok(fake_framework_package_zip_with_version("process", "0.1.0"))
            })
            .expect("install first package");
        assert_eq!(first.version.as_deref(), Some("0.1.0"));
        assert!(registry
            .runtime_dir("process")
            .join(FRAMEWORK_MANIFEST_FILE)
            .is_file());

        let second = registry
            .install_with_runtime_fetcher("process", &|_id| {
                Ok(fake_framework_package_zip_with_version("process", "0.2.0"))
            })
            .expect("upgrade package");
        assert_eq!(second.version.as_deref(), Some("0.2.0"));
        assert!(second.ready);
        let rolled_back = registry.rollback("process").expect("rollback package");
        assert_eq!(rolled_back.version.as_deref(), Some("0.1.0"));
        assert!(rolled_back.ready);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_rollback_rejects_tampered_or_revoked_previous_package() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let key = loom_plugin_security::generate_signing_key("release-key");
        registry
            .trust_publisher(PublisherTrustRecord {
                publisher_id: "publisher.rollback".to_owned(),
                key_id: key.key_id.clone(),
                public_key: key.public_key.clone(),
                revoked: false,
            })
            .expect("trust publisher");
        let reference = "publisher.rollback/signed-framework";
        registry
            .install_framework_package_from_zip(&signed_framework_package_zip(
                "signed-framework",
                "1.0.0",
                "publisher.rollback",
                &key,
            ))
            .expect("install v1");
        registry
            .install_framework_package_from_zip(&signed_framework_package_zip(
                "signed-framework",
                "2.0.0",
                "publisher.rollback",
                &key,
            ))
            .expect("install v2");
        registry
            .revoke_publisher("publisher.rollback", &key.key_id)
            .expect("revoke publisher");
        let (ready, detail) = registry.readiness(reference);
        assert!(!ready);
        assert!(detail.contains("信任策略"), "detail={detail}");
        assert!(registry.rollback(reference).is_err());

        let unsigned_root = temp_root();
        let unsigned = FrameworkRegistry::new(&unsigned_root);
        unsigned
            .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
                "process", "1.0.0",
            ))
            .expect("install unsigned v1");
        unsigned
            .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
                "process", "2.0.0",
            ))
            .expect("install unsigned v2");
        let activation = unsigned.activation("process").expect("activation");
        let previous = unsigned_root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process")
            .join(activation.previous.expect("previous"));
        set_framework_tree_readonly(&previous, false).expect("unlock previous");
        std::fs::write(
            previous.join("runtime/loom-framework-process.exe"),
            b"tampered",
        )
        .expect("tamper previous runtime");
        assert!(unsigned.rollback("process").is_err());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&unsigned_root).ok();
    }

    #[test]
    fn framework_package_can_be_disabled_and_reenabled() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip("process"))
            .expect("install package");

        let disabled = registry.disable("process").expect("disable package");
        assert!(disabled.installed);
        assert!(!disabled.enabled);
        assert!(!disabled.ready);
        assert_eq!(disabled.ready_detail, "已禁用");

        let enabled = registry.enable("process").expect("enable package");
        assert!(enabled.installed);
        assert!(enabled.enabled);
        assert!(enabled.ready);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_package_rejects_unsafe_zip_paths() {
        use std::io::Write;
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let mut zip_bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("../escape.txt", opts).unwrap();
            writer.write_all(b"escape").unwrap();
            writer.finish().unwrap();
        }
        let error = registry
            .install_framework_package_from_zip(&zip_bytes)
            .expect_err("unsafe package path must fail");
        assert!(matches!(error, FrameworkError::RuntimeUnpackFailed { .. }));
        assert!(!root.join("escape.txt").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    // Build a complete package zip for process. The package manifest and
    // process entry are required even when the package also carries Python.
    fn fake_python_runtime_zip() -> Vec<u8> {
        fake_framework_package_zip("process")
    }

    #[test]
    fn install_process_downloads_runtime_and_marks_installed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        // process is NOT installed by default and requires its package.
        assert!(!registry.is_installed("process"));

        let status = registry
            .install_with_runtime_fetcher("process", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install process with runtime");
        assert!(status.installed);
        assert!(status.ready, "package entry present => ready");
        assert!(registry.is_installed("process"));
        // The package landed in the active immutable version directory.
        assert!(registry
            .runtime_dir("process")
            .join("python-embed/python.exe")
            .is_file());

        // Uninstall reclaims the runtime dir.
        registry.uninstall("process").expect("uninstall");
        assert!(!registry.is_installed("process"));
        assert!(!root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process")
            .exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn process_readiness_reports_framework_package_detail() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let status = registry
            .install_with_runtime_fetcher("process", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install process with runtime");
        let ready_detail = status.ready_detail.replace('\\', "/");
        assert!(status.ready, "status={status:?}");
        assert!(
            ready_detail.contains("process test framework"),
            "status={status:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_process_download_failure_leaves_it_uninstalled() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let result = registry.install_with_runtime_fetcher("process", &|id| {
            Err(FrameworkError::RuntimeDownloadFailed {
                id: id.to_owned(),
                reason: "network down".to_owned(),
            })
        });
        assert!(result.is_err(), "download failure must error");
        assert!(
            !registry.is_installed("process"),
            "must not be marked installed on failure"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_recovery_restores_previous_activation_and_removes_orphan_target() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip("process"))
            .expect("install framework");
        let package_root = root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process");
        let old = registry.activation("process").expect("activation");
        let orphan_relative = "versions/interrupted-orphan".to_owned();
        let orphan = package_root.join(&orphan_relative);
        std::fs::create_dir_all(&orphan).expect("orphan target");
        std::fs::write(orphan.join("partial.bin"), b"partial").expect("partial payload");
        registry
            .write_lifecycle_journal(
                "process",
                &FrameworkLifecycleJournal {
                    old_activation: Some(old.clone()),
                    next_activation: FrameworkActivationState {
                        active: orphan_relative.clone(),
                        previous: Some(old.active.clone()),
                    },
                    target: orphan_relative,
                },
            )
            .expect("write lifecycle journal");

        let recovered = FrameworkRegistry::new(&root);
        assert_eq!(recovered.activation("process"), Some(old));
        assert!(!orphan.exists());
        assert!(!package_root.join(FRAMEWORK_LIFECYCLE_FILE).exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_recovery_quarantines_unsafe_journal_paths() {
        let root = temp_root();
        let package_root = root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process");
        std::fs::create_dir_all(&package_root).expect("package root");
        let outside = root.join("outside.txt");
        std::fs::write(&outside, b"keep").expect("outside sentinel");
        let journal = serde_json::json!({
            "oldActivation": null,
            "nextActivation": { "active": "../../outside.txt" },
            "target": "../../outside.txt"
        });
        std::fs::write(
            package_root.join(FRAMEWORK_LIFECYCLE_FILE),
            serde_json::to_vec(&journal).unwrap(),
        )
        .expect("unsafe journal");

        let _ = FrameworkRegistry::new(&root);
        assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        assert!(package_root.join("lifecycle.corrupt").is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_readiness_rejects_tampered_lockfile() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip("process"))
            .expect("install framework");
        let locks = root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process")
            .join("locks");
        let lockfile = std::fs::read_dir(&locks)
            .expect("locks")
            .next()
            .expect("lockfile")
            .expect("lock entry")
            .path();
        let mut lock: loom_protocol::PluginLockfile =
            serde_json::from_slice(&std::fs::read(&lockfile).unwrap()).unwrap();
        lock.package_id = "other-framework".to_owned();
        std::fs::write(&lockfile, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

        let (ready, detail) = registry.readiness("process");
        assert!(!ready);
        assert!(detail.contains("锁文件"), "detail={detail}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_version_retention_keeps_active_previous_and_history_limit() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        for version in ["0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0"] {
            registry
                .install_framework_package_from_zip(&fake_framework_package_zip_with_version(
                    "process", version,
                ))
                .unwrap_or_else(|error| panic!("install {version}: {error}"));
        }
        let package_root = root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process");
        let activation = registry.activation("process").expect("activation");
        let versions = std::fs::read_dir(package_root.join(FRAMEWORK_VERSIONS_DIR))
            .expect("versions")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        assert!(versions.len() <= framework_history_limit());
        assert!(package_root.join(&activation.active).is_dir());
        assert!(package_root
            .join(activation.previous.expect("previous version"))
            .is_dir());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_uninstall_tombstone_recovery_restores_or_finishes_from_registry_state() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip("process"))
            .expect("install framework");
        let live = root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("publisher.test")
            .join("process");
        let interrupted = uninstall_tombstone_path(&live, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)
            .expect("tombstone path");
        std::fs::rename(&live, &interrupted).expect("simulate pre-state crash");

        let recovered = FrameworkRegistry::new(&root);
        assert!(recovered.is_installed("process"));
        assert!(live.is_dir());
        assert!(!interrupted.exists());

        let committed = uninstall_tombstone_path(&live, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)
            .expect("tombstone path");
        std::fs::rename(&live, &committed).expect("simulate committed uninstall");
        recovered
            .write_installed(&BTreeMap::new())
            .expect("commit registry removal");
        let finished = FrameworkRegistry::new(&root);
        assert!(!finished.is_installed("process"));
        assert!(!live.exists());
        assert!(!committed.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn flat_framework_directory_is_not_resolved() {
        let root = temp_root();
        let flat = root.join("frameworks").join("flat-framework");
        let active = flat.join("versions").join("1.0.0-flat");
        std::fs::create_dir_all(&active).expect("flat framework directory");
        std::fs::write(
            flat.join(FRAMEWORK_ACTIVE_FILE),
            serde_json::to_vec(&FrameworkActivationState {
                active: "versions/1.0.0-flat".to_owned(),
                previous: None,
            })
            .unwrap(),
        )
        .expect("flat activation");
        std::fs::write(active.join(FRAMEWORK_MANIFEST_FILE), b"{}").expect("flat manifest");

        assert!(
            resolve_framework_package_dir(&root.join("frameworks"), "flat-framework").is_none()
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn permission_modes_audit_by_default_and_strictly_reject_unenforced_capabilities() {
        assert_eq!(
            parse_plugin_permission_mode(None).unwrap(),
            PluginPermissionMode::Audit
        );
        assert_eq!(
            parse_plugin_permission_mode(Some("strict")).unwrap(),
            PluginPermissionMode::Strict
        );
        assert!(parse_plugin_permission_mode(Some("permissive")).is_err());
        let manifest: FrameworkPackageManifest = serde_json::from_value(serde_json::json!({
            "id": "permission-test",
            "name": "Permission Test",
            "description": "permission fixture",
            "version": "1.0.0",
            "publisher": { "id": "publisher.test" },
            "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
            "platforms": [WINDOWS_X64_PLATFORM],
            "entry": {
                "kind": "process",
                "command": "runtime.exe",
                "args": [],
                "processModel": "per_execution"
            },
            "permissions": ["network.connect", "file.read", "process.spawn"],
            "permissionPolicy": {
                "network": { "domains": ["example.com"] },
                "filesystem": { "read": ["inputs"], "write": ["outputs"] },
                "process": { "spawn": true, "maxProcesses": 2 },
                "gpu": true,
                "clipboard": true
            },
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        }))
        .unwrap();
        assert_eq!(
            unsupported_permission_findings(&manifest),
            vec!["direct_network", "arbitrary_filesystem", "gpu", "clipboard"]
        );
        assert!(enforce_framework_permission_mode(&manifest, PluginPermissionMode::Audit).is_ok());
        assert!(
            enforce_framework_permission_mode(&manifest, PluginPermissionMode::Strict).is_err()
        );
    }
}
