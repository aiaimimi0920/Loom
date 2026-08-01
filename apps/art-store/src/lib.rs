//! Local Loom art store — core, transport-free logic.
//!
//! On-disk layout under the store root:
//!   <root>/arts/<id>.zip        art packages (manifest.json + resources)
//!   <root>/arts/<id>.zip.sha256 package digest sidecars
//!   <root>/binaries/<name>      third-party portable executables
//!
//! The daemon's art-store client (see `loom_tool_registry` / daemon) speaks:
//!   GET  /catalog               -> { "arts": [ {id,name,description,framework} ] }
//!   GET  /arts/<id>.zip         -> raw art package bytes
//!   GET  /arts/<id>.zip.sha256  -> package digest sidecar
//!   GET  /binaries/<name>       -> raw portable-exe bytes
//!   POST /publish               -> body = zip, header X-Art-Id: <id>
//!
//! This module holds only the pure pieces (catalog build, id/name validation,
//! publish persistence). The TCP server lives in `main.rs`.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const ARTS_DIR: &str = "arts";
pub const BINARIES_DIR: &str = "binaries";
/// Subdir holding framework package bundles, served as `/frameworks/<id>.zip`:
/// `<root>/frameworks/<id>.zip`. The daemon downloads an independently built
/// framework package from here and validates its manifest before installing it.
pub const FRAMEWORKS_DIR: &str = "frameworks";
const MANIFEST_NAME: &str = "manifest.json";

/// A catalog entry surfaced by `GET /catalog`. Mirrors the daemon's
/// `ArtStoreEntry` (camelCase over the wire is unnecessary — these are all
/// lowercase single words).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub framework: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid art id `{0}`")]
    InvalidArtId(String),
    #[error("invalid resource name `{0}`")]
    InvalidResourceName(String),
    #[error("published package missing {MANIFEST_NAME}")]
    MissingManifest,
    #[error("published manifest id `{manifest}` does not match declared id `{declared}`")]
    ArtIdMismatch { declared: String, manifest: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Reject ids that aren't safe as a single file-stem (no separators / traversal).
pub fn is_safe_art_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains(':')
        && id != "."
        && id != ".."
        && !id.contains("..")
}

/// Reject resource names that could escape the binaries dir. A single level of
/// nesting (`sub/dir/file.exe`) is allowed, but no absolute paths or traversal.
pub fn is_safe_resource_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') || name.contains(':') {
        return false;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return false;
    }
    path.components().all(|component| {
        matches!(component, std::path::Component::Normal(part) if part != std::ffi::OsStr::new(".."))
    })
}

