// Bounded ZIP metadata validation and entry reads for untrusted published packages.
use std::io::Read;

use crate::model::StoreError;
use crate::validation::is_safe_resource_name;

pub const MAX_PUBLISHED_ZIP_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_SIGNATURE_DOCUMENT_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;

pub(crate) fn open_bounded_archive(
    zip_bytes: &[u8],
) -> Result<zip::ZipArchive<std::io::Cursor<&[u8]>>, StoreError> {
    if zip_bytes.len() as u64 > MAX_PUBLISHED_ZIP_BYTES {
        return Err(StoreError::PackageTooLarge(MAX_PUBLISHED_ZIP_BYTES));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(StoreError::ArchiveEntryCount);
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        validate_archive_name(&file)?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(StoreError::ArchiveSymbolicLink(file.name().to_owned()));
        }
        let size = file.size();
        if size > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(StoreError::ArchiveEntryTooLarge {
                name: file.name().to_owned(),
                limit: MAX_ARCHIVE_ENTRY_BYTES,
            });
        }
        expanded = expanded
            .checked_add(size)
            .ok_or(StoreError::ArchiveExpandedTooLarge(
                MAX_ARCHIVE_EXPANDED_BYTES,
            ))?;
        if expanded > MAX_ARCHIVE_EXPANDED_BYTES {
            return Err(StoreError::ArchiveExpandedTooLarge(
                MAX_ARCHIVE_EXPANDED_BYTES,
            ));
        }
        let compressed = file.compressed_size();
        if size > 1024 * 1024
            && (compressed == 0 || size / compressed.max(1) > MAX_COMPRESSION_RATIO)
        {
            return Err(StoreError::ArchiveCompressionRatio(file.name().to_owned()));
        }
    }
    Ok(archive)
}

pub(crate) fn read_entry_bounded(
    file: &mut impl Read,
    name: &str,
    declared_size: u64,
    limit: u64,
) -> Result<Vec<u8>, StoreError> {
    if declared_size > limit {
        return Err(StoreError::ArchiveEntryTooLarge {
            name: name.to_owned(),
            limit,
        });
    }
    let mut bytes = Vec::with_capacity(declared_size.min(limit) as usize);
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(StoreError::ArchiveEntryTooLarge {
            name: name.to_owned(),
            limit,
        });
    }
    Ok(bytes)
}

pub(crate) fn hash_entry_bounded(
    file: &mut impl Read,
    hasher: &mut sha2::Sha256,
    name: &str,
    declared_size: u64,
) -> Result<(), StoreError> {
    use sha2::Digest as _;

    let mut remaining = declared_size;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let length = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file.read(&mut buffer[..length])?;
        if read == 0 {
            return Err(StoreError::ArchiveEntryTooLarge {
                name: name.to_owned(),
                limit: declared_size,
            });
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(StoreError::ArchiveEntryTooLarge {
            name: name.to_owned(),
            limit: declared_size,
        });
    }
    Ok(())
}

fn validate_archive_name(file: &zip::read::ZipFile<'_>) -> Result<(), StoreError> {
    let raw_name = file.name();
    let normalized = raw_name.replace('\\', "/");
    if raw_name.contains('\\')
        || file.enclosed_name().is_none()
        || !is_safe_resource_name(&normalized)
    {
        return Err(StoreError::InvalidResourceName(normalized));
    }
    Ok(())
}
