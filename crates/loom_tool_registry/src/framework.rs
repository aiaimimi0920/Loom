//! Art execution "frameworks" — the 6 execution kinds treated as first-class,
//! installable capabilities. Each art belongs to exactly one framework; an art
//! can only run when its framework is installed and ready.
//!
//! Unified model (per product decision): all 6 frameworks share the same
//! package-backed installed/ready state. No optional framework is compiled or
//! installed into a fresh control plane by default. A framework becomes
//! available only after its package manifest and runtime have been installed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ToolDefinition, ToolExecution};

const FRAMEWORKS_FILE: &str = "frameworks.json";
const FRAMEWORK_MANIFEST_FILE: &str = "framework.manifest.json";
pub const FRAMEWORK_PROTOCOL_VERSION: &str = "loom.framework.v1";
const WINDOWS_X64_PLATFORM: &str = "windows-x64";
/// Subdir under the control-plane root holding installed framework packages:
/// `<control-plane>/frameworks/<id>/`.
const FRAMEWORK_PACKAGES_DIR: &str = "frameworks";

/// The 6 framework ids, matching `ToolExecution` variants one-to-one.
pub const FRAMEWORK_IDS: [&str; 6] = [
    "cli_wrapper",
    "cloud_api",
    "script",
    "python_art",
    "mcp",
    "workflow",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkPackageManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub protocol_version: String,
    pub platforms: Vec<String>,
    pub entry: FrameworkRuntimeEntry,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub art_execution: FrameworkArtExecutionContract,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkRuntimeEntry {
    pub kind: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkArtExecutionContract {
    pub request_schema: String,
    pub response_schema: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkInstallationState {
    pub version: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkStatus {
    pub id: String,
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
}

/// The framework id that an execution belongs to (same mapping as
/// `execution_type_name`, exposed for readiness checks).
pub fn framework_id_for_execution(execution: &ToolExecution) -> &str {
    match execution {
        ToolExecution::CliWrapper { .. } => "cli_wrapper",
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Script { .. } => "script",
        ToolExecution::PythonArt { .. } => "python_art",
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
        "cli_wrapper" => "命令行框架",
        "cloud_api" => "云 API 框架",
        "script" => "脚本框架",
        "python_art" => "Python Art 框架",
        "mcp" => "MCP 框架",
        "workflow" => "工作流框架",
        _ => "第三方 Art 框架",
    }
}

fn framework_description(id: &str) -> &'static str {
    match id {
        "cli_wrapper" => "调用本地命令行工具（如图像压缩器）处理图像。",
        "cloud_api" => "调用云端 HTTP API 处理图像。",
        "script" => "运行脚本进程处理图像。",
        "python_art" => "运行 Python Art，需要 Python 运行时。",
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
    if !is_valid_framework(id) {
        return (false, "框架 ID 无效".to_owned());
    }

    let Some(root) = runtime_root else {
        return (false, "未提供框架包目录".to_owned());
    };
    let package_dir = root.join(id);
    let manifest_path = package_dir.join(FRAMEWORK_MANIFEST_FILE);
    let manifest = match read_framework_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(detail) => return (false, detail),
    };
    if manifest.id != id {
        return (
            false,
            format!("框架包 ID 不匹配：期望 {id}，实际 {}", manifest.id),
        );
    }
    if manifest.protocol_version != FRAMEWORK_PROTOCOL_VERSION {
        return (
            false,
            format!("不支持的框架协议：{}", manifest.protocol_version),
        );
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
    (
        true,
        format!("已安装框架包 {} {}", manifest.name, manifest.version),
    )
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
    if manifest.version.trim().is_empty() {
        return Err(invalid("version is required".to_owned()));
    }
    if manifest.protocol_version != FRAMEWORK_PROTOCOL_VERSION {
        return Err(invalid(format!(
            "unsupported protocol {}",
            manifest.protocol_version
        )));
    }
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
    if manifest.art_execution.request_schema != "loom.art.execute.v1"
        || manifest.art_execution.response_schema != "loom.art.result.v1"
    {
        return Err(invalid("unsupported Art execution schema".to_owned()));
    }
    Ok(())
}

/// Tracks which framework packages the user has installed, persisted to
/// `<control-plane>/frameworks.json`. `root` also anchors installed framework
/// packages under `<root>/frameworks/<id>/`.
#[derive(Debug, Clone)]
pub struct FrameworkRegistry {
    root: PathBuf,
    path: PathBuf,
}

impl FrameworkRegistry {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            path: root.join(FRAMEWORKS_FILE),
            root,
        }
    }

    /// Directory holding this framework's installed package:
    /// `<root>/frameworks/<id>/`.
    pub fn runtime_dir(&self, id: &str) -> PathBuf {
        self.root.join(FRAMEWORK_PACKAGES_DIR).join(id)
    }

    /// The set of installed framework ids. A persisted state entry is not
    /// enough by itself: the package manifest must also be present.
    pub fn installed_ids(&self) -> BTreeSet<String> {
        self.installation_states()
            .into_iter()
            .filter(|(id, _)| is_valid_framework(id) && self.package_manifest(id).is_some())
            .map(|(id, _)| id)
            .collect()
    }

    /// Whether a specific framework is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.installed_ids().contains(id)
    }

    /// Whether an installed framework package is enabled for execution.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.is_installed(id)
            && self
                .installation_states()
                .get(id)
                .map(|state| state.enabled)
                .unwrap_or(false)
    }

    /// Readiness of a framework, probing its installed package manifest and
    /// process entry. Disabled or uninstalled packages are never ready.
    pub fn readiness(&self, id: &str) -> (bool, String) {
        if !self.is_installed(id) {
            return (false, "未安装".to_owned());
        }
        if !self.is_enabled(id) {
            return (false, "已禁用".to_owned());
        }
        let runtime_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        framework_ready_in(id, Some(&runtime_root))
    }

    /// Full status for the host catalog plus any installed third-party
    /// framework packages.
    pub fn statuses(&self) -> Vec<FrameworkStatus> {
        let mut ids = FRAMEWORK_IDS
            .iter()
            .map(|id| (*id).to_owned())
            .collect::<BTreeSet<_>>();
        ids.extend(self.installed_ids());
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
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        if !self.is_installed(id) {
            return Err(FrameworkError::FrameworkNotInstalled(id.to_owned()));
        }
        self.install_framework_package_zip(zip_bytes, Some(id))
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
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        if !self.is_installed(id) {
            return Err(FrameworkError::FrameworkNotInstalled(id.to_owned()));
        }
        let mut installed = self.installation_states();
        let state = installed
            .get_mut(id)
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        state.enabled = enabled;
        if let Some(manifest) = self.package_manifest(id) {
            state.version = manifest.version;
        }
        self.write_installed(&installed)?;
        Ok(self.status_of(id))
    }

    fn install_framework_package_zip(
        &self,
        zip_bytes: &[u8],
        expected_id: Option<&str>,
    ) -> Result<FrameworkStatus, FrameworkError> {
        let staging = self.staging_dir(expected_id.unwrap_or("package"));
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
                if manifest.id != expected_id {
                    return Err(FrameworkError::InvalidPackage {
                        id: expected_id.to_owned(),
                        reason: format!("manifest id is {}", manifest.id),
                    });
                }
            }
            validate_framework_manifest(&manifest, &staging)?;

            let id = manifest.id.clone();
            let target = self.runtime_dir(&id);
            let packages_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
            fs::create_dir_all(&packages_root)?;
            let backup = self.backup_dir(&id);
            let had_old_package = target.exists();
            if had_old_package {
                if backup.exists() {
                    fs::remove_dir_all(&backup)?;
                }
                fs::rename(&target, &backup)?;
            }
            if let Err(error) = fs::rename(&staging, &target) {
                if had_old_package {
                    let _ = fs::rename(&backup, &target);
                }
                return Err(FrameworkError::Io(error));
            }

            let mut installed = self.installation_states();
            installed.insert(
                id.clone(),
                FrameworkInstallationState {
                    version: manifest.version,
                    enabled: true,
                },
            );
            if let Err(error) = self.write_installed(&installed) {
                let _ = fs::remove_dir_all(&target);
                if had_old_package {
                    let _ = fs::rename(&backup, &target);
                }
                return Err(error);
            }
            if had_old_package {
                fs::remove_dir_all(&backup)?;
            }
            Ok(self.status_of(&id))
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

    fn backup_dir(&self, id: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.root
            .join(format!(".loom-framework-backup-{id}-{nonce}"))
    }

    /// Mark a framework uninstalled and remove any downloaded runtime. Errors on
    /// an unknown id.
    pub fn uninstall(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let mut installed = self.installation_states();
        installed.remove(id);
        self.write_installed(&installed)?;
        // Reclaim disk from the downloaded runtime, if present.
        let runtime_dir = self.runtime_dir(id);
        if runtime_dir.exists() {
            let _ = fs::remove_dir_all(&runtime_dir);
        }
        Ok(self.status_of(id))
    }

    fn status_of(&self, id: &str) -> FrameworkStatus {
        let manifest = self.package_manifest(id);
        let state = self.installation_states().get(id).cloned();
        let installed = state.is_some() && manifest.is_some();
        let enabled = installed && state.as_ref().map(|value| value.enabled).unwrap_or(false);
        let (name, description, version) = match &manifest {
            Some(manifest) => (
                manifest.name.clone(),
                manifest.description.clone(),
                Some(manifest.version.clone()),
            ),
            None => (
                framework_name(id).to_owned(),
                framework_description(id).to_owned(),
                None,
            ),
        };
        let (ready, ready_detail) = if !installed {
            (false, "未安装".to_owned())
        } else if !enabled {
            (false, "已禁用".to_owned())
        } else {
            self.readiness(id)
        };
        FrameworkStatus {
            id: id.to_owned(),
            name,
            description,
            installed,
            enabled,
            ready,
            ready_detail,
            version,
            runtime_dir: installed.then(|| self.runtime_dir(id)),
        }
    }

    fn package_manifest(&self, id: &str) -> Option<FrameworkPackageManifest> {
        let manifest =
            read_framework_manifest(&self.runtime_dir(id).join(FRAMEWORK_MANIFEST_FILE)).ok()?;
        (manifest.id == id
            && manifest.protocol_version == FRAMEWORK_PROTOCOL_VERSION
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
        if let Ok(states) =
            serde_json::from_str::<BTreeMap<String, FrameworkInstallationState>>(&text)
        {
            return states;
        }
        // Read the pre-pluginization array once so an old control plane does
        // not crash. Entries still remain unavailable until package manifests
        // are installed, so this cannot silently restore built-in runtimes.
        serde_json::from_str::<Vec<String>>(&text)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| is_valid_framework(id))
            .map(|id| {
                (
                    id,
                    FrameworkInstallationState {
                        version: String::new(),
                        enabled: true,
                    },
                )
            })
            .collect()
    }

    fn write_installed(
        &self,
        installed: &BTreeMap<String, FrameworkInstallationState>,
    ) -> Result<(), FrameworkError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(installed)?;
        fs::write(&self.path, text)?;
        Ok(())
    }
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

