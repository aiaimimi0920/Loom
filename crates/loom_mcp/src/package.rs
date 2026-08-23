use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use loom_plugin_security::{verify_package_signature, TrustStore};
use loom_protocol::{PackageSignature, PackageTrustStatus, PublisherIdentity};
use loom_security::archive::{extract_zip_securely, SecureZipError};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

use crate::{
    validate_mcp_environment_name, validate_mcp_header_name, validate_mcp_tool_identifier,
    McpCredentialRequirement, McpServerConfig, McpServerPackageState, McpTransport,
    MAX_MCP_ARGUMENTS, MAX_MCP_ARGUMENT_BYTES, MAX_MCP_CREDENTIALS, MAX_MCP_CREDENTIAL_LABEL_BYTES,
    MAX_MCP_SERVER_DESCRIPTION_BYTES, MAX_MCP_SERVER_NAME_BYTES, MAX_MCP_TOOLS,
};

pub const MCP_SERVER_PACKAGE_MANIFEST: &str = "mcp.server.json";
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
/// Entry-count ceiling for a server package archive, matched to the shared extractor's own limit.
///
/// It was 128, which no real MCP server fits: an npm or Python server vendors its dependencies, and a
/// dependency tree is thousands of files before it is anything. The cap that mattered was never this
/// one anyway — `extract_zip_securely` enforces 4096 entries, per-entry and total size limits, and a
/// compression-ratio check — so 128 only turned normal packages away, and did it at install time with
/// a message about entry counts rather than anything a publisher could act on.
const MAX_PACKAGE_FILES: usize = 4096;
const MAX_EXTRACTED_BYTES: u64 = 128 * 1024 * 1024;

/// How much of the archive digest names the version directory.
///
/// The name has to be unique per archive, because two archives landing on one directory means one of
/// them runs the other's files. Twelve hex characters — 48 bits — was not enough for that: about 2^24
/// hashes finds two packages sharing a version string and a prefix, which is minutes of work rather
/// than an attack. Thirty-two characters is 128 bits, so a collision is out of reach, and the rest of
/// the digest is not spent on the path: an MCP server that vendors its dependencies nests deeply
/// inside this directory, and every character here comes out of the `MAX_PATH` budget those files
/// need. The full digest is recorded in `active.json` and in the server config either way.
pub const PACKAGE_DIRECTORY_DIGEST_CHARS: usize = 32;

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
    /// Publisher signature over the package, in the shape Art packages already use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_security: Option<McpPackageSecurity>,
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

/// The signature block of an MCP server package manifest.
///
/// This is deliberately the same `PackageSignature` an Art's `metadata.packageSecurity` carries, so
/// one signing tool, one trust store, and one verifier serve both package kinds. Unlike the Art
/// block it holds no publisher identity: the manifest already names its publisher, and letting the
/// security block name a second one would only create two places to disagree.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageSecurity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<PackageSignature>,
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
    #[error("MCP server package integrity check failed: {0}")]
    Integrity(String),
    #[error("MCP server package trust check failed: {0}")]
    Trust(String),
    #[error("MCP server package IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP server package JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// The persisted `active.json` state for an installed MCP server package.
///
/// The installer used to write this file and nobody read it back, which made it a decoration
/// rather than a record: the digests it carried were never compared against anything. It is now
/// the authoritative copy of what was installed, and `verify_installed_entry` reads it before a
/// package-backed server is spawned.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPackageActiveState {
    pub qualified_id: String,
    pub version: String,
    pub digest: String,
    pub package_dir: PathBuf,
    /// SHA-256 of every extracted file, keyed by its package-relative path with `/` separators.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// What the trust check concluded at install time, so a later reader does not have to re-verify
    /// a signature to say whether the package was signed and by whom.
    #[serde(default)]
    pub trust_status: PackageTrustStatus,
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
        let trust_status = verify_package_trust(control_plane_root, &manifest, &staging_root)?;
        let files = digest_tree(&staging_root)?;

        let package_root = control_plane_root
            .join("mcp")
            .join("packages")
            .join(&manifest.publisher.id)
            .join(&manifest.id);
        let versions_root = package_root.join("versions");
        fs::create_dir_all(&versions_root)?;
        let target_dir = versions_root.join(format!(
            "{}-{}",
            manifest.version,
            &digest[..PACKAGE_DIRECTORY_DIGEST_CHARS]
        ));
        if target_dir.exists() {
            // The archive digest names this directory, so an existing one must hold exactly the
            // bytes just extracted. Checking that is what makes reinstalling a package a repair
            // instead of a no-op that keeps a tree somebody else edited.
            verify_tree_digests(&target_dir, &files)?;
            fs::remove_dir_all(&staging_root)?;
        } else {
            fs::rename(&staging_root, &target_dir)?;
        }
        write_active_state(
            &package_root,
            &manifest,
            &digest,
            &target_dir,
            &files,
            &trust_status,
        )?;
        config_from_manifest(manifest, digest, target_dir, files, trust_status)
    })();
    if staging_root.exists() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}

/// Read the `active.json` written for an installed package.
pub fn read_active_state(
    control_plane_root: &Path,
    publisher_id: &str,
    package_id: &str,
) -> Result<McpPackageActiveState, McpPackageError> {
    if !is_safe_identity(publisher_id) || !is_safe_identity(package_id) {
        return Err(McpPackageError::InvalidManifest(format!(
            "`{publisher_id}/{package_id}` is not a safe package identity"
        )));
    }
    read_active_state_file(
        &control_plane_root
            .join("mcp")
            .join("packages")
            .join(publisher_id)
            .join(package_id)
            .join("active.json"),
    )
}

