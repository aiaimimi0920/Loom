//! Bundled framework fallback, version lookup, and verified package intake.

use super::*;

pub(super) fn install_packaged_framework_from_exe(
    base_url: &str,
    id: &str,
    current_exe: &Path,
) -> Result<Value, String> {
    let id = id.trim();
    if id.is_empty() || id.len() > 256 {
        return Err("框架 ID 无效。".to_string());
    }

    let install_path = format!("/v1/frameworks/{}/install", percent_encode_path_segment(id));
    let install_timeout = Duration::from_secs(60);
    let original_error = match http_post_json_with_timeout(
        base_url,
        &install_path,
        &serde_json::json!({}),
        install_timeout,
    ) {
        Ok(response) => return Ok(response),
        Err(error) => error,
    };

    if !framework_source_is_missing(&original_error) || !OFFICIAL_FRAMEWORK_IDS.contains(&id) {
        return Err(original_error);
    }

    let candidates = packaged_framework_package_candidates(current_exe, id);
    let package_path = candidates
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            let searched = candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("; ");
            format!("{original_error}；当前 Loom 包内未找到框架安装包：{searched}")
        })?;
    let package = read_verified_framework_package(id, package_path)?;
    let response = http_post_json_with_timeout(
        base_url,
        "/v1/frameworks/install",
        &serde_json::json!({ "zipBase64": base64_encode(&package) }),
        install_timeout,
    )?;
    Ok(response)
}

pub(super) fn upgrade_packaged_framework_from_exe(
    base_url: &str,
    id: &str,
    current_exe: &Path,
) -> Result<Value, String> {
    validate_packaged_framework_id(id)?;
    let package_path = packaged_framework_package_candidates(current_exe, id)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("当前 Loom 包内未找到框架升级包：{id}"))?;
    let package = read_verified_framework_package(id, &package_path)?;
    http_post_json_with_timeout(
        base_url,
        &format!("/v1/frameworks/{}/upgrade", percent_encode_path_segment(id)),
        &serde_json::json!({ "zipBase64": base64_encode(&package) }),
        Duration::from_secs(120),
    )
}

pub(super) fn packaged_framework_version(current_exe: &Path, id: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join("summary.json"));
    }
    if let Some(parent) = current_exe.parent() {
        candidates.push(
            parent
                .join("packages")
                .join("frameworks")
                .join("summary.json"),
        );
    }
    candidates.into_iter().find_map(|path| {
        let bytes =
            read_bounded_regular_file(&path, MAX_PACKAGE_CATALOG_BYTES, "打包框架目录").ok()?;
        let catalog: PackagedFrameworkCatalog = serde_json::from_slice(&bytes).ok()?;
        catalog
            .frameworks
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.version)
    })
}

pub(super) fn framework_source_is_missing(error: &str) -> bool {
    error.contains("no configured runtime download source")
        || error.contains("no available package source")
}

fn validate_packaged_framework_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 256
        || id.contains("..")
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("框架 ID 无效。".to_owned());
    }
    Ok(())
}

pub(super) fn packaged_framework_package_candidates(current_exe: &Path, id: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(FRAMEWORK_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join(format!("{id}.zip")));
    }
    let packaged = packaged_framework_package_path(current_exe, id);
    if !candidates.iter().any(|candidate| candidate == &packaged) {
        candidates.push(packaged);
    }
    candidates
}

pub(super) fn packaged_framework_package_path(current_exe: &Path, id: &str) -> PathBuf {
    current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("packages")
        .join("frameworks")
        .join(format!("{id}.zip"))
}

pub(super) fn read_verified_framework_package(
    id: &str,
    package_path: &Path,
) -> Result<Vec<u8>, String> {
    read_verified_package("框架", id, package_path, FRAMEWORK_PACKAGE_MAX_BYTES)
}

pub(super) fn read_verified_art_package(id: &str, package_path: &Path) -> Result<Vec<u8>, String> {
    read_verified_package("Art", id, package_path, ART_PACKAGE_MAX_BYTES)
}

pub(super) fn read_verified_mcp_server_package(
    id: &str,
    package_path: &Path,
) -> Result<Vec<u8>, String> {
    read_verified_package("MCP 服务", id, package_path, MCP_SERVER_PACKAGE_MAX_BYTES)
}

pub(super) fn read_verified_package(
    kind: &str,
    id: &str,
    package_path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let subject = format!("{kind} `{id}` 安装包");
    let package = read_bounded_regular_file(package_path, max_bytes, &subject)?;
    let checksum_path = package_path.with_extension("zip.sha256");
    let checksum =
        read_bounded_utf8_file(&checksum_path, 4096, &format!("{kind} `{id}` 校验文件"))?;
    let mut fields = checksum.split_whitespace();
    let expected_hash = fields
        .next()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let expected_name = fields.next();
    let package_name = package_path.file_name().and_then(|name| name.to_str());
    if expected_hash.is_none() || expected_name != package_name || fields.next().is_some() {
        return Err(format!(
            "{kind} `{id}` 校验文件格式无效：{}",
            checksum_path.display()
        ));
    }
    let actual_hash = format!("{:x}", Sha256::digest(&package));
    if !actual_hash.eq_ignore_ascii_case(expected_hash.expect("validated checksum hash")) {
        return Err(format!(
            "{kind} `{id}` 安装包 SHA-256 不匹配：{}",
            package_path.display()
        ));
    }
    Ok(package)
}