/// Download a framework runtime zip from the configured store URL.
fn default_runtime_fetcher(id: &str) -> Result<Vec<u8>, FrameworkError> {
    let url = framework_runtime_url(id)
        .ok_or_else(|| FrameworkError::RuntimeSourceMissing { id: id.to_owned() })?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("Loom/0.1 Framework Runtime Fetch")
        // Bypass any (possibly dead) system proxy; the store is typically local.
        .no_proxy()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|error| FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: error.to_string(),
        })?;
    let bytes = client
        .get(&url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.bytes())
        .map_err(|error| FrameworkError::RuntimeDownloadFailed {
            id: id.to_owned(),
            reason: format!("{url}: {error}"),
        })?;
    Ok(bytes.to_vec())
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
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|error| fail(error.to_string()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| fail(error.to_string()))?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(fail(format!(
                "unsafe path in runtime zip: {}",
                entry.name()
            )));
        };
        let out_path = runtime_dir.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|error| fail(error.to_string()))?;
        fs::write(&out_path, &buf)?;
    }
    Ok(())
}

fn is_valid_framework(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
        && !id.ends_with('.')
}

#[derive(Debug, thiserror::Error)]
pub enum FrameworkError {
    #[error("unknown framework `{0}`")]
    UnknownFramework(String),
    #[error("framework `{0}` is not installed")]
    FrameworkNotInstalled(String),
    #[error("invalid framework package `{id}`: {reason}")]
    InvalidPackage { id: String, reason: String },
    #[error("framework `{id}` has no configured runtime download source (set LOOM_ART_STORE_URL or LOOM_FRAMEWORK_RUNTIME_URL)")]
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
        assert!(!installed.contains("python_art"));
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
    fn statuses_cover_all_six_frameworks() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let statuses = registry.statuses();
        assert_eq!(statuses.len(), 6);
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
    fn read_dependencies_defaults_framework_from_execution() {
        let tool = ToolDefinition {
            id: "art-a".to_owned(),
            name: "A".to_owned(),
            description: "d".to_owned(),
            enabled: true,
            execution: ToolExecution::CliWrapper {
                command: "pingo".to_owned(),
                args: vec![],
            },
            inputs: vec![],
            outputs: vec![],
            params: vec![],
            metadata: None,
        };
        let deps = read_dependencies(&tool);
        assert_eq!(deps.framework.as_deref(), Some("cli_wrapper"));
        assert!(deps.binaries.is_empty());
    }

