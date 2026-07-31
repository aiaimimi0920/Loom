//! Art execution "frameworks" — the 6 execution kinds treated as first-class,
//! installable capabilities. Each art belongs to exactly one framework; an art
//! can only run when its framework is installed and ready.
//!
//! Unified model (per product decision): all 6 frameworks share the same
//! installed/ready state. The 4 built-in kinds (cli_wrapper/cloud_api/script/
//! workflow) need no external runtime, so they default to installed; python_art
//! and mcp require an explicit install and a readiness probe.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ToolDefinition, ToolExecution};

const FRAMEWORKS_FILE: &str = "frameworks.json";
/// Subdir under the control-plane root holding installed framework runtimes:
/// `<control-plane>/framework-runtimes/<id>/`.
const FRAMEWORK_RUNTIMES_DIR: &str = "framework-runtimes";

/// The 6 framework ids, matching `ToolExecution` variants one-to-one.
pub const FRAMEWORK_IDS: [&str; 6] = [
    "cli_wrapper",
    "cloud_api",
    "script",
    "python_art",
    "mcp",
    "workflow",
];

/// Framework ids that ship built into Loom and need no external runtime, so they
/// are installed by default.
const BUILT_IN_FRAMEWORKS: [&str; 4] = ["cli_wrapper", "cloud_api", "script", "workflow"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Whether the user has installed/enabled this framework.
    pub installed: bool,
    /// Whether the framework's runtime is actually available (probed).
    pub ready: bool,
    pub ready_detail: String,
}

/// The framework id that an execution belongs to (same mapping as
/// `execution_type_name`, exposed for readiness checks).
pub fn framework_id_for_execution(execution: &ToolExecution) -> &'static str {
    match execution {
        ToolExecution::CliWrapper { .. } => "cli_wrapper",
        ToolExecution::CloudApi { .. } => "cloud_api",
        ToolExecution::Script { .. } => "script",
        ToolExecution::PythonArt { .. } => "python_art",
        ToolExecution::Mcp { .. } => "mcp",
        ToolExecution::Workflow { .. } => "workflow",
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
        _ => "未知框架",
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
        _ => "",
    }
}

/// Whether a framework needs an external runtime installed before it is ready.
/// The 4 built-in kinds and mcp ship inside Loom; only python_art needs a
/// downloaded runtime (an embedded Python + launcher).
pub fn framework_needs_runtime(id: &str) -> bool {
    id == "python_art"
}

/// The relative path, inside a framework's runtime dir, that must exist for the
/// framework to be considered installed via a downloaded runtime. For
/// python_art this is the embedded interpreter.
fn framework_runtime_marker(id: &str) -> Option<&'static str> {
    match id {
        "python_art" => Some("python-embed/python.exe"),
        _ => None,
    }
}

/// Probe whether a framework's runtime is available. Built-in frameworks are
/// always ready; python_art is ready when a runtime was installed under
/// `runtime_root` (or a Python is otherwise discoverable); mcp uses the
/// built-in stdio client. `runtime_root` is `<control-plane>/framework-runtimes`.
pub fn framework_ready(id: &str) -> (bool, String) {
    framework_ready_in(id, None)
}

/// Readiness probe that also checks a control-plane runtime dir. `runtime_root`
/// points at `<control-plane>/framework-runtimes`; the framework's own runtime
/// lives at `<runtime_root>/<id>/`.
pub fn framework_ready_in(id: &str, runtime_root: Option<&Path>) -> (bool, String) {
    match id {
        "cli_wrapper" | "cloud_api" | "script" | "workflow" => {
            (true, "内置能力，随 Loom 就绪".to_owned())
        }
        "mcp" => (true, "内置 MCP 客户端".to_owned()),
        "python_art" => {
            // Prefer a runtime installed into the control-plane dir.
            if let Some(root) = runtime_root {
                if let Some(marker) = framework_runtime_marker(id) {
                    let installed = root.join(id).join(marker);
                    if installed.is_file() {
                        return (true, format!("已安装运行时：{}", installed.display()));
                    }
                }
            }
            match probe_python() {
                Some(path) => (true, format!("已找到 Python：{path}")),
                None => (false, "未安装 Python 运行时（点击安装以下载）".to_owned()),
            }
        }
        _ => (false, "未知框架".to_owned()),
    }
}

