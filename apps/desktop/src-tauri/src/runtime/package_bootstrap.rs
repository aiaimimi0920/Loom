//! Bundled MCP and Art catalog validation, repair, and installation.

use super::*;

pub(super) fn packaged_art_catalog_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(ART_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join("summary.json"));
    }
    let packaged = current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("packages")
        .join("arts")
        .join("summary.json");
    if !candidates.iter().any(|candidate| candidate == &packaged) {
        candidates.push(packaged);
    }
    candidates
}

pub(super) fn packaged_mcp_server_catalog_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os(MCP_SERVER_PACKAGE_CATALOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        candidates.push(root.join("summary.json"));
    }
    let packaged = current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("packages")
        .join("mcp-servers")
        .join("summary.json");
    if !candidates.iter().any(|candidate| candidate == &packaged) {
        candidates.push(packaged);
    }
    candidates
}

pub(super) fn validate_packaged_mcp_server_entry(
    entry: &PackagedMcpServerCatalogEntry,
) -> Result<(), String> {
    if entry.id.is_empty()
        || entry.id.len() > 128
        || entry.id.contains("..")
        || !entry
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("打包 MCP 服务 ID 无效：{}", entry.id));
    }
    let Some((publisher, package_id)) = entry.qualified_id.split_once('/') else {
        return Err(format!("打包 MCP 包 ID 无效：{}", entry.qualified_id));
    };
    if publisher.is_empty()
        || package_id != entry.id
        || !publisher
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("打包 MCP 包 ID 无效：{}", entry.qualified_id));
    }
    if entry.version.is_empty() || entry.zip != format!("{}.zip", entry.id) {
        return Err(format!("打包 MCP 服务 `{}` 的版本或 ZIP 无效。", entry.id));
    }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("打包 MCP 服务 `{}` 的 SHA-256 无效。", entry.id));
    }
    Ok(())
}

pub(super) fn bootstrap_packaged_mcp_servers(
    base_url: &str,
    current_exe: &Path,
    control_plane_root: &Path,
) -> Result<Vec<String>, String> {
    let Some(catalog_path) = packaged_mcp_server_catalog_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(Vec::new());
    };
    let catalog_bytes = read_bounded_regular_file(
        &catalog_path,
        MAX_PACKAGE_CATALOG_BYTES,
        "打包 MCP 服务目录",
    )?;
    let catalog_hash = format!("{:x}", Sha256::digest(&catalog_bytes));
    // The MCP installer changed its immutable version-directory suffix from the
    // legacy 12-hex prefix to 32 hex characters in phase 79.  A plain catalog
    // hash marker cannot distinguish an already-processed catalog from a
    // control plane populated by the pre-hardening installer, so keep a
    // migration generation in the marker.  This forces one repair install on
    // the first desktop start after that format change while preserving the
    // existing "do not restore an intentionally uninstalled server" behavior
    // for subsequent starts.
    // v3 also repairs Art lockfiles after an MCP catalog refresh.  The marker
    // generation is intentionally bumped so existing control planes receive
    // one deterministic MCP + dependent-Art reinstall.
    let migration_marker = format!("mcp-v3:{catalog_hash}");
    let marker_path = control_plane_root
        .join("migrations")
        .join("packaged-mcp-servers.sha256");
    if read_bounded_utf8_file(&marker_path, 128, "打包 MCP 服务迁移标记")
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(&migration_marker))
    {
        return Ok(Vec::new());
    }
    let catalog: PackagedMcpServerCatalog =
        serde_json::from_slice(&catalog_bytes).map_err(|error| {
            format!(
                "无法解析打包 MCP 服务目录 `{}`：{error}",
                catalog_path.display()
            )
        })?;
    if catalog.servers.is_empty() {
        return Err(format!("打包 MCP 服务目录为空：{}", catalog_path.display()));
    }
    let catalog_root = catalog_path.parent().unwrap_or_else(|| Path::new("."));
    let mut installed_ids = Vec::new();
    for entry in &catalog.servers {
        validate_packaged_mcp_server_entry(entry)?;
        if installed_ids.contains(&entry.id) {
            return Err(format!("打包 MCP 服务目录包含重复 ID：{}", entry.id));
        }
        let package_path = catalog_root.join(&entry.zip);
        let package = read_verified_mcp_server_package(&entry.id, &package_path)?;
        let actual_hash = format!("{:x}", Sha256::digest(&package));
        if !actual_hash.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!("打包 MCP 服务 `{}` 的目录哈希不匹配。", entry.id));
        }
        http_post_json_with_timeout(
            base_url,
            "/v1/mcp/servers/install",
            &serde_json::json!({ "zipBase64": base64_encode(&package) }),
            Duration::from_secs(120),
        )?;
        installed_ids.push(entry.id.clone());
    }
    write_utf8_regular_file(
        &marker_path,
        &format!("{migration_marker}\n"),
        "打包 MCP 服务迁移标记",
    )?;
    Ok(installed_ids)
}

