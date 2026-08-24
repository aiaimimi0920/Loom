//! Contained removal of an installed MCP package family.

use super::*;

pub fn uninstall_server_package(
    control_plane_root: &Path,
    config: &McpServerConfig,
) -> Result<(), McpPackageError> {
    let _lifecycle = lock_package_lifecycle();
    let Some(package) = &config.package else {
        return Ok(());
    };
    if !is_safe_identity(&package.publisher_id) {
        return Err(McpPackageError::InvalidManifest(
            "installed package publisher is unsafe".to_owned(),
        ));
    }
    let package_id = package
        .qualified_id
        .split_once('/')
        .filter(|(publisher, id)| *publisher == package.publisher_id && is_safe_identity(id))
        .map(|(_, id)| id)
        .ok_or_else(|| {
            McpPackageError::InvalidManifest(
                "installed package identity does not match its publisher".to_owned(),
            )
        })?;
    let package_root = control_plane_root
        .join("mcp")
        .join("packages")
        .join(&package.publisher_id)
        .join(package_id);
    let expected_versions_root = package_root.join("versions");
    if package.package_dir.parent() != Some(expected_versions_root.as_path()) {
        return Err(McpPackageError::InvalidManifest(
            "installed package directory is outside its package root".to_owned(),
        ));
    }
    if package_root.exists() {
        ensure_plain_directory(&package_root)?;
        ensure_plain_directory(&expected_versions_root)?;
        ensure_plain_directory(&package.package_dir)?;
        fs::remove_dir_all(&package_root)?;
    }
    Ok(())
}
