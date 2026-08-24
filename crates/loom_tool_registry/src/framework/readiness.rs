//! Runtime readiness probes and installed-package resolution.
use super::*;

pub(super) fn framework_name(id: &str) -> &'static str {
    match id {
        "process" => "脚本",
        "cloud_api" => "云端",
        "mcp" => "MCP",
        "workflow" => "流程",
        _ => "第三方 Art 框架",
    }
}

pub(super) fn framework_description(id: &str) -> &'static str {
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
        Ok(package_dir) => package_dir,
        Err(FrameworkError::FrameworkNotInstalled(_) | FrameworkError::UnknownFramework(_)) => {
            return (false, "未找到活动框架包".to_owned())
        }
        Err(error) => return (false, format!("框架包目录解析失败：{error}")),
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
    if let Err(reason) = loom_process::executable_path_within(&package_dir, command_path) {
        return (false, format!("框架入口不安全或不存在：{reason}"));
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
    // The persisted policy, not just the environment override: an operator who sets
    // `require-trusted` in the trust store must not have framework packages — the components that
    // execute Art code with the highest privilege — fall back to allowing unsigned ones.
    if let Err(error) = trust_store.effective_policy().enforce(trust_status) {
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

/// Locates the active package directory of `id` under `runtime_root`.
///
/// Failures that belong to one candidate — an unreadable directory entry, a sibling package with a
/// damaged manifest — skip that candidate instead of ending the scan, so one broken publisher cannot
/// make a healthy framework unresolvable. A local id carried by more than one publisher is reported
/// as ambiguous rather than as missing, because the two cases need different operator action.
pub fn resolve_framework_package_dir(
    runtime_root: &Path,
    id: &str,
) -> Result<PathBuf, FrameworkError> {
    if !is_valid_framework_reference(id) {
        return Err(FrameworkError::UnknownFramework(id.to_owned()));
    }
    if id.contains('/') {
        return framework_storage_path(id)
            .map(|path| runtime_root.join(path))
            .as_deref()
            .and_then(resolve_framework_package_root)
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()));
    }
    let publishers = match fs::read_dir(runtime_root) {
        Ok(publishers) => publishers,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(FrameworkError::FrameworkNotInstalled(id.to_owned()))
        }
        Err(error) => return Err(error.into()),
    };
    let mut matches = Vec::new();
    for publisher in publishers {
        let Ok(publisher) = publisher else {
            continue;
        };
        if !is_directory_without_links(&publisher.path()).unwrap_or(false) {
            continue;
        }
        let candidate = publisher.path().join(id);
        if let Some(package) = resolve_framework_package_root(&candidate) {
            let Ok(manifest) = read_framework_manifest(&package.join(FRAMEWORK_MANIFEST_FILE))
            else {
                continue;
            };
            if manifest.id == id {
                matches.push(package);
            }
        }
    }
    match matches.len() {
        0 => Err(FrameworkError::FrameworkNotInstalled(id.to_owned())),
        1 => Ok(matches.remove(0)),
        _ => Err(FrameworkError::AmbiguousFramework(id.to_owned())),
    }
}

pub(super) fn resolve_framework_package_root(package_root: &Path) -> Option<PathBuf> {
    if !is_directory_without_links(package_root).ok()?
        || !is_directory_without_links(package_root.parent()?).ok()?
    {
        return None;
    }
    let active_path = package_root.join(FRAMEWORK_ACTIVE_FILE);
    if is_file_without_links(&active_path).ok()? {
        let activation: FrameworkActivationState = serde_json::from_slice(
            &read_bounded_file(&active_path, FRAMEWORK_METADATA_MAX_BYTES).ok()?,
        )
        .ok()?;
        if !is_safe_framework_version_path(&activation.active) {
            return None;
        }
        let versions_root = package_root.join(FRAMEWORK_VERSIONS_DIR);
        if !is_directory_without_links(&versions_root).ok()? {
            return None;
        }
        let relative = Path::new(&activation.active);
        let resolved = package_root.join(relative);
        return (is_directory_without_links(&resolved).ok()?
            && is_file_without_links(&resolved.join(FRAMEWORK_MANIFEST_FILE)).ok()?)
        .then_some(resolved);
    }
    None
}

pub(super) fn framework_storage_path(reference: &str) -> Option<PathBuf> {
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

pub(super) fn framework_local_id(reference: &str) -> &str {
    reference
        .rsplit_once('/')
        .map(|(_, id)| id)
        .unwrap_or(reference)
}

pub(super) fn read_framework_manifest(path: &Path) -> Result<FrameworkPackageManifest, String> {
    let bytes = read_bounded_file(path, FRAMEWORK_METADATA_MAX_BYTES)
        .map_err(|error| format!("无法读取框架包清单 {}：{error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("框架包清单无效 {}：{error}", path.display()))
}

pub(super) fn validate_framework_manifest(
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
    loom_process::executable_path_within(package_dir, command_path)
        .map_err(|reason| invalid(format!("entry.command is unsafe or unavailable: {reason}")))?;
    Ok(())
}