/// Derive `{id,name,description,framework}` from an art package's manifest.json.
/// `framework` prefers `metadata.dependencies.framework`, else `execution.type`.
pub fn catalog_entry_from_manifest(manifest_bytes: &[u8]) -> Result<CatalogEntry, StoreError> {
    let value: serde_json::Value = serde_json::from_slice(manifest_bytes)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_owned();
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let framework = value
        .get("metadata")
        .and_then(|m| m.get("dependencies"))
        .and_then(|d| d.get("framework"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("execution")
                .and_then(|e| e.get("type"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    Ok(CatalogEntry {
        id,
        name,
        description,
        framework,
    })
}

/// Read the manifest.json bytes out of an art package zip.
pub fn read_manifest_bytes(zip_bytes: &[u8]) -> Result<Vec<u8>, StoreError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    let mut file = archive
        .by_name(MANIFEST_NAME)
        .map_err(|_| StoreError::MissingManifest)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Build the catalog by scanning `<root>/arts/*.zip` and reading each manifest.
/// Packages that fail to parse are skipped (a bad upload shouldn't 500 the list).
pub fn build_catalog(root: &Path) -> Result<Vec<CatalogEntry>, StoreError> {
    let arts_dir = root.join(ARTS_DIR);
    let mut entries = Vec::new();
    if !arts_dir.is_dir() {
        return Ok(entries);
    }
    for dir_entry in std::fs::read_dir(&arts_dir)? {
        let path = dir_entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("zip") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(manifest) = read_manifest_bytes(&bytes) else {
            continue;
        };
        let Ok(entry) = catalog_entry_from_manifest(&manifest) else {
            continue;
        };
        if entry.id.is_empty() {
            continue;
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}

/// Absolute path to an art package zip, validated for a safe id.
pub fn art_zip_path(root: &Path, id: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_art_id(id) {
        return Err(StoreError::InvalidArtId(id.to_owned()));
    }
    Ok(root.join(ARTS_DIR).join(format!("{id}.zip")))
}

/// Read an art package zip by id.
pub fn read_art_zip(root: &Path, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let path = art_zip_path(root, id)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Read the adjacent SHA-256 sidecar for an Art package. Legacy store roots
/// that predate sidecars remain readable: the digest is synthesized from the
/// ZIP without mutating the store.
pub fn read_art_zip_sha256(root: &Path, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let zip_path = art_zip_path(root, id)?;
    let sidecar_path = zip_path.with_extension("zip.sha256");
    match std::fs::read(&sidecar_path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(zip_bytes) = read_art_zip(root, id)? else {
                return Ok(None);
            };
            Ok(Some(art_zip_sha256_sidecar(id, &zip_bytes).into_bytes()))
        }
        Err(error) => Err(error.into()),
    }
}

fn art_zip_sha256_sidecar(id: &str, zip_bytes: &[u8]) -> String {
    let digest = Sha256::digest(zip_bytes);
    format!("{digest:x}  {id}.zip\n")
}

/// Absolute path to a third-party binary resource, validated for a safe name.
pub fn binary_path(root: &Path, name: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_resource_name(name) {
        return Err(StoreError::InvalidResourceName(name.to_owned()));
    }
    Ok(root.join(BINARIES_DIR).join(name))
}

/// Read a third-party binary resource by name.
pub fn read_binary(root: &Path, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let path = binary_path(root, name)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Absolute path to a framework package ZIP, validated for a safe id.
pub fn framework_package_path(root: &Path, id: &str) -> Result<PathBuf, StoreError> {
    if !is_safe_art_id(id) {
        return Err(StoreError::InvalidArtId(id.to_owned()));
    }
    Ok(root.join(FRAMEWORKS_DIR).join(format!("{id}.zip")))
}

/// Read a framework package ZIP by framework id.
pub fn read_framework_package(root: &Path, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
    let path = framework_package_path(root, id)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Persist a published art package: validate its manifest id matches `declared_id`
/// (when provided), then write `<root>/arts/<id>.zip`. Returns the stored id.
pub fn store_published_zip(
    root: &Path,
    declared_id: Option<&str>,
    zip_bytes: &[u8],
) -> Result<String, StoreError> {
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
    let arts_dir = root.join(ARTS_DIR);
    std::fs::create_dir_all(&arts_dir)?;
    let path = arts_dir.join(format!("{}.zip", entry.id));
    std::fs::write(&path, zip_bytes)?;
    std::fs::write(
        path.with_extension("zip.sha256"),
        art_zip_sha256_sidecar(&entry.id, zip_bytes),
    )?;
    Ok(entry.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_root() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "loom-art-store-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    fn build_zip(manifest: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file(MANIFEST_NAME, opts).unwrap();
            writer.write_all(manifest.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn safe_art_id_rejects_traversal_and_separators() {
        assert!(is_safe_art_id("pingo-art"));
        assert!(!is_safe_art_id("../evil"));
        assert!(!is_safe_art_id("a/b"));
        assert!(!is_safe_art_id("a\\b"));
        assert!(!is_safe_art_id(""));
    }

    #[test]
    fn safe_resource_name_allows_nesting_but_rejects_escape() {
        assert!(is_safe_resource_name("pingo.exe"));
        assert!(is_safe_resource_name("bin/pingo.exe"));
        assert!(!is_safe_resource_name("../pingo.exe"));
        assert!(!is_safe_resource_name("bin/../../pingo.exe"));
        assert!(!is_safe_resource_name("/etc/passwd"));
    }

    #[test]
    fn catalog_entry_prefers_declared_framework() {
        let manifest = r#"{"id":"wf","name":"WF","description":"d",
            "execution":{"type":"workflow","workflowId":"x"},
            "metadata":{"dependencies":{"framework":"workflow"}}}"#;
        let entry = catalog_entry_from_manifest(manifest.as_bytes()).unwrap();
        assert_eq!(entry.id, "wf");
        assert_eq!(entry.framework, "workflow");
    }

    #[test]
    fn catalog_entry_falls_back_to_execution_type() {
        let manifest = r#"{"id":"p","name":"P","description":"d",
            "execution":{"type":"cli_wrapper","command":"x","args":[]}}"#;
        let entry = catalog_entry_from_manifest(manifest.as_bytes()).unwrap();
        assert_eq!(entry.framework, "cli_wrapper");
    }

    #[test]
    fn build_catalog_lists_published_zips_sorted() {
        let root = temp_root();
        std::fs::create_dir_all(root.join(ARTS_DIR)).unwrap();
        let a = build_zip(
            r#"{"id":"b-art","name":"B","description":"d",
            "execution":{"type":"cli_wrapper","command":"x","args":[]}}"#,
        );
        let b = build_zip(
            r#"{"id":"a-art","name":"A","description":"d",
            "execution":{"type":"cloud_api","endpoint":"https://x","method":"POST"}}"#,
        );
        std::fs::write(root.join(ARTS_DIR).join("b-art.zip"), &a).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("a-art.zip"), &b).unwrap();
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].id, "a-art");
        assert_eq!(catalog[1].id, "b-art");
        assert_eq!(catalog[1].framework, "cli_wrapper");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn store_and_read_roundtrip() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"pingo-art","name":"Pingo","description":"d",
            "execution":{"type":"cli_wrapper","command":"bin/pingo.exe","args":[]}}"#,
        );
        let id = store_published_zip(&root, Some("pingo-art"), &zip).unwrap();
        assert_eq!(id, "pingo-art");
        let read = read_art_zip(&root, "pingo-art").unwrap().unwrap();
        assert_eq!(read, zip);
        let sidecar = read_art_zip_sha256(&root, "pingo-art").unwrap().unwrap();
        assert_eq!(
            String::from_utf8(sidecar).unwrap(),
            art_zip_sha256_sidecar("pingo-art", &zip)
        );
        assert!(read_art_zip(&root, "missing").unwrap().is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn digest_sidecar_is_synthesized_for_legacy_art_packages() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"legacy-art","name":"Legacy","description":"d",
            "execution":{"type":"cli_wrapper","command":"x","args":[]}}"#,
        );
        std::fs::create_dir_all(root.join(ARTS_DIR)).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("legacy-art.zip"), &zip).unwrap();

        let sidecar = read_art_zip_sha256(&root, "legacy-art").unwrap().unwrap();
        assert_eq!(
            String::from_utf8(sidecar).unwrap(),
            art_zip_sha256_sidecar("legacy-art", &zip)
        );
        assert!(!root.join(ARTS_DIR).join("legacy-art.zip.sha256").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_rejects_id_mismatch() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"real-id","name":"R","description":"d",
            "execution":{"type":"cli_wrapper","command":"x","args":[]}}"#,
        );
        let err = store_published_zip(&root, Some("claimed-id"), &zip).unwrap_err();
        assert!(matches!(err, StoreError::ArtIdMismatch { .. }));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_binary_validates_and_reads() {
        let root = temp_root();
        std::fs::create_dir_all(root.join(BINARIES_DIR)).unwrap();
        std::fs::write(root.join(BINARIES_DIR).join("pingo.exe"), b"MZ-fake").unwrap();
        let bytes = read_binary(&root, "pingo.exe").unwrap().unwrap();
        assert_eq!(bytes, b"MZ-fake");
        assert!(read_binary(&root, "missing.exe").unwrap().is_none());
        assert!(read_binary(&root, "../escape").is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}
