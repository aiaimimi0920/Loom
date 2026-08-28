use std::fs;
use std::path::PathBuf;

use loom_plugin_security::{canonical_package_digest, verify_package_signature, TrustStore};
use loom_protocol::{
    parse_capability_manifest, CapabilityPackageManifest, PackageSignature, PackageTrustStatus,
    PublisherIdentity, MAX_CAPABILITY_MANIFEST_BYTES,
};

use super::types::{CapabilityInstallError, CapabilityResult};
use super::CapabilityPluginRegistry;
use crate::private_store::read_bounded_regular_file;

#[derive(Clone, Debug)]
pub struct VerifiedCapabilityPackage {
    pub manifest: CapabilityPackageManifest,
    pub package_dir: PathBuf,
    pub digest: String,
    pub trust_status: PackageTrustStatus,
}

impl CapabilityPluginRegistry {
    /// Resolves an installed immutable version and refreshes its trust evidence.
    pub fn verify_installed_version(
        &self,
        qualified_id: &str,
        digest: &str,
    ) -> CapabilityResult<VerifiedCapabilityPackage> {
        let record = self
            .get(qualified_id)?
            .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
        let version = record
            .versions
            .iter()
            .find(|version| version.digest == digest)
            .ok_or_else(|| CapabilityInstallError::NotFound(digest.to_owned()))?;
        let root = self.packages_root();
        let package_dir = root.join(&version.relative_path);
        let root_canonical = fs::canonicalize(&root)?;
        let package_metadata = fs::symlink_metadata(&package_dir)?;
        if package_metadata.file_type().is_symlink() || !package_metadata.is_dir() {
            return Err(CapabilityInstallError::InvalidPackage(
                "installed package root is linked or not a directory".to_owned(),
            ));
        }
        let package_canonical = fs::canonicalize(&package_dir)?;
        if !package_canonical.starts_with(&root_canonical) {
            return Err(CapabilityInstallError::InvalidPackage(
                "installed package escapes the capability root".to_owned(),
            ));
        }

        let manifest_bytes = read_bounded_regular_file(
            &package_dir.join("capability.manifest.json"),
            MAX_CAPABILITY_MANIFEST_BYTES as u64,
        )?;
        let manifest = parse_capability_manifest(&manifest_bytes)
            .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
        if manifest.qualified_id() != qualified_id
            || manifest.version != version.version
            || manifest.permissions != version.requested_permissions
        {
            return Err(CapabilityInstallError::InvalidPackage(
                "installed manifest does not match its registry identity".to_owned(),
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
        let trust_store_path = self.control_plane_root().join("plugin-trust.json");
        let trust_store = TrustStore::load(&trust_store_path)
            .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &package_dir,
            Some(&identity),
            Some(&signature),
            &trust_store,
        )
        .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
        trust_store
            .effective_policy()
            .enforce(trust_status.clone())
            .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
        let actual_digest = canonical_package_digest(&package_dir, Some(&signature.file))
            .map_err(|error| CapabilityInstallError::InvalidPackage(error.to_string()))?;
        if actual_digest != digest {
            return Err(CapabilityInstallError::InvalidPackage(
                "installed package digest changed".to_owned(),
            ));
        }
        Ok(VerifiedCapabilityPackage {
            manifest,
            package_dir,
            digest: digest.to_owned(),
            trust_status,
        })
    }
}
