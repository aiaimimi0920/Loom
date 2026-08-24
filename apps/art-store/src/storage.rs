// Validated Art, binary and framework package paths plus byte reads.
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::archive::MAX_PUBLISHED_ZIP_BYTES;
use crate::filesystem::read_optional_regular_file;
use crate::model::StoreError;
use crate::validation::{is_safe_art_id, is_safe_resource_name};

pub const ARTS_DIR: &str = "arts";
pub const BINARIES_DIR: &str = "binaries";
/// Framework packages are stored as `<root>/frameworks/<id>.zip`.
pub const FRAMEWORKS_DIR: &str = "frameworks";
const MAX_DIGEST_SIDECAR_BYTES: u64 = 4096;
const MAX_STORED_RESOURCE_BYTES: u64 = 256 * 1024 * 1024;

pub fn art_version_zip_path(root: &Path, id: &str, version: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_art_id(id) {
        return Err(StoreError::InvalidArtId(id.to_owned()));
    }
    if semver::Version::parse(version).is_err() {
        return Err(StoreError::InvalidVersion {
            id: id.to_owned(),
            version: version.to_owned(),
        });
    }
    Ok(root.join(ARTS_DIR).join(id).join(format!("{version}.zip")))
}

pub fn read_art_zip_version(
    root: &Path,
    id: &str,
    version: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    read_optional_regular_file(
        root,
        &art_version_zip_path(root, id, version)?,
        MAX_PUBLISHED_ZIP_BYTES,
    )
}

pub fn read_art_zip_version_sha256(
    root: &Path,
    id: &str,
    version: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    read_optional_regular_file(
        root,
        &art_version_zip_path(root, id, version)?.with_extension("zip.sha256"),
        MAX_DIGEST_SIDECAR_BYTES,
    )
}

pub(crate) fn art_zip_sha256_sidecar(id: &str, zip_bytes: &[u8]) -> String {
    let digest = Sha256::digest(zip_bytes);
    format!("{digest:x}  {id}.zip\n")
}

pub fn binary_path(root: &Path, name: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_resource_name(name) {
        return Err(StoreError::InvalidResourceName(name.to_owned()));
    }
    Ok(root.join(BINARIES_DIR).join(name))
}

pub fn read_binary(root: &Path, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
    read_optional_regular_file(root, &binary_path(root, name)?, MAX_STORED_RESOURCE_BYTES)
}

pub fn framework_package_path(root: &Path, id: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_art_id(id) {
        return Err(StoreError::InvalidArtId(id.to_owned()));
    }
    Ok(root.join(FRAMEWORKS_DIR).join(format!("{id}.zip")))
}

pub fn read_framework_package(root: &Path, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
    read_optional_regular_file(
        root,
        &framework_package_path(root, id)?,
        MAX_STORED_RESOURCE_BYTES,
    )
}
