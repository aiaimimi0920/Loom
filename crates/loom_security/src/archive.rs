//! Hardened zip extraction for packages that arrive from outside the host.
//!
//! Moved out of `loom_tool_registry` so that `loom_mcp` can use the same extractor; see the
//! crate-level documentation for why a leaf crate is required.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path};

use crate::metadata_has_link_semantics;

const MAX_COMPRESSED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_RELATIVE_PATH_BYTES: usize = 240;
const MAX_COMPRESSION_RATIO: u64 = 200;

#[derive(Debug, thiserror::Error)]
pub enum SecureZipError {
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

pub fn extract_zip_securely(
    zip_bytes: &[u8],
    destination: &Path,
) -> Result<Vec<String>, SecureZipError> {
    if zip_bytes.len() as u64 > MAX_COMPRESSED_BYTES {
        return Err(SecureZipError::CompressedSize);
    }
    if destination.exists() {
        let metadata = fs::symlink_metadata(destination)?;
        if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
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
    let mut declared_uncompressed = 0u64;
    let mut written_uncompressed = 0u64;
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

        // Sizes read from the archive index are attacker-controlled, so they are only ever a
        // cheap early reject. Every limit that has to hold is re-enforced below against the
        // bytes actually decompressed.
        let entry_size = entry.size();
        if entry_size > MAX_ENTRY_BYTES {
            return Err(SecureZipError::EntrySize { name: raw_name });
        }
        declared_uncompressed = declared_uncompressed.saturating_add(entry_size);
        if declared_uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(SecureZipError::UncompressedSize);
        }
        let compressed_size = entry.compressed_size();

        let output_path = destination.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            validate_plain_directory_tree(destination, &output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
            validate_plain_directory_tree(destination, parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        // An entry that decompresses past what the archive claimed stops at the tighter of the
        // per-entry ceiling and whatever is left of the archive-wide budget, so a lying index
        // cannot turn 4096 entries into an unbounded amount of disk.
        let remaining = MAX_UNCOMPRESSED_BYTES - written_uncompressed;
        let budget = MAX_ENTRY_BYTES.min(remaining);
        let copied = copy_bounded(&mut entry, &mut output, budget)?;
        if copied > budget {
            let _ = fs::remove_file(&output_path);
            return Err(if remaining < MAX_ENTRY_BYTES {
                SecureZipError::UncompressedSize
            } else {
                SecureZipError::EntrySize { name: raw_name }
            });
        }
        written_uncompressed += copied;
        if copied > 1024 * 1024
            && (compressed_size == 0 || copied / compressed_size.max(1) > MAX_COMPRESSION_RATIO)
        {
            let _ = fs::remove_file(&output_path);
            return Err(SecureZipError::CompressionRatio { name: raw_name });
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

fn validate_plain_directory_tree(root: &Path, directory: &Path) -> Result<(), SecureZipError> {
    let relative = directory
        .strip_prefix(root)
        .map_err(|_| SecureZipError::UnsafePath(directory.display().to_string()))?;
    let mut current = root.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            current.push(component.as_os_str());
        }
        let metadata = fs::symlink_metadata(&current)?;
        if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
            return Err(SecureZipError::SymbolicLink(current.display().to_string()));
        }
    }
    Ok(())
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
                    .start_file(*name, SimpleFileOptions::default())
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

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).expect("create directory symlink");
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) {
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status()
            .expect("run mklink");
        assert!(status.success(), "create directory junction");
    }

    #[test]
    fn rejects_a_preexisting_linked_destination() {
        let outside = temp_dir("linked-destination-target");
        let destination = temp_dir("linked-destination");
        fs::create_dir_all(&outside).expect("outside directory");
        create_directory_link(&outside, &destination);

        let error = extract_zip_securely(&archive(&[("escaped.txt", b"bad")]), &destination)
            .expect_err("linked destination");

        assert!(matches!(error, SecureZipError::SymbolicLink(_)));
        assert!(!outside.join("escaped.txt").exists());
        #[cfg(windows)]
        fs::remove_dir(&destination).expect("remove junction");
        #[cfg(unix)]
        fs::remove_file(&destination).expect("remove symlink");
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn a_lying_index_cannot_bypass_the_decompressed_byte_budget() {
        // Both size limits used to be enforced against the sizes the archive declares for itself,
        // which an attacker writes. Zeroing them left the per-entry and archive-wide budgets
        // looking satisfied no matter how much the entry actually decompressed to.
        let mut bytes = archive(&[("payload.bin", &vec![0u8; 2 * 1024 * 1024])]);
        zero_declared_sizes(&mut bytes);
        let mut reader = zip::ZipArchive::new(Cursor::new(bytes.clone())).expect("archive");
        assert_eq!(reader.by_index(0).expect("entry").size(), 0);

        let destination = temp_dir("lying-index");
        let error = extract_zip_securely(&bytes, &destination).expect_err("lying index");
        assert!(matches!(error, SecureZipError::CompressionRatio { .. }));
        assert!(!destination.join("payload.bin").exists());
        let _ = fs::remove_dir_all(destination);
    }

    /// Rewrites the declared uncompressed length in the local and central headers to zero.
    fn zero_declared_sizes(bytes: &mut [u8]) {
        assert_eq!(&bytes[..4], b"PK\x03\x04", "local file header");
        bytes[22..26].fill(0);
        let central = bytes
            .windows(4)
            .rposition(|window| window == b"PK\x01\x02")
            .expect("central directory header");
        bytes[central + 24..central + 28].fill(0);
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        let destination = temp_dir("traversal");
        let error = extract_zip_securely(&archive(&[("../escape.txt", b"bad")]), &destination)
            .expect_err("traversal");
        assert!(matches!(error, SecureZipError::UnsafePath(_)));
        assert!(!destination.parent().unwrap().join("escape.txt").exists());
        let _ = fs::remove_dir_all(destination);
    }

    #[test]
    fn rejects_absolute_and_drive_qualified_entries() {
        let destination = temp_dir("absolute");
        let error = extract_zip_securely(
            &archive(&[("C:/windows/system32/a.txt", b"bad")]),
            &destination,
        )
        .expect_err("drive qualified");
        assert!(matches!(
            error,
            SecureZipError::UnsafePath(_) | SecureZipError::UnsafeWindowsName(_)
        ));
        let _ = fs::remove_dir_all(destination);
    }
}