/// Re-verify a package-backed stdio server's entry file against the digest recorded at install.
///
/// For a stdio transport the manifest names an executable that becomes the server's command, and
/// the daemon spawns it with the user's credentials in its environment. Both `servers.json` and the
/// version directory it points at are ordinary files in the control plane, so the command deserved
/// no standing trust: the installer now records a digest per extracted file, `active.json` holds
/// the authoritative copy of that record, and this runs before every spawn. An entry script swapped
/// underneath an otherwise untouched package is refused instead of executed.
///
/// This is not a substitute for signing the package — a writer who can edit the version directory
/// can edit `active.json` beside it. It closes the case where only the executable is replaced, and
/// it is the check a publisher signature would hang off once packages carry one.
///
/// The command is also re-anchored inside the package directory here rather than trusted from
/// `servers.json`, which is the other half of the same problem: the registry row could keep an
/// installed package's publisher, version, and digest while pointing `command` at any file on the
/// machine, and the operator UI would still present it as that package.
pub fn verify_installed_entry(config: &McpServerConfig) -> Result<(), McpPackageError> {
    let Some(package) = &config.package else {
        return Ok(());
    };
    if config.transport != McpTransport::Stdio {
        return Ok(());
    }
    let command = Path::new(&config.command);
    let entry_key = command
        .strip_prefix(&package.package_dir)
        .ok()
        .and_then(package_key)
        .ok_or_else(|| {
            McpPackageError::Integrity(format!(
                "installed server `{}` runs `{}`, which is not inside its package directory `{}`",
                config.id,
                config.command,
                package.package_dir.display()
            ))
        })?;
    // The check above is lexical, so a link inside the package directory still satisfies it while
    // resolving somewhere else entirely. Compare the resolved paths as well.
    let resolved_package_dir = fs::canonicalize(&package.package_dir).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` cannot resolve its package directory `{}`: {error}",
            config.id,
            package.package_dir.display()
        ))
    })?;
    let resolved_command = fs::canonicalize(command).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` cannot resolve its entry `{entry_key}`: {error}",
            config.id
        ))
    })?;
    if !resolved_command.starts_with(&resolved_package_dir) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` resolves to `{}`, outside its package directory `{}`",
            config.id,
            resolved_command.display(),
            resolved_package_dir.display()
        )));
    }
    // An extensionless command is resolved by the platform: Windows appends every `PATHEXT` entry
    // in turn, so `runtime/server` can start `runtime/server.exe` — a different file than the one
    // this hashes. Packages name their entry point exactly.
    #[cfg(windows)]
    if command.extension().is_none() {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` has no file extension, so Windows could \
             resolve it to a different file than the one that was installed",
            config.id
        )));
    }
    // `.bat` and `.cmd` are not run directly: `std::process::Command` hands them to `cmd.exe`, so the
    // only thing between manifest-supplied `args` and a shell command line is the standard library's
    // batch-argument escaping. A package names its own entry point, so it can ship an executable or a
    // `.ps1` script instead and not depend on that one mitigation. Unpackaged servers keep batch files,
    // because on Windows `npx` and `npm` *are* `.cmd` shims and refusing them would rule out most of
    // the MCP ecosystem — which is also why this check lives here rather than in the spawn path.
    if names_a_batch_file(command) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` is a batch file, which Windows runs through \
             `cmd.exe`",
            config.id
        )));
    }
    let package_root = package
        .package_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            McpPackageError::Integrity(format!(
                "installed server `{}` has no package root above `{}`",
                config.id,
                package.package_dir.display()
            ))
        })?;
    let active = read_active_state_file(&package_root.join("active.json")).map_err(|error| {
        McpPackageError::Integrity(format!(
            "installed server `{}` has no readable package state: {error}",
            config.id
        ))
    })?;
    if active.qualified_id != package.qualified_id
        || active.version != package.version
        || !active.digest.eq_ignore_ascii_case(&package.digest)
        || active.package_dir != package.package_dir
    {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` does not match the recorded state of package `{}`",
            config.id, package.qualified_id
        )));
    }
    let expected = active.files.get(&entry_key).ok_or_else(|| {
        // A package installed before digests were recorded lands here. Refusing it is deliberate:
        // the alternative is spawning an unverifiable executable, and reinstalling the package
        // restores the record.
        McpPackageError::Integrity(format!(
            "installed server `{}` has no recorded digest for its entry `{entry_key}`; reinstall the package",
            config.id
        ))
    })?;
    if let Some(recorded) = package.files.get(&entry_key) {
        if !recorded.eq_ignore_ascii_case(expected) {
            return Err(McpPackageError::Integrity(format!(
                "installed server `{}` and package `{}` disagree on the digest of `{entry_key}`",
                config.id, package.qualified_id
            )));
        }
    }
    if !file_digest(command)?.eq_ignore_ascii_case(expected) {
        return Err(McpPackageError::Integrity(format!(
            "installed server `{}` entry `{entry_key}` does not match its recorded digest",
            config.id
        )));
    }
    Ok(())
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
    files: BTreeMap<String, String>,
    trust_status: PackageTrustStatus,
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
        files,
        trust_status,
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
    validate_manifest_text("name", &manifest.name, MAX_MCP_SERVER_NAME_BYTES, true)?;
    validate_manifest_text(
        "description",
        &manifest.description,
        MAX_MCP_SERVER_DESCRIPTION_BYTES,
        false,
    )?;
    validate_manifest_text(
        "publisher.name",
        &manifest.publisher.name,
        MAX_MCP_SERVER_NAME_BYTES,
        true,
    )?;
    if manifest.tools.len() > MAX_MCP_TOOLS {
        return Err(McpPackageError::InvalidManifest(format!(
            "tools contains {} entries; limit is {MAX_MCP_TOOLS}",
            manifest.tools.len()
        )));
    }
    for (index, tool) in manifest.tools.iter().enumerate() {
        validate_mcp_tool_identifier(&format!("tools[{index}]"), tool)
            .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    }
    if manifest.entry.args.len() > MAX_MCP_ARGUMENTS {
        return Err(McpPackageError::InvalidManifest(format!(
            "entry.args contains {} entries; limit is {MAX_MCP_ARGUMENTS}",
            manifest.entry.args.len()
        )));
    }
    for (index, argument) in manifest.entry.args.iter().enumerate() {
        validate_manifest_text(
            &format!("entry.args[{index}]"),
            argument,
            MAX_MCP_ARGUMENT_BYTES,
            false,
        )?;
    }
    if manifest.credentials.len() > MAX_MCP_CREDENTIALS {
        return Err(McpPackageError::InvalidManifest(format!(
            "credentials contains {} entries; limit is {MAX_MCP_CREDENTIALS}",
            manifest.credentials.len()
        )));
    }
    let mut credential_ids = BTreeMap::new();
    for (index, credential) in manifest.credentials.iter().enumerate() {
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
        validate_manifest_text(
            &format!("credentials[{index}].label"),
            &credential.label,
            MAX_MCP_CREDENTIAL_LABEL_BYTES,
            true,
        )?;
        match credential.target.kind {
            McpPackageCredentialTargetKind::Env => {
                validate_mcp_environment_name(&credential.target.name)
            }
            McpPackageCredentialTargetKind::Header => {
                validate_mcp_header_name(&credential.target.name)
            }
        }
        .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    }
    match manifest.transport {
        McpTransport::Stdio => {
            let command = safe_relative_path(&manifest.entry.command)?;
            if names_a_batch_file(&command) {
                return Err(McpPackageError::InvalidManifest(format!(
                    "entry.command `{}` is a batch file, which Windows runs through `cmd.exe`; \
                     name an executable or a `.ps1` script instead",
                    command.display()
                )));
            }
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

fn validate_manifest_text(
    field: &str,
    value: &str,
    max_bytes: usize,
    required: bool,
) -> Result<(), McpPackageError> {
    if required && value.trim().is_empty() {
        return Err(McpPackageError::InvalidManifest(format!(
            "{field} is required"
        )));
    }
    if value.len() > max_bytes {
        return Err(McpPackageError::InvalidManifest(format!(
            "{field} is {} bytes; limit is {max_bytes}",
            value.len()
        )));
    }
    Ok(())
}

/// Verify a staged MCP server package against the same trust store Art packages use.
///
/// MCP server packages arrive from the same places Arts do, so they get the same chain: an optional
/// `packageSecurity.signature` in the manifest, verified against `plugin-trust.json`, after which
/// the store's effective policy decides whether the resulting status is acceptable. Until now the
/// installer accepted any zip that parsed, which meant an operator who set `require-signed` or
/// `require-trusted` had that setting honoured for Arts and quietly ignored for MCP servers.
///
/// The publisher passed to the verifier is the one the manifest names, so a signature made with a
/// key this machine already trusts for that publisher reaches `Trusted` rather than stopping at
/// `Verified`. The default policy is `allow-unsigned`, so an unsigned package still installs
/// exactly as it did before.
fn verify_package_trust(
    control_plane_root: &Path,
    manifest: &McpServerPackageManifest,
    staging_root: &Path,
) -> Result<PackageTrustStatus, McpPackageError> {
    let signature = manifest
        .package_security
        .as_ref()
        .and_then(|security| security.signature.as_ref());
    let publisher = PublisherIdentity {
        id: manifest.publisher.id.clone(),
        name: Some(manifest.publisher.name.clone()),
        website: None,
        key_id: signature.map(|signature| signature.key_id.clone()),
    };
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    let status = verify_package_signature(staging_root, Some(&publisher), signature, &trust_store)
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    if let Some(signature) = signature {
        enforce_publisher_key_binding(&trust_store, &manifest.publisher.id, signature)?;
    }
    trust_store
        .effective_policy()
        .enforce(status.clone())
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    Ok(status)
}

/// Refuse a signature whose key is not one this machine records for the publisher it names.
///
/// `verify_package_signature` reports an unknown `(publisher, key)` pair as `Verified`: the signature
/// checks out, so the package is signed, just not by anyone this machine has an opinion about. That
/// is the right answer for an unknown publisher and the wrong one for a known publisher, because it
/// lets any valid key claim a name the operator has already pinned. Under `require-signed` such a
/// package would install and then be presented under the borrowed publisher's name.
///
/// A publisher with no records at all is left alone: there is no pinned key to contradict, so the
/// policy alone decides whether `Verified` is enough.
fn enforce_publisher_key_binding(
    trust_store: &TrustStore,
    publisher_id: &str,
    signature: &PackageSignature,
) -> Result<(), McpPackageError> {
    let mut recorded = trust_store
        .publishers
        .iter()
        .filter(|record| record.publisher_id == publisher_id)
        .peekable();
    if recorded.peek().is_none() {
        return Ok(());
    }
    if recorded.any(|record| record.key_id == signature.key_id) {
        return Ok(());
    }
    Err(McpPackageError::Trust(format!(
        "package claims publisher `{publisher_id}` but is signed with key `{}`, which this machine \
         does not record for that publisher",
        signature.key_id
    )))
}

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
fn extract_package(package_bytes: &[u8], staging_root: &Path) -> Result<(), McpPackageError> {
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

fn package_error_from_secure_zip(error: SecureZipError) -> McpPackageError {
    match error {
        SecureZipError::UnsafePath(value)
        | SecureZipError::UnsafeWindowsName(value)
        | SecureZipError::SymbolicLink(value)
        | SecureZipError::DuplicatePath(value) => McpPackageError::UnsafePath(value),
        SecureZipError::Io(error) => McpPackageError::Io(error),
        other => McpPackageError::InvalidArchive(other.to_string()),
    }
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
    files: &BTreeMap<String, String>,
    trust_status: &PackageTrustStatus,
) -> Result<(), McpPackageError> {
    fs::create_dir_all(package_root)?;
    let path = package_root.join("active.json");
    // The temporary name carries a nonce. A constant one is shared by every concurrent install of the
    // same package — a retry racing its own first attempt is enough to have two — and then both write
    // the same file and whichever rename lands second publishes a mix of the two payloads. This is the
    // nonce the staging directory already uses.
    let temporary = package_root.join(format!("active.json.{}.tmp", staging_name()));
    let payload = serde_json::to_vec_pretty(&McpPackageActiveState {
        qualified_id: manifest.qualified_id(),
        version: manifest.version.clone(),
        digest: digest.to_owned(),
        package_dir: target_dir.to_path_buf(),
        files: files.clone(),
        trust_status: trust_status.clone(),
    })?;
    // Synced before the rename, the way `write_tools` and the zip extractor both do it:
    // `MOVEFILE_WRITE_THROUGH` flushes the rename, not the bytes of the file being renamed, so a crash
    // in between could otherwise leave an `active.json` that is present and empty. That reads back as a
    // package with no recorded digests, which is the one state a spawn refuses outright.
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(&payload)?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = replace_file(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// Whether a command names a Windows batch file, which is executed by `cmd.exe` rather than directly.
///
/// The extension is compared case-insensitively because Windows treats `SERVER.CMD` and `server.cmd`
/// as the same file, and the check is not `cfg(windows)`-gated: a package installs on one platform and
/// may be inspected on another, and a batch entry is wrong for the package either way.
fn names_a_batch_file(command: &Path) -> bool {
    command
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("bat") || value.eq_ignore_ascii_case("cmd"))
}

fn read_active_state_file(path: &Path) -> Result<McpPackageActiveState, McpPackageError> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|error| {
        McpPackageError::InvalidManifest(format!("cannot parse {}: {error}", path.display()))
    })
}

/// Hash every extracted file, keyed by its package-relative path with `/` separators.
///
/// The archive digest says what was downloaded; it says nothing about what is on disk now. These
/// per-file digests are what let a later spawn notice that one file inside an installed package was
/// replaced while the manifest, the version directory name, and the archive digest all still agree.
fn digest_tree(root: &Path) -> Result<BTreeMap<String, String>, McpPackageError> {
    let mut files = BTreeMap::new();
    collect_tree_digests(root, root, &mut files)?;
    Ok(files)
}

fn collect_tree_digests(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), McpPackageError> {
    let mut entries = fs::read_dir(directory)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        // `symlink_metadata` rather than `metadata`, so a link is seen as a link instead of being
        // followed to whatever it points at. The shared extractor already rejects links, so one
        // here was not produced by it.
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_tree_digests(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(McpPackageError::UnsafePath(path.display().to_string()));
        }
        let key = path
            .strip_prefix(root)
            .ok()
            .and_then(package_key)
            .ok_or_else(|| McpPackageError::UnsafePath(path.display().to_string()))?;
        let digest = file_digest(&path)?;
        files.insert(key, digest);
    }
    Ok(())
}

fn verify_tree_digests(
    root: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<(), McpPackageError> {
    let actual = digest_tree(root)?;
    for (key, digest) in expected {
        match actual.get(key) {
            Some(found) if found.eq_ignore_ascii_case(digest) => {}
            Some(_) => {
                return Err(McpPackageError::Integrity(format!(
                    "`{key}` in {} does not match its recorded digest",
                    root.display()
                )))
            }
            None => {
                return Err(McpPackageError::Integrity(format!(
                    "`{key}` is missing from {}",
                    root.display()
                )))
            }
        }
    }
    if let Some(extra) = actual.keys().find(|key| !expected.contains_key(*key)) {
        return Err(McpPackageError::Integrity(format!(
            "`{extra}` in {} was not part of the package",
            root.display()
        )));
    }
    Ok(())
}

/// Turn a package-relative path into its `active.json` key, rejecting anything but plain names.
fn package_key(relative: &Path) -> Option<String> {
    let mut key = String::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        let value = value.to_str()?;
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(value);
    }
    (!key.is_empty()).then_some(key)
}

fn file_digest(path: &Path) -> Result<String, McpPackageError> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
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
    use loom_plugin_security::{
        generate_signing_key, sign_package, SigningKeyDocument, TrustPolicy,
    };
    use loom_protocol::PublisherTrustRecord;
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    fn package_bytes(manifest: &str, script: &[u8]) -> Vec<u8> {
        package_bytes_with_entry(manifest, "runtime/server.ps1", script)
    }

    fn package_bytes_with_entry(manifest: &str, entry: &str, script: &[u8]) -> Vec<u8> {
        package_bytes_with_files(&[
            (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
            (entry, script),
        ])
    }

    fn package_bytes_with_files(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = SimpleFileOptions::default();
            for (name, contents) in files {
                zip.start_file(*name, options).expect("zip entry");
                zip.write_all(contents).expect("zip bytes");
            }
            zip.finish().expect("finish zip");
        }
        bytes.into_inner()
    }

    const SIGNATURE_FILE: &str = "package.signature.json";

    fn signed_manifest(key_id: &str) -> String {
        format!(
            r#"{{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{{"id":"publisher.test","name":"Publisher"}},
                "transport":"stdio",
                "entry":{{"command":"runtime/server.ps1","args":[]}},
                "packageSecurity":{{"signature":{{"algorithm":"ed25519","keyId":"{key_id}","file":"{SIGNATURE_FILE}"}}}}
            }}"#
        )
    }

    /// Build a package the way a publisher would: lay the tree out, sign it, then archive the tree
    /// together with the signature document `sign_package` wrote.
    fn signed_package_bytes(key: &SigningKeyDocument, script: &[u8]) -> Vec<u8> {
        let manifest = signed_manifest(&key.key_id);
        let source = std::env::temp_dir().join(staging_name());
        fs::create_dir_all(source.join("runtime")).expect("create source tree");
        fs::write(source.join(MCP_SERVER_PACKAGE_MANIFEST), &manifest).expect("write manifest");
        fs::write(source.join("runtime").join("server.ps1"), script).expect("write entry");
        sign_package(&source, SIGNATURE_FILE, key).expect("sign package");
        let signature = fs::read(source.join(SIGNATURE_FILE)).expect("read signature document");
        let _ = fs::remove_dir_all(&source);
        package_bytes_with_files(&[
            (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
            ("runtime/server.ps1", script),
            (SIGNATURE_FILE, &signature),
        ])
    }

    fn write_trust_store(root: &Path, store: &TrustStore) {
        store
            .write_atomic(&root.join("plugin-trust.json"))
            .expect("write trust store");
    }

    fn stdio_package_bytes() -> Vec<u8> {
        package_bytes(
            r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]}
            }"#,
            b"Write-Output ready",
        )
    }

    #[test]
    fn manifest_validation_bounds_tools_arguments_and_credential_labels() {
        let root = std::env::temp_dir().join(staging_name());
        fs::create_dir_all(root.join("runtime")).expect("create package validation fixture");
        fs::write(root.join("runtime/server.ps1"), b"Write-Output ready")
            .expect("write package validation entry");
        let mut manifest: McpServerPackageManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "id": "fixture-search",
            "name": "Fixture Search",
            "description": "bounded",
            "version": "1.2.3",
            "publisher": {"id": "publisher.test", "name": "Publisher"},
            "transport": "stdio",
            "entry": {"command": "runtime/server.ps1", "args": []},
            "tools": [],
            "credentials": []
        }))
        .expect("parse package validation manifest");

        manifest.tools = vec!["search".to_owned(); MAX_MCP_TOOLS + 1];
        assert!(validate_manifest(&manifest, &root)
            .unwrap_err()
            .to_string()
            .contains("tools contains"));

        manifest.tools.clear();
        manifest.entry.args = vec!["argument".to_owned(); MAX_MCP_ARGUMENTS + 1];
        assert!(validate_manifest(&manifest, &root)
            .unwrap_err()
            .to_string()
            .contains("entry.args"));

        manifest.entry.args.clear();
        manifest.credentials = vec![McpPackageCredential {
            id: "api_key".to_owned(),
            label: "x".repeat(MAX_MCP_CREDENTIAL_LABEL_BYTES + 1),
            required: true,
            target: McpPackageCredentialTarget {
                kind: McpPackageCredentialTargetKind::Env,
                name: "API_KEY".to_owned(),
            },
        }];
        assert!(validate_manifest(&manifest, &root)
            .unwrap_err()
            .to_string()
            .contains("credentials[0].label"));
        fs::remove_dir_all(root).expect("cleanup package validation fixture");
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

        // Every extracted file is hashed at install, and the persisted state carries the same
        // record the returned config does.
        let state = config.package.as_ref().expect("package state");
        assert_eq!(
            state.files.keys().collect::<Vec<_>>(),
            vec![MCP_SERVER_PACKAGE_MANIFEST, "runtime/server.ps1"]
        );
        assert_eq!(
            state.files["runtime/server.ps1"],
            format!("{:x}", Sha256::digest(b"Write-Output ready"))
        );
        let active = read_active_state(&root, "publisher.test", "fixture-search")
            .expect("read active state");
        assert_eq!(active.files, state.files);
        assert_eq!(active.digest, state.digest);
        assert_eq!(active.trust_status, PackageTrustStatus::Unsigned);
        verify_installed_entry(&config).expect("entry matches its recorded digest");

        uninstall_server_package(&root, &config).expect("uninstall package");
        assert!(!root
            .join("mcp/packages/publisher.test/fixture-search")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_an_unsigned_package_when_the_trust_policy_requires_a_signature() {
        // The trust policy used to apply to Arts only, so an operator who required signatures still
        // got unsigned MCP servers installed without a word.
        let root = std::env::temp_dir().join(staging_name());
        write_trust_store(
            &root,
            &TrustStore {
                policy: TrustPolicy::RequireSigned,
                ..TrustStore::default()
            },
        );

        let error = match install_server_package(&root, &stdio_package_bytes()) {
            Ok(_) => panic!("an unsigned package must not install under require-signed"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, McpPackageError::Trust(_)),
            "unexpected error: {error}"
        );
        assert!(!root
            .join("mcp/packages/publisher.test/fixture-search")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installs_a_signed_package_when_the_trust_policy_requires_a_signature() {
        let root = std::env::temp_dir().join(staging_name());
        write_trust_store(
            &root,
            &TrustStore {
                policy: TrustPolicy::RequireSigned,
                ..TrustStore::default()
            },
        );
        let key = generate_signing_key("fixture-key");

        let config =
            install_server_package(&root, &signed_package_bytes(&key, b"Write-Output ready"))
                .expect("a signed package installs");

        assert_eq!(config.id, "fixture-search");
        // The signature document travels with the package and is hashed like any other file.
        let state = config.package.as_ref().expect("package state");
        assert!(state.files.contains_key(SIGNATURE_FILE));
        // A signature nobody has pinned a key for is `Verified`, and that verdict is persisted so a
        // reader does not have to re-verify the signature to report it.
        assert_eq!(state.trust_status, PackageTrustStatus::Verified);
        let active = read_active_state(&root, "publisher.test", "fixture-search")
            .expect("read active state");
        assert_eq!(active.trust_status, PackageTrustStatus::Verified);
        verify_installed_entry(&config).expect("entry matches its recorded digest");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_a_package_whose_files_changed_after_it_was_signed() {
        // Repacking a signed package with a different runtime file leaves the signature document
        // internally consistent; only the digest it covers gives the swap away. No policy is set
        // here, because a signature that does not match its package is a failure at any policy.
        let root = std::env::temp_dir().join(staging_name());
        let key = generate_signing_key("fixture-key");
        let signed = signed_package_bytes(&key, b"Write-Output ready");
        let mut archive = ZipArchive::new(Cursor::new(signed)).expect("open signed package");
        let mut signature = Vec::new();
        archive
            .by_name(SIGNATURE_FILE)
            .expect("signature entry")
            .read_to_end(&mut signature)
            .expect("read signature document");
        let manifest = signed_manifest(&key.key_id);
        let tampered = package_bytes_with_files(&[
            (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
            ("runtime/server.ps1", b"Write-Output tampered"),
            (SIGNATURE_FILE, &signature),
        ]);

        let error = match install_server_package(&root, &tampered) {
            Ok(_) => panic!("a package that no longer matches its signature must not install"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, McpPackageError::Trust(message) if message.contains("digest")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn requires_a_trusted_publisher_key_when_the_policy_asks_for_one() {
        let root = std::env::temp_dir().join(staging_name());
        let key = generate_signing_key("fixture-key");
        write_trust_store(
            &root,
            &TrustStore {
                policy: TrustPolicy::RequireTrusted,
                publishers: vec![PublisherTrustRecord {
                    publisher_id: "publisher.test".to_owned(),
                    key_id: key.key_id.clone(),
                    public_key: key.public_key.clone(),
                    revoked: false,
                }],
                ..TrustStore::default()
            },
        );

        install_server_package(&root, &signed_package_bytes(&key, b"Write-Output ready"))
            .expect("a package signed by a trusted key installs");

        let active = read_active_state(&root, "publisher.test", "fixture-search")
            .expect("read active state");
        assert_eq!(active.trust_status, PackageTrustStatus::Trusted);

        // A signature made with some other key is refused: the publisher named in the manifest has a
        // pinned key here, and this is not it.
        let other = generate_signing_key("other-key");
        let error =
            match install_server_package(&root, &signed_package_bytes(&other, b"Write-Output odd"))
            {
                Ok(_) => panic!("an untrusted key must not satisfy require-trusted"),
                Err(error) => error,
            };
        assert!(
            matches!(&error, McpPackageError::Trust(_)),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_a_signature_from_a_key_the_publisher_does_not_use() {
        // The verifier calls an unknown `(publisher, key)` pair `Verified`, which `require-signed`
        // accepts. Without a binding check, anyone holding any valid key could publish under a
        // publisher name the operator had already pinned.
        let root = std::env::temp_dir().join(staging_name());
        let pinned = generate_signing_key("pinned-key");
        write_trust_store(
            &root,
            &TrustStore {
                policy: TrustPolicy::RequireSigned,
                publishers: vec![PublisherTrustRecord {
                    publisher_id: "publisher.test".to_owned(),
                    key_id: pinned.key_id.clone(),
                    public_key: pinned.public_key.clone(),
                    revoked: false,
                }],
                ..TrustStore::default()
            },
        );
        let impostor = generate_signing_key("impostor-key");

        let error = match install_server_package(
            &root,
            &signed_package_bytes(&impostor, b"Write-Output odd"),
        ) {
            Ok(_) => panic!("a key that is not recorded for this publisher must be refused"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, McpPackageError::Trust(message)
                if message.contains("does not record for that publisher")),
            "unexpected error: {error}"
        );
        assert!(!root
            .join("mcp/packages/publisher.test/fixture-search")
            .exists());

        // The publisher's own key still installs, and a publisher nobody has pinned is unaffected:
        // the check only fires when there is a recorded key to contradict.
        install_server_package(&root, &signed_package_bytes(&pinned, b"Write-Output ready"))
            .expect("the recorded key installs");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_spawn_a_packaged_server_whose_command_points_outside_the_package() {
        // `servers.json` supplies the command, and a row that keeps the package block while
        // pointing `command` elsewhere was still presented in the UI as the installed package.
        let root = std::env::temp_dir().join(staging_name());
        let mut config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");
        let outside = root.join("outside.ps1");
        fs::write(&outside, b"Write-Output elsewhere").expect("write outside script");
        config.command = outside.display().to_string();

        let error = verify_installed_entry(&config)
            .expect_err("a command outside the package must be refused");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("not inside its package directory")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn refuses_to_spawn_a_packaged_entry_without_a_file_extension() {
        // Windows resolves an extensionless command through `PATHEXT`, so `runtime/server` can
        // start `runtime/server.exe`: a file this never hashed.
        let root = std::env::temp_dir().join(staging_name());
        let config = install_server_package(
            &root,
            &package_bytes_with_entry(
                r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server","args":[]}
            }"#,
                "runtime/server",
                b"Write-Output ready",
            ),
        )
        .expect("install package");

        let error = verify_installed_entry(&config)
            .expect_err("an extensionless packaged entry must be refused");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("no file extension")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_spawn_a_package_whose_entry_was_replaced() {
        // The command, its arguments, and its environment all come from `servers.json`, and the
        // version directory it points at is an ordinary directory in the control plane. Replacing
        // just the entry script left the manifest, the directory name, and the archive digest all
        // agreeing, so nothing noticed that the file about to run with the user's credentials was
        // not the file that was installed.
        let root = std::env::temp_dir().join(staging_name());
        let config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");

        fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

        let error = verify_installed_entry(&config).expect_err("a replaced entry must be refused");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("runtime/server.ps1")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_spawn_a_package_with_no_recorded_digests() {
        // The shape a package installed before digests were recorded has. Refusing it is
        // deliberate: the alternative is spawning an executable nothing can vouch for.
        let root = std::env::temp_dir().join(staging_name());
        let mut config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");
        let state = config.package.as_mut().expect("package state");
        state.files.clear();
        let package_root = state
            .package_dir
            .parent()
            .and_then(Path::parent)
            .expect("package root")
            .to_path_buf();
        let mut active =
            read_active_state_file(&package_root.join("active.json")).expect("read active state");
        active.files.clear();
        fs::write(
            package_root.join("active.json"),
            serde_json::to_vec_pretty(&active).expect("serialize active state"),
        )
        .expect("write legacy active state");

        let error =
            verify_installed_entry(&config).expect_err("an unverifiable entry must be refused");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("reinstall the package")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_a_package_whose_entry_is_a_batch_file() {
        // A batch entry is run by `cmd.exe`, and a package can just as easily ship a real executable
        // or a `.ps1`. Refused at install, where the publisher can still act on the message.
        let root = std::env::temp_dir().join(staging_name());
        let error = install_server_package(
            &root,
            &package_bytes_with_entry(
                r#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.cmd","args":[]}
            }"#,
                "runtime/server.cmd",
                b"@echo ready",
            ),
        )
        .expect_err("a batch entry must be refused");
        assert!(
            matches!(&error, McpPackageError::InvalidManifest(message)
                if message.contains("batch file")),
            "unexpected error: {error}"
        );
        assert!(!root
            .join("mcp/packages/publisher.test/fixture-search")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_spawn_a_packaged_server_pointed_at_a_batch_file() {
        // `servers.json` supplies the command, so the install-time check is not the last word: the row
        // can be edited to a batch file that sits inside the package directory.
        let root = std::env::temp_dir().join(staging_name());
        let mut config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");
        let package_dir = config
            .package
            .as_ref()
            .expect("package state")
            .package_dir
            .clone();
        let batch = package_dir.join("runtime/server.cmd");
        fs::write(&batch, b"@echo ready").expect("write batch entry");
        config.command = batch.display().to_string();

        let error = verify_installed_entry(&config)
            .expect_err("a packaged batch entry must be refused at spawn");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("batch file")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writing_active_state_leaves_no_temporary_file_behind() {
        // The temporary file is named per install now, so nothing sweeps a stale one up by reusing the
        // same name: the rename has to be what removes it, or every install leaves one behind.
        let root = std::env::temp_dir().join(staging_name());
        let bytes = stdio_package_bytes();
        let config = install_server_package(&root, &bytes).expect("install package");
        install_server_package(&root, &bytes).expect("reinstall package");
        let package_root = config
            .package
            .as_ref()
            .expect("package state")
            .package_dir
            .parent()
            .and_then(Path::parent)
            .expect("package root")
            .to_path_buf();

        let leftovers: Vec<String> = fs::read_dir(&package_root)
            .expect("read package root")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "unexpected temporary files: {leftovers:?}"
        );
        // What survived is a whole record rather than a file the rename published half-written.
        let active =
            read_active_state_file(&package_root.join("active.json")).expect("read active state");
        assert_eq!(active.digest.len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installs_a_package_that_vendors_its_dependencies() {
        // The entry cap was 128, which no npm or Python server fits once its dependency tree is in the
        // archive. The limits that guard extraction live in the shared extractor, not in this number.
        let root = std::env::temp_dir().join(staging_name());
        let vendored: Vec<String> = (0..300)
            .map(|index| format!("runtime/node_modules/dep-{index}/index.js"))
            .collect();
        let mut files: Vec<(&str, &[u8])> = vec![
            (
                MCP_SERVER_PACKAGE_MANIFEST,
                br#"{
                "schemaVersion":1,
                "id":"fixture-search",
                "name":"Fixture Search",
                "version":"1.2.3",
                "publisher":{"id":"publisher.test","name":"Publisher"},
                "transport":"stdio",
                "entry":{"command":"runtime/server.ps1","args":[]}
            }"#,
            ),
            ("runtime/server.ps1", b"Write-Output ready"),
        ];
        files.extend(
            vendored
                .iter()
                .map(|name| (name.as_str(), b"module.exports = {};" as &[u8])),
        );

        let config = install_server_package(&root, &package_bytes_with_files(&files))
            .expect("install a package that vendors its dependencies");

        assert_eq!(
            config.package.as_ref().expect("package state").files.len(),
            files.len()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_version_directory_is_named_after_enough_of_the_digest_to_be_unique() {
        // Two archives sharing this directory means one of them runs the other's files, so the part
        // of the digest in the name has to be wide enough that no attacker can arrange the collision.
        let root = std::env::temp_dir().join(staging_name());
        let config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");
        let state = config.package.as_ref().expect("package state");

        assert!(PACKAGE_DIRECTORY_DIGEST_CHARS >= 32);
        assert_eq!(
            state.package_dir.file_name().and_then(|name| name.to_str()),
            Some(
                format!(
                    "{}-{}",
                    state.version,
                    &state.digest[..PACKAGE_DIRECTORY_DIGEST_CHARS]
                )
                .as_str()
            )
        );
        // Only the directory name is shortened; what is recorded stays the whole digest.
        assert_eq!(state.digest.len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_reinstall_over_a_tampered_version_directory() {
        // A reinstall used to see the version directory already there and throw the freshly
        // extracted copy away, which made the one repair a user can perform by hand a no-op.
        let root = std::env::temp_dir().join(staging_name());
        let bytes = stdio_package_bytes();
        let config = install_server_package(&root, &bytes).expect("install package");
        fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

        let error = install_server_package(&root, &bytes)
            .expect_err("a reinstall must not adopt a tampered tree");
        assert!(
            matches!(&error, McpPackageError::Integrity(message)
                if message.contains("runtime/server.ps1")),
            "unexpected error: {error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_spawn_a_package_backed_server_whose_entry_was_replaced() {
        // The gate belongs on the spawn path, not only in the checker: `StdioMcpClient` is what
        // turns a stored server row into a running process.
        let root = std::env::temp_dir().join(staging_name());
        let config =
            install_server_package(&root, &stdio_package_bytes()).expect("install package");
        let client =
            crate::StdioMcpClient::spawn_with_timeout(&config, std::time::Duration::from_secs(5))
                .expect("an untouched package spawns");
        drop(client);

        fs::write(&config.command, b"Write-Output tampered").expect("replace entry script");

        let error = match crate::StdioMcpClient::spawn_with_timeout(
            &config,
            std::time::Duration::from_secs(5),
        ) {
            Ok(_) => panic!("a replaced entry must not be spawned"),
            Err(error) => error,
        };
        assert!(
            matches!(&error, crate::McpError::PackageIntegrity(message)
                if message.contains("runtime/server.ps1")),
            "unexpected error: {error}"
        );
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

    #[test]
    fn rejects_a_package_with_a_case_colliding_manifest() {
        // On Windows two entries differing only in case land on one file, so the last copy used to
        // win while a reviewer reading the archive by name saw the first: what was reviewed and
        // what was installed could differ.
        let root = std::env::temp_dir().join(staging_name());
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = SimpleFileOptions::default();
            zip.start_file(MCP_SERVER_PACKAGE_MANIFEST, options)
                .expect("reviewed manifest entry");
            zip.write_all(br#"{"schemaVersion":1,"id":"reviewed"}"#)
                .expect("reviewed manifest bytes");
            zip.start_file(MCP_SERVER_PACKAGE_MANIFEST.to_ascii_uppercase(), options)
                .expect("installed manifest entry");
            zip.write_all(br#"{"schemaVersion":1,"id":"installed"}"#)
                .expect("installed manifest bytes");
            zip.finish().expect("finish zip");
        }
        let error = install_server_package(&root, &bytes.into_inner())
            .expect_err("case-colliding manifest must fail");
        assert!(matches!(error, McpPackageError::UnsafePath(_)));
        let _ = fs::remove_dir_all(root);
    }
}
