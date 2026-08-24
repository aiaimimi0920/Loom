// Immutable package publication after manifest and publisher-signature validation.
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::catalog::build_catalog;
use crate::filesystem::ensure_write_parent;
use crate::indexes::assign_global_art_id;
use crate::manifest::{catalog_entry_from_manifest, read_manifest_bytes};
use crate::model::{PublishedArt, StoreError};
use crate::persistence::{lock_store, write_bytes_atomic};
use crate::signature::verify_published_package_signature;
use crate::storage::{art_version_zip_path, art_zip_sha256_sidecar, read_art_zip_version};
use crate::validation::is_safe_art_id;

pub fn store_verified_published_zip(
    root: &Path,
    declared_id: Option<&str>,
    zip_bytes: &[u8],
) -> Result<PublishedArt, StoreError> {
    verify_published_package_signature(root, zip_bytes)?;
    store_published_zip(root, declared_id, zip_bytes)
}

pub fn store_published_zip(
    root: &Path,
    declared_id: Option<&str>,
    zip_bytes: &[u8],
) -> Result<PublishedArt, StoreError> {
    let manifest = read_manifest_bytes(zip_bytes)?;
    let entry = catalog_entry_from_manifest(&manifest)?;
    if entry.id.is_empty() {
        return Err(StoreError::MissingManifest);
    }
    if !is_safe_art_id(&entry.id) {
        return Err(StoreError::InvalidArtId(entry.id));
    }
    if let Some(declared) = declared_id {
        let declared = declared.trim();
        if !declared.is_empty() && declared != entry.id {
            return Err(StoreError::ArtIdMismatch {
                declared: declared.to_owned(),
                manifest: entry.id,
            });
        }
    }
    let _lock = lock_store(root)?;
    if let Some(existing) = build_catalog(root)?
        .into_iter()
        .find(|existing| existing.id == entry.id)
    {
        if existing.qualified_id != entry.qualified_id {
            return Err(StoreError::IdentityConflict {
                id: entry.id,
                existing: existing.qualified_id,
                incoming: entry.qualified_id,
            });
        }
    }
    let version = entry.latest_version.clone();
    let path = art_version_zip_path(root, &entry.id, &version)?;
    let parent = path.parent().expect("versioned Art package parent");
    std::fs::create_dir_all(parent)?;
    ensure_write_parent(root, parent)?;
    if let Some(existing) = read_art_zip_version(root, &entry.id, &version)? {
        if Sha256::digest(&existing) != Sha256::digest(zip_bytes) {
            return Err(StoreError::VersionConflict {
                id: entry.id,
                version,
            });
        }
    } else {
        write_bytes_atomic(&path, zip_bytes)?;
    }
    let sidecar =
        art_zip_sha256_sidecar(&format!("{}/{}", entry.id, entry.latest_version), zip_bytes);
    write_bytes_atomic(&path.with_extension("zip.sha256"), sidecar.as_bytes())?;
    let global_id = assign_global_art_id(root, &entry.qualified_id)?;
    Ok(PublishedArt {
        art_id: entry.id,
        global_id,
    })
}
