use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use loom_plugin_security::{canonical_package_digest, verify_package_signature, TrustStore};
use loom_protocol::{
    parse_capability_manifest, PackageSignature, PublisherIdentity, MAX_CAPABILITY_MANIFEST_BYTES,
};

use super::registry::ensure_capability_root;
use super::types::{
    CapabilityInstallError, CapabilityInstallReport, CapabilityInstalledVersion, CapabilityResult,
};
use super::CapabilityPluginRegistry;
use crate::private_store::read_bounded_private_file;

const MANIFEST_FILE: &str = "capability.manifest.json";

/// Installs a verified package without activating it.
///
/// Package bytes are first expanded into a private staging directory on the
/// same volume as the immutable target. The registry is updated only after the
/// package has been verified, moved into place, and made read-only.
pub fn install_capability_from_zip(
    zip_bytes: &[u8],
    registry: &CapabilityPluginRegistry,
) -> CapabilityResult<CapabilityInstallReport> {
    let packages_root = registry.packages_root();
    ensure_capability_root(&packages_root)?;
    let staging_root = packages_root.join(".staging");
    ensure_capability_root(&staging_root)?;
    let staging = unique_staging_path(&staging_root);

    let result = install_staged(zip_bytes, registry, &staging);
    if result.is_err() {
        let _ = remove_private_tree(&staging);
    }
    result
}

fn install_staged(
    zip_bytes: &[u8],
    registry: &CapabilityPluginRegistry,
    staging: &Path,
) -> CapabilityResult<CapabilityInstallReport> {
    let installed_files = crate::secure_zip::extract_zip_securely(zip_bytes, staging)
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
    validate_package_kind(&installed_files)?;
    let manifest_path = staging.join(MANIFEST_FILE);
    if !installed_files.iter().any(|path| path == MANIFEST_FILE) {
        return Err(CapabilityInstallError::InvalidPackage(format!(
            "package is missing root {MANIFEST_FILE}"
        )));
    }
    let manifest_bytes = read_bounded_private_file(
        &manifest_path,
        u64::try_from(MAX_CAPABILITY_MANIFEST_BYTES).expect("manifest budget fits u64"),
    )?;
    let manifest = parse_capability_manifest(&manifest_bytes)
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;

    let identity = PublisherIdentity {
        id: manifest.publisher.id.clone(),
        name: None,
        website: None,
        key_id: Some(manifest.publisher.key_id.clone()),
    };
    let signature = PackageSignature {
        algorithm: manifest.signature.algorithm.clone(),
        key_id: manifest.signature.key_id.clone(),
        file: manifest.signature.file.clone(),
    };
    let trust_store = TrustStore::load(&registry.control_plane_root().join("plugin-trust.json"))
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
    let trust_status =
        verify_package_signature(staging, Some(&identity), Some(&signature), &trust_store)
            .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
    trust_store
        .effective_policy()
        .enforce(trust_status.clone())
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;

    let digest = canonical_package_digest(staging, Some(&signature.file))
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
    let relative_path = PathBuf::from(&manifest.publisher.id)
        .join(&manifest.id)
        .join("versions")
        .join(format!(
            "{}-{}",
            sanitize_version(&manifest.version),
            &digest[..12]
        ));
    let package_dir = registry.packages_root().join(&relative_path);
    let created = commit_immutable_package(staging, &package_dir, &digest, &signature.file)?;
    let result = (|| {
        let plugin_root = package_dir.parent().and_then(Path::parent).ok_or_else(|| {
            CapabilityInstallError::InvalidPackage("invalid package root".to_owned())
        })?;
        for name in ["state", "cache", "outputs", "locks"] {
            ensure_capability_root(&plugin_root.join(name))?;
        }
        registry.register_install(
            &manifest,
            CapabilityInstalledVersion {
                version: manifest.version.clone(),
                digest: digest.clone(),
                relative_path: relative_path.to_string_lossy().replace('\\', "/"),
                trust_status: trust_status.clone(),
                installed_at: Utc::now().to_rfc3339(),
                requested_permissions: manifest.permissions.clone(),
            },
        )?;
        Ok(CapabilityInstallReport {
            qualified_id: manifest.qualified_id(),
            version: manifest.version,
            digest,
            package_dir: package_dir.clone(),
            trust_status,
            installed_files,
        })
    })();
    if result.is_err() && created {
        // A package is not installed until its registry record is durable.
        let _ = remove_private_tree(&package_dir);
    }
    result
}

fn validate_package_kind(installed_files: &[String]) -> CapabilityResult<()> {
    let markers = [
        (MANIFEST_FILE, true),
        ("framework.manifest.json", false),
        ("manifest.json", false),
        ("mcp.server.json", false),
    ];
    let present = markers
        .into_iter()
        .filter(|(name, _)| installed_files.iter().any(|path| path == name))
        .collect::<Vec<_>>();
    if present.len() != 1 || !present[0].1 {
        return Err(CapabilityInstallError::InvalidPackage(
            "package kind is missing, conflicting, or not capability".to_owned(),
        ));
    }
    Ok(())
}

fn commit_immutable_package(
    staging: &Path,
    target: &Path,
    digest: &str,
    signature_file: &str,
) -> CapabilityResult<bool> {
    let parent = target.parent().ok_or_else(|| {
        CapabilityInstallError::InvalidPackage("package target has no parent".to_owned())
    })?;
    ensure_capability_root(parent)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if crate::install::fs_safety::metadata_has_link_semantics(&metadata)
                || !metadata.is_dir()
            {
                return Err(CapabilityInstallError::Conflict(
                    "immutable target is linked or not a directory".to_owned(),
                ));
            }
            let existing = canonical_package_digest(target, Some(signature_file))
                .map_err(|error| CapabilityInstallError::Conflict(error.to_string()))?;
            if existing != digest {
                return Err(CapabilityInstallError::Conflict(
                    "immutable target digest does not match".to_owned(),
                ));
            }
            remove_private_tree(staging)?;
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            loom_plugin_security::repair_private_tree_permissions(staging)?;
            fs::rename(staging, target)?;
            if let Err(error) = set_private_tree_readonly(target) {
                let _ = remove_private_tree(target);
                return Err(error);
            }
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

fn set_private_tree_readonly(path: &Path) -> CapabilityResult<()> {
    crate::install::fs_safety::set_tree_readonly(path, true)
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))
}

fn remove_private_tree(path: &Path) -> CapabilityResult<()> {
    crate::install::fs_safety::remove_tree(path)
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))
}

fn sanitize_version(version: &str) -> String {
    version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn unique_staging_path(root: &Path) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    root.join(format!("install-{}-{nonce}", std::process::id()))
}
