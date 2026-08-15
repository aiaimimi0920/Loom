use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

use crate::{McpCredentialRequirement, McpServerConfig, McpServerPackageState, McpTransport};

pub const MCP_SERVER_PACKAGE_MANIFEST: &str = "mcp.server.json";
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PACKAGE_FILES: usize = 128;
const MAX_EXTRACTED_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerPackageManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub publisher: McpPackagePublisher,
    pub transport: McpTransport,
    pub entry: McpPackageEntry,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub credentials: Vec<McpPackageCredential>,
}

impl McpServerPackageManifest {
    #[must_use]
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.publisher.id, self.id)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackagePublisher {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageEntry {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageCredential {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub required: bool,
    pub target: McpPackageCredentialTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageCredentialTarget {
    pub kind: McpPackageCredentialTargetKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpPackageCredentialTargetKind {
    Env,
    Header,
}

#[derive(Debug, Error)]
pub enum McpPackageError {
    #[error("MCP server package exceeds {MAX_PACKAGE_BYTES} bytes")]
    PackageTooLarge,
    #[error("MCP server package archive is invalid: {0}")]
    InvalidArchive(String),
    #[error("MCP server package path is unsafe: {0}")]
    UnsafePath(String),
    #[error("MCP server package manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("MCP server package entry is missing: {0}")]
    MissingEntry(String),
    #[error("MCP server package IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP server package JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn install_server_package(
    control_plane_root: &Path,
    package_bytes: &[u8],
) -> Result<McpServerConfig, McpPackageError> {
    if package_bytes.len() > MAX_PACKAGE_BYTES {
        return Err(McpPackageError::PackageTooLarge);
    }
    let digest = format!("{:x}", Sha256::digest(package_bytes));
    let staging_root = control_plane_root
        .join("mcp")
        .join("staging")
        .join(staging_name());
    fs::create_dir_all(&staging_root)?;
    let result = (|| {
        extract_package(package_bytes, &staging_root)?;
        let manifest_path = staging_root.join(MCP_SERVER_PACKAGE_MANIFEST);
        let manifest: McpServerPackageManifest = serde_json::from_slice(&fs::read(&manifest_path)?)
            .map_err(|error| {
                McpPackageError::InvalidManifest(format!(
                    "cannot parse {}: {error}",
                    manifest_path.display()
                ))
            })?;
        validate_manifest(&manifest, &staging_root)?;

        let package_root = control_plane_root
            .join("mcp")
            .join("packages")
            .join(&manifest.publisher.id)
            .join(&manifest.id);
        let versions_root = package_root.join("versions");
        fs::create_dir_all(&versions_root)?;
        let target_dir = versions_root.join(format!("{}-{}", manifest.version, &digest[..12]));
        if target_dir.exists() {
            fs::remove_dir_all(&staging_root)?;
        } else {
            fs::rename(&staging_root, &target_dir)?;
        }
        write_active_state(&package_root, &manifest, &digest, &target_dir)?;
        config_from_manifest(manifest, digest, target_dir)
    })();
    if staging_root.exists() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}

pub fn uninstall_server_package(
    control_plane_root: &Path,
    config: &McpServerConfig,
) -> Result<(), McpPackageError> {
    let Some(package) = &config.package else {
        return Ok(());
    };
    if !is_safe_identity(&package.publisher_id) {
        return Err(McpPackageError::InvalidManifest(
            "installed package publisher is unsafe".to_owned(),
        ));
    }
    let package_id = package
        .qualified_id
        .split_once('/')
        .filter(|(publisher, id)| *publisher == package.publisher_id && is_safe_identity(id))
        .map(|(_, id)| id)
        .ok_or_else(|| {
            McpPackageError::InvalidManifest(
                "installed package identity does not match its publisher".to_owned(),
            )
        })?;
    let package_root = control_plane_root
        .join("mcp")
        .join("packages")
        .join(&package.publisher_id)
        .join(package_id);
    let expected_versions_root = package_root.join("versions");
    if package.package_dir.parent() != Some(expected_versions_root.as_path()) {
        return Err(McpPackageError::InvalidManifest(
            "installed package directory is outside its package root".to_owned(),
        ));
    }
    if package_root.exists() {
        fs::remove_dir_all(&package_root)?;
    }
    Ok(())
}

fn config_from_manifest(
    manifest: McpServerPackageManifest,
    digest: String,
    target_dir: PathBuf,
) -> Result<McpServerConfig, McpPackageError> {
    let qualified_id = manifest.qualified_id();
    let mut config = match manifest.transport {
        McpTransport::Stdio => McpServerConfig::new(
            manifest.id.clone(),
            manifest.name.clone(),
            target_dir
                .join(&manifest.entry.command)
                .display()
                .to_string(),
        ),
        McpTransport::StreamableHttp => McpServerConfig::remote(
            manifest.id.clone(),
            manifest.name.clone(),
            manifest.entry.url.clone(),
        ),
    };
    config.description = manifest.description;
    config.args = manifest.entry.args;
    config.tools = manifest.tools;
    for credential in manifest.credentials {
        match credential.target.kind {
            McpPackageCredentialTargetKind::Env => {
                config
                    .credential_env
                    .insert(credential.target.name, credential.id.clone());
            }
            McpPackageCredentialTargetKind::Header => {
                config
                    .credential_headers
                    .insert(credential.target.name, credential.id.clone());
            }
        }
        config
            .credential_requirements
            .push(McpCredentialRequirement {
                id: credential.id,
                label: credential.label,
                required: credential.required,
            });
    }
    config.package = Some(McpServerPackageState {
        qualified_id,
        publisher_id: manifest.publisher.id,
        version: manifest.version,
        digest,
        package_dir: target_dir,
    });
    config
        .validate()
        .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    Ok(config)
}

fn validate_manifest(
    manifest: &McpServerPackageManifest,
    staging_root: &Path,
) -> Result<(), McpPackageError> {
    if manifest.schema_version != 1 {
        return Err(McpPackageError::InvalidManifest(
            "schemaVersion must be 1".to_owned(),
        ));
    }
    for (label, value) in [
        ("id", manifest.id.as_str()),
        ("publisher.id", manifest.publisher.id.as_str()),
    ] {
        if !is_safe_identity(value) {
            return Err(McpPackageError::InvalidManifest(format!(
                "{label} is not a safe package identity"
            )));
        }
    }
    Version::parse(&manifest.version).map_err(|error| {
        McpPackageError::InvalidManifest(format!("version must be SemVer: {error}"))
    })?;
    if manifest.name.trim().is_empty() {
        return Err(McpPackageError::InvalidManifest(
            "name is required".to_owned(),
        ));
    }
    let mut credential_ids = BTreeMap::new();
    for credential in &manifest.credentials {
        if !is_safe_identity(&credential.id) || credential.target.name.trim().is_empty() {
            return Err(McpPackageError::InvalidManifest(
                "credential id and target name are required".to_owned(),
            ));
        }
        if credential_ids.insert(&credential.id, ()).is_some() {
            return Err(McpPackageError::InvalidManifest(format!(
                "duplicate credential id `{}`",
                credential.id
            )));
        }
    }
    match manifest.transport {
        McpTransport::Stdio => {
            let command = safe_relative_path(&manifest.entry.command)?;
            if !staging_root.join(&command).is_file() {
                return Err(McpPackageError::MissingEntry(command.display().to_string()));
            }
        }
        McpTransport::StreamableHttp if manifest.entry.url.trim().is_empty() => {
            return Err(McpPackageError::InvalidManifest(
                "streamable-http entry.url is required".to_owned(),
            ));
        }
        McpTransport::StreamableHttp => {}
    }
    Ok(())
}

fn extract_package(package_bytes: &[u8], staging_root: &Path) -> Result<(), McpPackageError> {
    let mut archive = ZipArchive::new(Cursor::new(package_bytes))
        .map_err(|error| McpPackageError::InvalidArchive(error.to_string()))?;
    if archive.len() > MAX_PACKAGE_FILES {
        return Err(McpPackageError::InvalidArchive(format!(
            "archive contains more than {MAX_PACKAGE_FILES} entries"
        )));
    }
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| McpPackageError::InvalidArchive(error.to_string()))?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| McpPackageError::UnsafePath(entry.name().to_owned()))?
            .to_path_buf();
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(McpPackageError::UnsafePath(entry.name().to_owned()));
        }
        extracted_bytes = extracted_bytes.saturating_add(entry.size());
        if extracted_bytes > MAX_EXTRACTED_BYTES {
            return Err(McpPackageError::InvalidArchive(format!(
                "extracted content exceeds {MAX_EXTRACTED_BYTES} bytes"
            )));
        }
        let target = staging_root.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, McpPackageError> {
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

fn write_active_state(
    package_root: &Path,
    manifest: &McpServerPackageManifest,
    digest: &str,
    target_dir: &Path,
) -> Result<(), McpPackageError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ActiveState<'a> {
        qualified_id: String,
        version: &'a str,
        digest: &'a str,
        package_dir: &'a Path,
    }
    fs::create_dir_all(package_root)?;
    let path = package_root.join("active.json");
    let temporary = package_root.join("active.json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&ActiveState {
            qualified_id: manifest.qualified_id(),
            version: &manifest.version,
            digest,
            package_dir: target_dir,
        })?,
    )?;
    replace_file(&temporary, &path)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = fs::canonicalize(source)?;
    let destination_parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "MCP package state path has no parent",
        )
    })?;
    let destination =
        fs::canonicalize(destination_parent)?.join(destination.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "MCP package state path has no file name",
            )
        })?);
    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn is_safe_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn staging_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn package_bytes(manifest: &str, script: &[u8]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = SimpleFileOptions::default();
            zip.start_file(MCP_SERVER_PACKAGE_MANIFEST, options)
                .expect("manifest entry");
            zip.write_all(manifest.as_bytes()).expect("manifest bytes");
            zip.start_file("runtime/server.ps1", options)
                .expect("runtime entry");
            zip.write_all(script).expect("runtime bytes");
            zip.finish().expect("finish zip");
        }
        bytes.into_inner()
    }

    #[test]
    fn installs_independent_mcp_server_package() {
        let root = std::env::temp_dir().join(staging_name());
        let bytes = package_bytes(
            r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]},
                "tools":["search"],
                "credentials":[{"id":"api_key","label":"API Key","required":true,"target":{"kind":"env","name":"API_KEY"}}]
            }"#,
            b"Write-Output ready",
        );

        let config = install_server_package(&root, &bytes).expect("install package");

        assert_eq!(config.id, "fixture-search");
        assert_eq!(config.tools, vec!["search"]);
        assert_eq!(config.credential_env["API_KEY"], "api_key");
        assert_eq!(
            config.package.as_ref().expect("package state").qualified_id,
            "publisher.test/fixture-search"
        );
        assert!(Path::new(&config.command).is_file());
        uninstall_server_package(&root, &config).expect("uninstall package");
        assert!(!root
            .join("mcp/packages/publisher.test/fixture-search")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_package_path_traversal() {
        let root = std::env::temp_dir().join(staging_name());
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            zip.start_file("../outside.txt", SimpleFileOptions::default())
                .expect("unsafe entry");
            zip.write_all(b"unsafe").expect("unsafe bytes");
            zip.finish().expect("finish zip");
        }
        let error = install_server_package(&root, &bytes.into_inner())
            .expect_err("path traversal must fail");
        assert!(matches!(error, McpPackageError::UnsafePath(_)));
        let _ = fs::remove_dir_all(root);
    }
}