pub(super) fn packaged_art_ids_needing_integrity_repair(
    base_url: &str,
    catalog: &PackagedArtCatalog,
) -> Result<BTreeSet<String>, String> {
    let response = http_get_json(base_url, "/v1/doctor/arts")?;
    let statuses = response
        .get("arts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Loom 本地服务没有返回 Art 诊断数组。".to_string())?;
    Ok(catalog
        .packages
        .iter()
        .filter(|entry| {
            let qualified_id = format!("neuro.official/{}", entry.id);
            statuses.iter().any(|status| {
                status.get("qualifiedId").and_then(Value::as_str) == Some(qualified_id.as_str())
                    && status.get("lockfileValid").and_then(Value::as_bool) == Some(false)
            })
        })
        .map(|entry| entry.id.clone())
        .collect())
}

pub(super) fn packaged_art_sha256_allowlist(current_exe: &Path) -> Result<Vec<String>, String> {
    let Some(catalog_path) = packaged_art_catalog_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(Vec::new());
    };
    let catalog_bytes =
        read_bounded_regular_file(&catalog_path, MAX_PACKAGE_CATALOG_BYTES, "打包 Art 目录")?;
    let catalog: PackagedArtCatalog = serde_json::from_slice(&catalog_bytes).map_err(|error| {
        format!(
            "无法解析打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    if catalog.packages.is_empty() {
        return Err(format!("打包 Art 目录为空：{}", catalog_path.display()));
    }

    let mut art_ids = BTreeSet::new();
    let mut hashes = Vec::with_capacity(catalog.packages.len());
    for entry in &catalog.packages {
        validate_packaged_art_entry(entry)?;
        if !art_ids.insert(entry.id.as_str()) {
            return Err(format!("打包 Art 目录包含重复 ID：{}", entry.id));
        }
        hashes.push(entry.sha256.to_ascii_lowercase());
    }
    Ok(hashes)
}

pub(super) fn desktop_control_plane_root() -> PathBuf {
    std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("APPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|root| root.join("Loom").join("control-plane"))
        })
        .unwrap_or_else(|| PathBuf::from(".runtime").join("loom").join("control-plane"))
}

pub(super) fn normalize_daemon_auth_token(value: &str, source: &str) -> Result<String, String> {
    let token = value.trim();
    if token.is_empty() {
        return Err(format!("Loom 本地服务认证令牌为空：{source}"));
    }
    if token.len() > 4096 || token.bytes().any(|byte| byte <= b' ' || byte == 0x7f) {
        return Err(format!("Loom 本地服务认证令牌格式无效：{source}"));
    }
    Ok(token.to_owned())
}

pub(super) fn daemon_auth_token() -> Result<Option<String>, String> {
    if let Some(value) = std::env::var_os("LOOM_DAEMON_TOKEN").filter(|value| !value.is_empty()) {
        return normalize_daemon_auth_token(&value.to_string_lossy(), "LOOM_DAEMON_TOKEN")
            .map(Some);
    }
    let path = desktop_control_plane_root().join(DAEMON_AUTH_TOKEN_FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "无法检查 Loom 本地服务认证令牌 {}：{error}",
            path.display()
        )),
        Ok(_) => {
            let value = read_bounded_utf8_file(&path, 8192, "Loom 本地服务认证令牌")?;
            normalize_daemon_auth_token(&value, &format!("{}", path.display())).map(Some)
        }
    }
}

pub(super) fn settings_url_with_daemon_token(url: &str) -> String {
    let Ok(Some(token)) = daemon_auth_token() else {
        return url.to_owned();
    };
    format!("{url}?token={}", percent_encode_path_segment(&token))
}

pub(super) fn validate_packaged_art_entry(entry: &PackagedArtCatalogEntry) -> Result<(), String> {
    for (kind, value) in [
        ("Art", entry.id.as_str()),
        ("框架", entry.framework.as_str()),
    ] {
        if value.is_empty()
            || value.len() > 256
            || value.contains("..")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(format!("打包{kind} ID 无效：{value}"));
        }
    }
    if entry.zip != format!("{}.zip", entry.id) {
        return Err(format!("打包 Art `{}` 的 ZIP 文件名无效。", entry.id));
    }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("打包 Art `{}` 的 SHA-256 无效。", entry.id));
    }
    Ok(())
}