    #[test]
    fn read_dependencies_parses_metadata_manifest() {
        let tool = ToolDefinition {
            id: "art-b".to_owned(),
            name: "B".to_owned(),
            description: "d".to_owned(),
            enabled: true,
            execution: ToolExecution::CliWrapper {
                command: "pingo".to_owned(),
                args: vec![],
            },
            inputs: vec![],
            outputs: vec![],
            params: vec![],
            metadata: Some(serde_json::json!({
                "dependencies": {
                    "framework": "cli_wrapper",
                    "binaries": [{ "name": "pingo.exe", "sha256": "abc" }],
                    "arts": ["dep-art-1"]
                }
            })),
        };
        let deps = read_dependencies(&tool);
        assert_eq!(deps.framework.as_deref(), Some("cli_wrapper"));
        assert_eq!(deps.binaries.len(), 1);
        assert_eq!(deps.binaries[0].name, "pingo.exe");
        assert_eq!(deps.arts, vec!["dep-art-1"]);
    }

    fn fake_framework_package_zip(id: &str) -> Vec<u8> {
        fake_framework_package_zip_with_version(id, "0.1.0")
    }

    fn fake_framework_package_zip_with_version(id: &str, version: &str) -> Vec<u8> {
        use std::io::Write;
        let command = match id {
            "cli_wrapper" => "runtime/loom-framework-cli-wrapper.exe",
            "cloud_api" => "runtime/loom-framework-cloud-api.exe",
            "script" => "runtime/loom-framework-script.exe",
            "python_art" => "runtime/loom-framework-python-art.exe",
            "mcp" => "runtime/loom-framework-mcp.exe",
            "workflow" => "runtime/loom-framework-workflow.exe",
            _ => "runtime/loom-framework-third-party.exe",
        };
        let manifest = serde_json::json!({
            "id": id,
            "name": format!("{id} test framework"),
            "description": "test framework",
            "version": version,
            "protocolVersion": FRAMEWORK_PROTOCOL_VERSION,
            "platforms": [WINDOWS_X64_PLATFORM],
            "entry": { "kind": "process", "command": command, "args": ["--stdio"] },
            "permissions": ["process.spawn"],
            "artExecution": {
                "requestSchema": "loom.art.execute.v1",
                "responseSchema": "loom.art.result.v1"
            }
        });
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
            if id == "python_art" {
                writer.start_file("python-embed/python.exe", opts).unwrap();
                writer.write_all(b"MZ-fake-python").unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn framework_package_install_uses_package_directory_and_replaces_version() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);

        let first = registry
            .install_with_runtime_fetcher("script", &|_id| {
                Ok(fake_framework_package_zip_with_version("script", "0.1.0"))
            })
            .expect("install first package");
        assert_eq!(first.version.as_deref(), Some("0.1.0"));
        assert!(root
            .join("frameworks")
            .join("script")
            .join(FRAMEWORK_MANIFEST_FILE)
            .is_file());

        let second = registry
            .install_with_runtime_fetcher("script", &|_id| {
                Ok(fake_framework_package_zip_with_version("script", "0.2.0"))
            })
            .expect("upgrade package");
        assert_eq!(second.version.as_deref(), Some("0.2.0"));
        assert!(second.ready);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn framework_package_can_be_disabled_and_reenabled() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        registry
            .install_framework_package_from_zip(&fake_framework_package_zip("script"))
            .expect("install package");

        let disabled = registry.disable("script").expect("disable package");
        assert!(disabled.installed);
        assert!(!disabled.enabled);
        assert!(!disabled.ready);
        assert_eq!(disabled.ready_detail, "已禁用");

        let enabled = registry.enable("script").expect("enable package");
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

    // Build a complete package zip for python_art. The package manifest and
    // process entry are required even when the package also carries Python.
    fn fake_python_runtime_zip() -> Vec<u8> {
        fake_framework_package_zip("python_art")
    }

    #[test]
    fn install_python_art_downloads_runtime_and_marks_installed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        // python_art is NOT installed by default and requires its package.
        assert!(!registry.is_installed("python_art"));

        let status = registry
            .install_with_runtime_fetcher("python_art", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install python_art with runtime");
        assert!(status.installed);
        assert!(status.ready, "package entry present => ready");
        assert!(registry.is_installed("python_art"));
        // The package landed under frameworks/python_art/.
        assert!(root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("python_art")
            .join("python-embed/python.exe")
            .is_file());

        // Uninstall reclaims the runtime dir.
        registry.uninstall("python_art").expect("uninstall");
        assert!(!registry.is_installed("python_art"));
        assert!(!root
            .join(FRAMEWORK_PACKAGES_DIR)
            .join("python_art")
            .exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn python_art_readiness_reports_framework_package_detail() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let status = registry
            .install_with_runtime_fetcher("python_art", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install python_art with runtime");
        let ready_detail = status.ready_detail.replace('\\', "/");
        assert!(status.ready, "status={status:?}");
        assert!(
            ready_detail.contains("python_art test framework"),
            "status={status:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_python_art_download_failure_leaves_it_uninstalled() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let result = registry.install_with_runtime_fetcher("python_art", &|id| {
            Err(FrameworkError::RuntimeDownloadFailed {
                id: id.to_owned(),
                reason: "network down".to_owned(),
            })
        });
        assert!(result.is_err(), "download failure must error");
        assert!(
            !registry.is_installed("python_art"),
            "must not be marked installed on failure"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
