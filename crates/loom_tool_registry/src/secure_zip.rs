use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path};

const MAX_COMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_RELATIVE_PATH_BYTES: usize = 240;
const MAX_COMPRESSION_RATIO: u64 = 200;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SecureZipError {
    #[error("archive exceeds compressed size limit of {MAX_COMPRESSED_BYTES} bytes")]
    CompressedSize,
    #[error("archive contains more than {MAX_FILES} entries")]
    FileCount,
    #[error("archive exceeds uncompressed size limit of {MAX_UNCOMPRESSED_BYTES} bytes")]
    UncompressedSize,
    #[error("archive entry `{name}` exceeds per-entry limit of {MAX_ENTRY_BYTES} bytes")]
    EntrySize { name: String },
    #[error("archive entry `{name}` has a suspicious compression ratio")]
    CompressionRatio { name: String },
    #[error("archive entry has an unsafe path: {0}")]
    UnsafePath(String),
    #[error("archive contains a duplicate or case-colliding path: {0}")]
    DuplicatePath(String),
    #[error("archive entry is a symbolic link: {0}")]
    SymbolicLink(String),
    #[error("archive entry uses an unsafe Windows path component: {0}")]
    UnsafeWindowsName(String),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub(crate) fn extract_zip_securely(
    zip_bytes: &[u8],
    destination: &Path,
) -> Result<Vec<String>, SecureZipError> {
    if zip_bytes.len() as u64 > MAX_COMPRESSED_BYTES {
        return Err(SecureZipError::CompressedSize);
    }
    if destination.exists() {
        let metadata = fs::symlink_metadata(destination)?;
        if metadata.file_type().is_symlink() {
            return Err(SecureZipError::SymbolicLink(
                destination.display().to_string(),
            ));
        }
    } else {
        fs::create_dir_all(destination)?;
    }

    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    if archive.len() > MAX_FILES {
        return Err(SecureZipError::FileCount);
    }

    let mut seen = HashSet::new();
    let mut total_uncompressed = 0u64;
    let mut installed_files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let raw_name = entry.name().to_owned();
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(SecureZipError::UnsafePath(raw_name));
        };
        validate_relative_path(&enclosed)?;
        let normalized = normalize_relative_path(&enclosed);
        if !seen.insert(normalized.clone()) {
            return Err(SecureZipError::DuplicatePath(normalized));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(SecureZipError::SymbolicLink(raw_name));
        }

        let entry_size = entry.size();
        if entry_size > MAX_ENTRY_BYTES {
            return Err(SecureZipError::EntrySize { name: raw_name });
        }
        total_uncompressed = total_uncompressed.saturating_add(entry_size);
        if total_uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(SecureZipError::UncompressedSize);
        }
        let compressed_size = entry.compressed_size();
        if entry_size > 1024 * 1024
            && (compressed_size == 0 || entry_size / compressed_size.max(1) > MAX_COMPRESSION_RATIO)
        {
            return Err(SecureZipError::CompressionRatio { name: raw_name });
        }

        let output_path = destination.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
            let metadata = fs::symlink_metadata(parent)?;
            if metadata.file_type().is_symlink() {
                return Err(SecureZipError::SymbolicLink(parent.display().to_string()));
            }
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        let copied = copy_bounded(&mut entry, &mut output, MAX_ENTRY_BYTES)?;
        if copied > MAX_ENTRY_BYTES {
            let _ = fs::remove_file(&output_path);
            return Err(SecureZipError::EntrySize { name: raw_name });
        }
        output.sync_all()?;
        installed_files.push(enclosed.to_string_lossy().replace('\\', "/"));
    }
    Ok(installed_files)
}

fn copy_bounded(reader: &mut impl Read, writer: &mut impl Write, limit: u64) -> io::Result<u64> {
    let mut limited = reader.take(limit + 1);
    io::copy(&mut limited, writer)
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn validate_relative_path(path: &Path) -> Result<(), SecureZipError> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.len() > MAX_RELATIVE_PATH_BYTES {
        return Err(SecureZipError::UnsafePath(normalized));
    }
    for component in path.components() {
        let Component::Normal(component) = component else {
            return Err(SecureZipError::UnsafePath(normalized));
        };
        let value = component.to_string_lossy();
        if value.is_empty()
            || value.contains(':')
            || value.ends_with('.')
            || value.ends_with(' ')
            || is_windows_reserved_name(&value)
        {
            return Err(SecureZipError::UnsafeWindowsName(value.into_owned()));
        }
    }
    Ok(())
}

fn is_windows_reserved_name(value: &str) -> bool {
    let base = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
        || base
            .strip_prefix("LPT")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zip::write::SimpleFileOptions;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            for (name, content) in entries {
                writer
                    .start_file(name, SimpleFileOptions::default())
                    .expect("start file");
                writer.write_all(content).expect("content");
            }
            writer.finish().expect("finish");
        }
        bytes
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-secure-zip-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn extracts_regular_files() {
        let destination = temp_dir("regular");
        let files = extract_zip_securely(
            &archive(&[("manifest.json", b"{}"), ("runtime/main.txt", b"ok")]),
            &destination,
        )
        .expect("extract");
        assert_eq!(files, vec!["manifest.json", "runtime/main.txt"]);
        assert_eq!(
            fs::read(destination.join("runtime/main.txt")).unwrap(),
            b"ok"
        );
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn rejects_case_collisions_and_windows_reserved_names() {
        let destination = temp_dir("collision");
        let error = extract_zip_securely(
            &archive(&[("Art.json", b"{}"), ("art.json", b"{}")]),
            &destination,
        )
        .expect_err("collision");
        assert!(matches!(error, SecureZipError::DuplicatePath(_)));
        let _ = fs::remove_dir_all(&destination);

        let destination = temp_dir("reserved");
        let error = extract_zip_securely(&archive(&[("CON.txt", b"bad")]), &destination)
            .expect_err("reserved name");
        assert!(matches!(error, SecureZipError::UnsafeWindowsName(_)));
        let _ = fs::remove_dir_all(destination);
    }
}