pub(super) fn bootstrap_packaged_arts_from_exe(
    base_url: &str,
    current_exe: &Path,
    control_plane_root: &Path,
) -> Result<PackagedArtBootstrapResult, String> {
    let _bootstrap_guard = PACKAGED_ART_BOOTSTRAP_LOCK
        .lock()
        .map_err(|_| "打包 Art 初始化锁已损坏。".to_string())?;
    let Some(catalog_path) = packaged_art_catalog_candidates(current_exe)
        .into_iter()
        .find(|path| path.is_file())
    else {
        return Ok(PackagedArtBootstrapResult {
            available: false,
            applied: false,
            catalog_hash: None,
            framework_ids: Vec::new(),
            art_ids: Vec::new(),
        });
    };
    let catalog_bytes =
        read_bounded_regular_file(&catalog_path, MAX_PACKAGE_CATALOG_BYTES, "打包 Art 目录")?;
    let catalog_hash = format!("{:x}", Sha256::digest(&catalog_bytes));
    let catalog: PackagedArtCatalog = serde_json::from_slice(&catalog_bytes).map_err(|error| {
        format!(
            "无法解析打包 Art 目录 `{}`：{error}",
            catalog_path.display()
        )
    })?;
    if catalog.packages.is_empty() {
        return Err(format!("打包 Art 目录为空：{}", catalog_path.display()));
    }

    let mut art_ids = Vec::new();
    let mut framework_ids = Vec::new();
    for entry in &catalog.packages {
        validate_packaged_art_entry(entry)?;
        if art_ids.contains(&entry.id) {
            return Err(format!("打包 Art 目录包含重复 ID：{}", entry.id));
        }
        art_ids.push(entry.id.clone());
        if !framework_ids.contains(&entry.framework) {
            framework_ids.push(entry.framework.clone());
        }
    }

    // MCP services are independent packages. Install or upgrade the bundled
    // service catalog before frameworks and Arts that declare those services.
    // The separate marker intentionally prevents a user uninstall from being
    // undone on every desktop restart.
    let installed_mcp_ids =
        bootstrap_packaged_mcp_servers(base_url, current_exe, control_plane_root)?;
    let mcp_changed = !installed_mcp_ids.is_empty();

    let marker_path = control_plane_root
        .join("migrations")
        .join("packaged-arts.sha256");
    let catalog_already_applied = read_bounded_utf8_file(&marker_path, 128, "打包 Art 迁移标记")
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(&catalog_hash));

    let framework_response = http_get_json(base_url, "/v1/frameworks")?;
    let framework_statuses = framework_response
        .get("frameworks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Loom 本地服务没有返回框架数组。".to_string())?;
    let mut framework_changed = false;
    for framework_id in &framework_ids {
        let official_qualified_id = format!("neuro.official/{framework_id}");
        let status = framework_statuses.iter().find(|status| {
            status.get("id").and_then(Value::as_str) == Some(framework_id.as_str())
                || status.get("qualifiedId").and_then(Value::as_str)
                    == Some(official_qualified_id.as_str())
        });
        let installed = status
            .and_then(|value| value.get("installed"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let enabled = status
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ready = status
            .and_then(|value| value.get("ready"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let installed_version = status
            .and_then(|value| value.get("version"))
            .and_then(Value::as_str);
        let bundled_version = packaged_framework_version(current_exe, framework_id);
        let needs_upgrade = installed
            && ready
            && bundled_version
                .as_deref()
                .is_some_and(|version| installed_version != Some(version));
        if needs_upgrade {
            upgrade_packaged_framework_from_exe(base_url, framework_id, current_exe)?;
            framework_changed = true;
        } else if !installed || (enabled && !ready) {
            install_packaged_framework_from_exe(base_url, framework_id, current_exe)?;
            framework_changed = true;
        } else if !enabled {
            http_post_json_with_timeout(
                base_url,
                &format!(
                    "/v1/frameworks/{}/enable",
                    percent_encode_path_segment(framework_id)
                ),
                &serde_json::json!({}),
                Duration::from_secs(60),
            )?;
        }
    }

    // Dependency upgrades invalidate the immutable lock captured by an Art.
    // MCP repair is selected from installed doctor entries so a package update
    // cannot restore an intentionally uninstalled Art. On an ordinary startup,
    // an unavailable doctor is best-effort and will be retried next time.
    let install_all_arts = !catalog_already_applied || framework_changed;
    let repair_art_ids = if install_all_arts {
        BTreeSet::new()
    } else {
        match packaged_art_ids_needing_integrity_repair(base_url, &catalog) {
            Ok(ids) => ids,
            Err(error) if mcp_changed => return Err(error),
            Err(_) => BTreeSet::new(),
        }
    };
    if !install_all_arts && repair_art_ids.is_empty() {
        return Ok(PackagedArtBootstrapResult {
            available: true,
            applied: false,
            catalog_hash: Some(catalog_hash),
            framework_ids,
            art_ids,
        });
    }

    let catalog_root = catalog_path.parent().unwrap_or_else(|| Path::new("."));
    for entry in &catalog.packages {
        if !install_all_arts && !repair_art_ids.contains(&entry.id) {
            continue;
        }
        let package_path = catalog_root.join(&entry.zip);
        let package = read_verified_art_package(&entry.id, &package_path)?;
        let actual_hash = format!("{:x}", Sha256::digest(&package));
        if !actual_hash.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!("打包 Art `{}` 的目录哈希不匹配。", entry.id));
        }
        http_post_json_with_timeout(
            base_url,
            "/v1/arts/install",
            &serde_json::json!({
                "zipBase64": base64_encode(&package),
                "bundledCatalog": true,
            }),
            Duration::from_secs(120),
        )?;
    }

    write_utf8_regular_file(
        &marker_path,
        &format!("{catalog_hash}\n"),
        "打包 Art 迁移标记",
    )?;

    Ok(PackagedArtBootstrapResult {
        available: true,
        applied: true,
        catalog_hash: Some(catalog_hash),
        framework_ids,
        art_ids,
    })
}
