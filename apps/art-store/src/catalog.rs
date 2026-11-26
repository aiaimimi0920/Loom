// Catalog scanning, version merging, and multi-kind/multi-platform metadata enrichment.
use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::archive::MAX_PUBLISHED_ZIP_BYTES;
use crate::filesystem::{is_reparse_or_symlink, read_optional_regular_file};
use crate::indexes::enrich_catalog;
use crate::manifest::{catalog_entry_from_manifest, read_manifest_bytes};
use crate::model::{CatalogEntry, StoreError, StorePackageKind};
use crate::storage::{ARTS_DIR, PACKAGES_DIR};

pub fn build_catalog(root: &Path) -> Result<Vec<CatalogEntry>, StoreError> {
    Ok(build_unified_catalog(root)?
        .into_iter()
        .filter(|entry| entry.kind == StorePackageKind::Art)
        .collect())
}

pub fn build_unified_catalog(root: &Path) -> Result<Vec<CatalogEntry>, StoreError> {
    let mut entries_by_kind_id = BTreeMap::<(StorePackageKind, String), CatalogEntry>::new();
    scan_kind_directory(root, &root.join(ARTS_DIR), &mut entries_by_kind_id)?;
    for kind in [StorePackageKind::Capability, StorePackageKind::McpServer] {
        let kind_dir = root.join(PACKAGES_DIR).join(kind.as_str());
        scan_kind_directory(root, &kind_dir, &mut entries_by_kind_id)?;
    }
    let mut enrich_map = entries_by_kind_id
        .into_iter()
        .map(|((kind, id), entry)| (format!("{}:{id}", kind.as_str()), entry))
        .collect::<BTreeMap<_, _>>();
    enrich_catalog(root, &mut enrich_map)?;
    Ok(enrich_map.into_values().collect())
}

fn scan_kind_directory(
    store_root: &Path,
    kind_dir: &Path,
    entries: &mut BTreeMap<(StorePackageKind, String), CatalogEntry>,
) -> Result<(), StoreError> {
    let metadata = match std::fs::symlink_metadata(kind_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_dir() || is_reparse_or_symlink(&metadata) {
        return Ok(());
    }
    for dir_entry in std::fs::read_dir(kind_dir)? {
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
                    merge_catalog_zip(store_root, entries, &version_path)?;
                }
            }
        }
    }
    Ok(())
}

fn merge_catalog_zip(
    store_root: &Path,
    entries: &mut BTreeMap<(StorePackageKind, String), CatalogEntry>,
    path: &Path,
) -> Result<(), StoreError> {
    let Ok(Some(bytes)) = read_optional_regular_file(store_root, path, MAX_PUBLISHED_ZIP_BYTES)
    else {
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
        .entry((incoming.kind, incoming.id.clone()))
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
            entry.platforms = incoming.platforms;
            entry.binary = incoming.binary;
        }
    }
    Ok(())
}