/// Find a usable Python executable: prefer a bundled `python-embed` next to the
/// exe or cwd, else fall back to `python --version` on PATH.
fn probe_python() -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("bin").join("python-embed").join("python.exe"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("bin").join("python-embed").join("python.exe"));
        candidates.push(
            cwd.join("Loom")
                .join("resources")
                .join("python-embed")
                .join("python.exe"),
        );
    }
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.display().to_string());
        }
    }
    // Fall back to PATH python.
    let python = if cfg!(windows) { "python" } else { "python3" };
    if std::process::Command::new(python)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        return Some(python.to_owned());
    }
    None
}

/// Tracks which frameworks the user has installed, persisted to
/// `<control-plane>/frameworks.json`. `root` also anchors installed framework
/// runtimes under `<root>/framework-runtimes/<id>/`.
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

    /// Directory holding this framework's installed runtime (if any):
    /// `<root>/framework-runtimes/<id>/`.
    pub fn runtime_dir(&self, id: &str) -> PathBuf {
        self.root.join(FRAMEWORK_RUNTIMES_DIR).join(id)
    }

    /// The set of installed framework ids. If no file exists yet, the built-in
    /// frameworks are considered installed by default.
    pub fn installed_ids(&self) -> BTreeSet<String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str::<Vec<String>>(&text)
                .map(|ids| {
                    ids.into_iter()
                        .filter(|id| is_valid_framework(id))
                        .collect()
                })
                .unwrap_or_else(|_| default_installed()),
            Err(_) => default_installed(),
        }
    }

    /// Whether a specific framework is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.installed_ids().contains(id)
    }

    /// Readiness of a framework, probing this registry's installed runtime dir
    /// first (so a downloaded python-embed counts) before falling back to the
    /// ambient probe.
    pub fn readiness(&self, id: &str) -> (bool, String) {
        let runtime_root = self.root.join(FRAMEWORK_RUNTIMES_DIR);
        framework_ready_in(id, Some(&runtime_root))
    }

    /// Full status for all 6 frameworks (installed + readiness probe).
    pub fn statuses(&self) -> Vec<FrameworkStatus> {
        let installed = self.installed_ids();
        FRAMEWORK_IDS
            .iter()
            .map(|&id| {
                let is_installed = installed.contains(id);
                let (ready, ready_detail) = if is_installed {
                    self.readiness(id)
                } else {
                    (false, "未安装".to_owned())
                };
                FrameworkStatus {
                    id: id.to_owned(),
                    name: framework_name(id).to_owned(),
                    description: framework_description(id).to_owned(),
                    installed: is_installed,
                    ready,
                    ready_detail,
                }
            })
            .collect()
    }

    /// Install a framework: if it declares a downloadable runtime, fetch and
    /// unpack it into the runtime dir first; only mark it installed once that
    /// succeeds. Built-in frameworks (no runtime) just flip the flag. Errors on
    /// an unknown id or a failed runtime download.
    pub fn install(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.install_with_runtime_fetcher(id, &default_runtime_fetcher)
    }

    /// Install variant with an injectable runtime fetcher (a closure returning
    /// the runtime zip bytes for a framework id). Testable without the network.
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
        // Frameworks that need an external runtime download it now; failure
        // leaves the framework un-installed (the flag is not flipped).
        if framework_needs_runtime(id) {
            let runtime_dir = self.runtime_dir(id);
            // Only a runtime already downloaded into the control-plane counts as
            // "already installed" — a Python that merely happens to be on the
            // machine's PATH must not skip the download (方向 A: install always
            // provisions the runtime into Loom's own dir).
            let already_installed = framework_runtime_marker(id)
                .map(|marker| runtime_dir.join(marker).is_file())
                .unwrap_or(false);
            if !already_installed {
                let zip = fetch_runtime(id)?;
                unpack_runtime_zip(id, &zip, &runtime_dir)?;
                let (ready, detail) = framework_ready_in(id, Some(&runtime_dir));
                if !ready {
                    // Downloaded but still not runnable — do not mark installed.
                    let _ = fs::remove_dir_all(&runtime_dir);
                    return Err(FrameworkError::RuntimeUnavailable {
                        id: id.to_owned(),
                        reason: detail,
                    });
                }
            }
        }
        let mut installed = self.installed_ids();
        installed.insert(id.to_owned());
        self.write_installed(&installed)?;
        Ok(self.status_of(id))
    }

    /// Mark a framework uninstalled and remove any downloaded runtime. Errors on
    /// an unknown id.
    pub fn uninstall(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let mut installed = self.installed_ids();
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
        self.statuses()
            .into_iter()
            .find(|status| status.id == id)
            .unwrap_or(FrameworkStatus {
                id: id.to_owned(),
                name: framework_name(id).to_owned(),
                description: framework_description(id).to_owned(),
                installed: false,
                ready: false,
                ready_detail: "未知框架".to_owned(),
            })
    }

    fn write_installed(&self, installed: &BTreeSet<String>) -> Result<(), FrameworkError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let ids: Vec<&String> = installed.iter().collect();
        let text = serde_json::to_string_pretty(&ids)?;
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
    FRAMEWORK_IDS.contains(&id)
}

