use std::fs;

use loom_plugin_security::{canonical_package_digest, verify_package_signature, TrustStore};
use loom_protocol::{
    parse_capability_manifest, PackageSignature, PublisherIdentity, MAX_CAPABILITY_MANIFEST_BYTES,
};

use crate::error::HostResult;
use crate::{CapabilityHostError, CapabilityRuntimePackage};

/// Revalidates immutable package evidence at every process boundary.
pub(crate) fn verify_runtime_package(package: &CapabilityRuntimePackage) -> HostResult<()> {
    let metadata = fs::symlink_metadata(&package.package_dir).map_err(|error| {
        CapabilityHostError::InvalidPackage(format!("read package directory: {error}"))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CapabilityHostError::InvalidPackage(
            "package directory is linked or not a directory".to_owned(),
        ));
    }

    let manifest_path = package.package_dir.join("capability.manifest.json");
    let manifest_metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
        CapabilityHostError::InvalidPackage(format!("read capability manifest: {error}"))
    })?;
    if manifest_metadata.file_type().is_symlink()
        || !manifest_metadata.is_file()
        || manifest_metadata.len() > MAX_CAPABILITY_MANIFEST_BYTES as u64
    {
        return Err(CapabilityHostError::InvalidPackage(
            "capability manifest is linked, missing, or oversized".to_owned(),
        ));
    }
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| {
        CapabilityHostError::InvalidPackage(format!("read capability manifest: {error}"))
    })?;
    let manifest = parse_capability_manifest(&manifest_bytes)
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    if manifest != package.manifest {
        return Err(CapabilityHostError::InvalidPackage(
            "in-memory manifest does not match the installed package".to_owned(),
        ));
    }

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
    let trust_store = TrustStore::load(&package.trust_store_path)
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    let trust_status = verify_package_signature(
        &package.package_dir,
        Some(&identity),
        Some(&signature),
        &trust_store,
    )
    .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    trust_store
        .effective_policy()
        .enforce(trust_status)
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;

    let actual_digest = canonical_package_digest(&package.package_dir, Some(&signature.file))
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    if actual_digest != package.digest {
        return Err(CapabilityHostError::InvalidPackage(
            "installed package digest no longer matches the registry".to_owned(),
        ));
    }
    Ok(())
}
