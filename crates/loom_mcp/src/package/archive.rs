//! Bounded archive intake and package-relative path validation.

use super::*;

/// Extract a package archive with the same hardening the Art installer uses.
///
/// The shared extractor in `loom_security::archive` bounds the bytes actually produced by the
/// decompressor, rejects duplicate and case-colliding names, rejects Windows reserved names,
/// checks parent directories for symlinks, and opens every file with `create_new` so a second
/// entry can never overwrite the first. That last property is what keeps a reviewer and the
/// installer looking at the same `mcp.server.json`.
///
/// MCP packages are held to tighter limits than Art packages, so the declared entry count and
/// declared total size are still checked here first. Those values come from the central
/// directory and are attacker-controlled, which is why they are only an early reject: the real
/// bound is enforced against the produced bytes inside the shared extractor.
pub(super) fn extract_package(
    package_bytes: &[u8],
    staging_root: &Path,
) -> Result<(), McpPackageError> {
    let mut archive = ZipArchive::new(Cursor::new(package_bytes))
        .map_err(|error| McpPackageError::InvalidArchive(error.to_string()))?;
    if archive.len() > MAX_PACKAGE_FILES {
        return Err(McpPackageError::InvalidArchive(format!(
            "archive contains more than {MAX_PACKAGE_FILES} entries"
        )));
    }
    let mut declared_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| McpPackageError::InvalidArchive(error.to_string()))?;
        declared_bytes = declared_bytes.saturating_add(entry.size());
        if declared_bytes > MAX_EXTRACTED_BYTES {
            return Err(McpPackageError::InvalidArchive(format!(
                "extracted content exceeds {MAX_EXTRACTED_BYTES} bytes"
            )));
        }
    }
    extract_zip_securely(package_bytes, staging_root).map_err(package_error_from_secure_zip)?;
    Ok(())
}

pub(super) fn package_error_from_secure_zip(error: SecureZipError) -> McpPackageError {
    match error {
        SecureZipError::UnsafePath(value)
        | SecureZipError::UnsafeWindowsName(value)
        | SecureZipError::SymbolicLink(value)
        | SecureZipError::DuplicatePath(value) => McpPackageError::UnsafePath(value),
        SecureZipError::Io(error) => McpPackageError::Io(error),
        other => McpPackageError::InvalidArchive(other.to_string()),
    }
}

pub(super) fn safe_relative_path(value: &str) -> Result<PathBuf, McpPackageError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(McpPackageError::UnsafePath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}