fn default_installed() -> BTreeSet<String> {
    BUILT_IN_FRAMEWORKS.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, thiserror::Error)]
pub enum FrameworkError {
    #[error("unknown framework `{0}`")]
    UnknownFramework(String),
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

    fn temp_root() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "loom-frameworks-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn defaults_built_in_frameworks_installed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let installed = registry.installed_ids();
        assert!(installed.contains("cli_wrapper"));
        assert!(installed.contains("workflow"));
        // python_art / mcp are not installed by default.
        assert!(!installed.contains("python_art"));
        assert!(!installed.contains("mcp"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_and_uninstall_roundtrip() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let status = registry.install("mcp").expect("install mcp");
        assert!(status.installed);
        assert!(status.ready, "mcp is built-in ready once installed");
        assert!(registry.is_installed("mcp"));

        registry.uninstall("mcp").expect("uninstall mcp");
        assert!(!registry.is_installed("mcp"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn statuses_cover_all_six_frameworks() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let statuses = registry.statuses();
        assert_eq!(statuses.len(), 6);
        for id in FRAMEWORK_IDS {
            assert!(statuses.iter().any(|s| s.id == id));
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
    fn built_in_frameworks_ready_when_installed() {
        for id in BUILT_IN_FRAMEWORKS {
            let (ready, _) = framework_ready(id);
            assert!(ready, "{id} should be ready");
        }
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

    // Build a minimal zip whose single entry is `python-embed/python.exe` (the
    // python_art runtime marker), so a fetcher can hand it to `install`.
    fn fake_python_runtime_zip() -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file("python-embed/python.exe", opts).unwrap();
            writer.write_all(b"MZ-fake-python").unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn install_python_art_downloads_runtime_and_marks_installed() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        // python_art is NOT installed by default and needs a runtime.
        assert!(!registry.is_installed("python_art"));

        let status = registry
            .install_with_runtime_fetcher("python_art", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install python_art with runtime");
        assert!(status.installed);
        assert!(status.ready, "runtime marker present => ready");
        assert!(registry.is_installed("python_art"));
        // The runtime landed under framework-runtimes/python_art/.
        assert!(root
            .join(FRAMEWORK_RUNTIMES_DIR)
            .join("python_art")
            .join("python-embed/python.exe")
            .is_file());

        // Uninstall reclaims the runtime dir.
        registry.uninstall("python_art").expect("uninstall");
        assert!(!registry.is_installed("python_art"));
        assert!(!root
            .join(FRAMEWORK_RUNTIMES_DIR)
            .join("python_art")
            .exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn python_art_readiness_prefers_framework_runtime_marker_detail() {
        let root = temp_root();
        let registry = FrameworkRegistry::new(&root);
        let status = registry
            .install_with_runtime_fetcher("python_art", &|_id| Ok(fake_python_runtime_zip()))
            .expect("install python_art with runtime");
        let expected_marker = root
            .join(FRAMEWORK_RUNTIMES_DIR)
            .join("python_art")
            .join("python-embed")
            .join("python.exe");
        let expected_marker = expected_marker.display().to_string().replace('\\', "/");
        let ready_detail = status.ready_detail.replace('\\', "/");
        assert!(status.ready, "status={status:?}");
        assert!(ready_detail.contains(&expected_marker), "status={status:?}");
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
