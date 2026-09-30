use std::fs;

use loom_plugin_security::{
    canonical_package_digest, verify_package_signature_with_digest, TrustStore,
};
use loom_protocol::{
    parse_capability_manifest, PackageSignature, PublisherIdentity, MAX_CAPABILITY_MANIFEST_BYTES,
};
use loom_security::metadata_has_link_semantics;

use crate::error::HostResult;
use crate::{CapabilityHostError, CapabilityRuntimePackage};

/// Revalidates immutable package evidence at every process boundary.
pub(crate) fn verify_runtime_package(package: &CapabilityRuntimePackage) -> HostResult<()> {
    let metadata = fs::symlink_metadata(&package.package_dir).map_err(|error| {
        CapabilityHostError::InvalidPackage(format!("read package directory: {error}"))
    })?;
    if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
        return Err(CapabilityHostError::InvalidPackage(
            "package directory is linked or not a directory".to_owned(),
        ));
    }

    let manifest_path = package.package_dir.join("capability.manifest.json");
    let manifest_metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
        CapabilityHostError::InvalidPackage(format!("read capability manifest: {error}"))
    })?;
    if metadata_has_link_semantics(&manifest_metadata)
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
    let verified = verify_package_signature_with_digest(
        &package.package_dir,
        Some(&identity),
        Some(&signature),
        &trust_store,
    )
    .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    let trust_status = verified.trust_status;
    trust_store
        .effective_policy()
        .enforce(trust_status.clone())
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    // The recorded status is what the contribution snapshot publishes to Hook, which gates UI
    // affordances on it. Policy enforcement alone only rejects statuses the policy forbids, so a
    // downgrade the policy still tolerates — a publisher key removed from the trust store turning
    // `Trusted` into `Unsigned` — would leave the plugin running while Hook keeps showing the
    // status it had at activation. Treat any drift as evidence the package must be re-admitted.
    if trust_status != package.trust_status {
        return Err(CapabilityHostError::InvalidPackage(
            "installed package trust status no longer matches the registry".to_owned(),
        ));
    }

    // Reuses the digest the signature check already hashed the tree for. Recomputing it here meant
    // every reverification walked and hashed the whole package twice, and this runs before every
    // spawn — including every lazy restart after an idle session is pruned.
    let actual_digest = match verified.canonical_digest {
        Some(digest) => digest,
        // Unreachable while a signature is supplied: the digest is skipped only for an unsigned
        // package. Recomputed rather than assumed so an unsigned path added later still gets the
        // integrity check instead of silently losing it.
        None => canonical_package_digest(&package.package_dir, Some(&signature.file))
            .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?,
    };
    if actual_digest != package.digest {
        return Err(CapabilityHostError::InvalidPackage(
            "installed package digest no longer matches the registry".to_owned(),
        ));
    }
    Ok(())
}
