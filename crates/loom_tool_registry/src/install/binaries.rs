use super::*;

pub(super) fn sanitize_version_for_path(version: &str) -> String {
    let sanitized = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unknown".to_owned()
    } else {
        sanitized
    }
}

/// Hex-encode a byte slice (lowercase).
pub(super) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Compute the sha256 of `bytes` as a lowercase hex string.
pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

/// Verify (optional) sha256 of already-loaded binary bytes.
pub(super) fn verify_binary_hash(
    name: &str,
    bytes: &[u8],
    expected: &Option<String>,
) -> Result<(), ArtInstallError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        return Err(ArtInstallError::BinaryHashMismatch {
            name: name.to_owned(),
            expected: expected.trim().to_owned(),
            actual,
        });
    }
    Ok(())
}

/// Download a portable third-party binary into `dest`, verifying sha256 if given.
pub(super) fn download_binary(binary: &ArtBinary, dest: &Path) -> Result<(), ArtInstallError> {
    if binary
        .sha256
        .as_deref()
        .is_none_or(|digest| digest.trim().is_empty())
    {
        return Err(ArtInstallError::RemoteBinaryHashRequired {
            name: binary.name.clone(),
        });
    }
    let url = binary
        .url
        .as_deref()
        .ok_or_else(|| ArtInstallError::BinaryMissing {
            name: binary.name.clone(),
        })?;
    let policy = crate::network_policy::OutboundPolicy {
        allow_http_loopback: true,
        ..crate::network_policy::OutboundPolicy::default()
    };
    let client = crate::network_policy::secure_client(
        "Loom/0.1 Art Binary Fetch",
        std::time::Duration::from_secs(120),
        policy.clone(),
    )
    .map_err(|error| ArtInstallError::BinaryDownloadFailed {
        name: binary.name.clone(),
        reason: error,
    })?;
    let bytes = crate::network_policy::get_bounded(&client, url, &policy, 128 * 1024 * 1024)
        .map_err(|error| ArtInstallError::BinaryDownloadFailed {
            name: binary.name.clone(),
            reason: error,
        })?;
    verify_binary_hash(&binary.name, &bytes, &binary.sha256)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &bytes)?;
    Ok(())
}

/// Resolve each declared binary into the art dir. A binary bundled in the zip
/// (its `name` matches an extracted file) is verified in place; otherwise it is
/// downloaded from its `url`. Returns the relative names resolved.
pub(super) fn resolve_binaries(
    binaries: &[ArtBinary],
    art_dir: &Path,
    installed_files: &[String],
) -> Result<Vec<String>, ArtInstallError> {
    let mut resolved = Vec::new();
    for binary in binaries {
        let rel = binary.name.replace('\\', "/");
        let path = Path::new(&rel);
        if rel.trim().is_empty()
            || rel.contains(':')
            || path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ArtInstallError::InvalidPackage(format!(
                "art binary path must stay inside the package: {}",
                binary.name
            )));
        }
        let bundled = installed_files
            .iter()
            .any(|file| file.replace('\\', "/") == rel);
        let dest = art_dir.join(&rel);
        if bundled {
            // Verify the already-extracted file if a hash was declared.
            if binary.sha256.is_some() {
                let bytes = std::fs::read(&dest)?;
                verify_binary_hash(&binary.name, &bytes, &binary.sha256)?;
            }
        } else if binary.url.is_some() {
            download_binary(binary, &dest)?;
        } else {
            return Err(ArtInstallError::BinaryMissing {
                name: binary.name.clone(),
            });
        }
        resolved.push(rel);
    }
    Ok(resolved)
}
