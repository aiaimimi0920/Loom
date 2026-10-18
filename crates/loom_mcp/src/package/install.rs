//! Staged package installation and immutable version activation.

use super::*;

pub fn install_server_package(
    control_plane_root: &Path,
    package_bytes: &[u8],
) -> Result<McpServerConfig, McpPackageError> {
    install_server_package_with_commit(control_plane_root, package_bytes, |_| Ok(()))
}

/// 提交失败时恢复原激活事实，保留所有不可变版本；回调不得再次进入包生命周期锁。
pub fn install_server_package_with_commit(
    control_plane_root: &Path,
    package_bytes: &[u8],
    commit: impl FnOnce(&mut McpServerConfig) -> Result<(), McpPackageError>,
) -> Result<McpServerConfig, McpPackageError> {
    let _lifecycle = lock_package_lifecycle();
    if package_bytes.len() > MAX_MCP_SERVER_PACKAGE_BYTES {
        return Err(McpPackageError::PackageTooLarge);
    }
    let digest = format!("{:x}", Sha256::digest(package_bytes));
    let staging_parent = ensure_directory_chain(control_plane_root, &["mcp", "staging"])?;
    let staging_root = create_unique_directory(&staging_parent, "install")?;
    let result = (|| {
        extract_package(package_bytes, &staging_root)?;
        let manifest_path = staging_root.join(MCP_SERVER_PACKAGE_MANIFEST);
        let manifest_bytes = read_bounded_file(
            &manifest_path,
            MAX_PACKAGE_MANIFEST_BYTES,
            "MCP package manifest",
        )?;
        let manifest: McpServerPackageManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|error| {
                McpPackageError::InvalidManifest(format!(
                    "cannot parse {}: {error}",
                    manifest_path.display()
                ))
            })?;
        validate_manifest(&manifest, &staging_root)?;
        let trust_status = verify_package_trust(control_plane_root, &manifest, &staging_root)?;
        let files = digest_tree(&staging_root)?;

        let packages_root = ensure_directory_chain(control_plane_root, &["mcp", "packages"])?;
        let package_root =
            ensure_directory_chain(&packages_root, &[&manifest.publisher.id, &manifest.id])?;
        let versions_root = ensure_directory_chain(&package_root, &["versions"])?;
        let target_dir = versions_root.join(format!(
            "{}-{}",
            manifest.version,
            &digest[..PACKAGE_DIRECTORY_DIGEST_CHARS]
        ));
        if target_dir.exists() {
            ensure_plain_directory(&target_dir)?;
            // The archive digest names this directory, so an existing one must hold exactly the
            // bytes just extracted. Checking that is what makes reinstalling a package a repair
            // instead of a no-op that keeps a tree somebody else edited.
            verify_tree_digests(&target_dir, &files)?;
            fs::remove_dir_all(&staging_root)?;
        } else {
            fs::rename(&staging_root, &target_dir)?;
        }
        let mut config = config_from_manifest(
            manifest.clone(),
            digest.clone(),
            target_dir.clone(),
            files.clone(),
            trust_status.clone(),
        )?;
        let active = package_root.join("active.json");
        let previous = match fs::symlink_metadata(&active) {
            Ok(_) => Some(read_bounded_file(
                &active,
                MAX_ACTIVE_STATE_BYTES,
                "MCP package active state",
            )?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        write_active_state(
            &package_root,
            &manifest,
            &digest,
            &target_dir,
            &files,
            &trust_status,
        )?;
        if let Err(error) = commit(&mut config) {
            let restored = match previous {
                Some(bytes) => write_active_bytes(&package_root, &bytes),
                None => fs::remove_file(&active).map_err(McpPackageError::from),
            };
            if let Err(rollback) = restored {
                return Err(McpPackageError::Integrity(format!(
                    "commit failed: {error}; active rollback failed: {rollback}"
                )));
            }
            return Err(error);
        }
        Ok(config)
    })();
    if staging_root.exists() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}
