// Bounded regular-file reads constrained to the configured Art Store root.
use std::io::Read;
use std::path::Path;

use crate::model::StoreError;

pub(crate) fn read_optional_regular_file(
    root: &Path,
    path: &Path,
    limit: u64,
) -> Result<Option<Vec<u8>>, StoreError> {
    let canonical_root = match std::fs::canonicalize(root) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let canonical_path = match std::fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !canonical_path.starts_with(&canonical_root) {
        return Err(StoreError::UnsafeStoredPath);
    }
    let metadata = std::fs::symlink_metadata(&canonical_path)?;
    if !metadata.file_type().is_file() || is_reparse_or_symlink(&metadata) {
        return Err(StoreError::UnsafeStoredPath);
    }
    if metadata.len() > limit {
        return Err(StoreError::StoredResourceTooLarge(limit));
    }
    let file = std::fs::File::open(&canonical_path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(StoreError::StoredResourceTooLarge(limit));
    }
    Ok(Some(bytes))
}

pub(crate) fn ensure_write_parent(root: &Path, parent: &Path) -> Result<(), StoreError> {
    let canonical_root = std::fs::canonicalize(root)?;
    let canonical_parent = std::fs::canonicalize(parent)?;
    if !canonical_parent.starts_with(canonical_root) {
        return Err(StoreError::UnsafeStoredPath);
    }
    let metadata = std::fs::symlink_metadata(&canonical_parent)?;
    if !metadata.file_type().is_dir() || is_reparse_or_symlink(&metadata) {
        return Err(StoreError::UnsafeStoredPath);
    }
    Ok(())
}

pub(crate) fn is_reparse_or_symlink(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}
