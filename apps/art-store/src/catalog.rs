// Catalog scanning, version merging and platform metadata enrichment.
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::archive::MAX_PUBLISHED_ZIP_BYTES;
use crate::filesystem::{is_reparse_or_symlink, read_optional_regular_file};
use crate::indexes::enrich_catalog;
use crate::manifest::{catalog_entry_from_manifest, read_manifest_bytes};
use crate::model::{CatalogEntry, StoreError};
use crate::storage::ARTS_DIR;

pub fn build_catalog(root: &Path) -> Result<Vec<CatalogEntry>, StoreError> {
    let arts_dir = root.join(ARTS_DIR);
    let mut entries = std::collections::BTreeMap::<String, CatalogEntry>::new();
    let arts_metadata = match std::fs::symlink_metadata(&arts_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    if !arts_metadata.file_type().is_dir() || is_reparse_or_symlink(&arts_metadata) {
        return Ok(Vec::new());
    }
    for dir_entry in std::fs::read_dir(&arts_dir)? {
        let dir_entry = dir_entry?;
        let path = dir_entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_dir() && !is_reparse_or_symlink(&metadata) {
            for version_entry in std::fs::read_dir(&path)? {
                let version_entry = version_entry?;
                let version_path = version_entry.path();
                let metadata = std::fs::symlink_metadata(&version_path)?;
                if metadata.file_type().is_file()
                    && !is_reparse_or_symlink(&metadata)
                    && version_path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        == Some("zip")
                {
                    merge_catalog_zip(&mut entries, &version_path)?;
                }
            }
        }
    }
    enrich_catalog(root, &mut entries)?;
    Ok(entries.into_values().collect())
}

fn merge_catalog_zip(
    entries: &mut std::collections::BTreeMap<String, CatalogEntry>,
    path: &Path,
) -> Result<(), StoreError> {
    let root = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or(StoreError::UnsafeStoredPath)?;
    let Ok(Some(bytes)) = read_optional_regular_file(root, path, MAX_PUBLISHED_ZIP_BYTES) else {
        return Ok(());
    };
    let Ok(manifest) = read_manifest_bytes(&bytes) else {
        return Ok(());
    };
    let Ok(mut incoming) = catalog_entry_from_manifest(&manifest) else {
        return Ok(());
    };
    if incoming.id.is_empty() {
        return Ok(());
    }
    incoming.versions[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
    let version = incoming.versions[0].clone();
    let entry = entries
        .entry(incoming.id.clone())
        .or_insert_with(|| incoming.clone());
    if entry.qualified_id != incoming.qualified_id {
        return Err(StoreError::IdentityConflict {
            id: incoming.id,
            existing: entry.qualified_id.clone(),
            incoming: incoming.qualified_id,
        });
    }
    if let Some(existing) = entry
        .versions
        .iter()
        .find(|existing| existing.version == version.version)
    {
        if existing.sha256 != version.sha256 {
            return Err(StoreError::VersionConflict {
                id: incoming.id,
                version: version.version,
            });
        }
    } else {
        entry.versions.push(version.clone());
    }
    entry.versions.sort_by(|left, right| {
        semver::Version::parse(&left.version)
            .ok()
            .cmp(&semver::Version::parse(&right.version).ok())
            .then_with(|| left.version.cmp(&right.version))
    });
    if let Some(latest) = entry.versions.last() {
        entry.latest_version = latest.version.clone();
        if latest.version == version.version {
            entry.qualified_id = incoming.qualified_id;
            entry.name = incoming.name;
            entry.description = incoming.description;
            entry.framework = incoming.framework;
        }
    }
    Ok(())
}
