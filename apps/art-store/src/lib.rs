//! Local Loom art store — core, transport-free logic.
//!
//! On-disk layout under the store root:
//!   <root>/arts/<id>/<version>.zip        immutable versioned art packages
//!   <root>/arts/<id>/<version>.zip.sha256 package digest sidecars
//!   <root>/official-art-certifications.json platform-owned package certifications
//!   <root>/binaries/<name>      third-party portable executables
//!
//! The daemon's art-store client (see `loom_tool_registry` / daemon) speaks:
//!   GET  /catalog               -> { "arts": [ {...,latestVersion,versions} ] }
//!   GET  /arts/<id>/<version>.zip -> exact package version
//!   GET  /arts/<id>/<version>.zip.sha256 -> package digest sidecar
//!   GET  /binaries/<name>       -> raw portable-exe bytes
//!   POST /publish               -> body = zip, header X-Art-Id: <id>
//!
//! This module holds only the pure pieces (catalog build, id/name validation,
//! publish persistence). The TCP server lives in `main.rs`.

use std::io::Read;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use loom_protocol::{PackageSignature, PackageSignatureDocument, PublisherIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ARTS_DIR: &str = "arts";
pub const BINARIES_DIR: &str = "binaries";
/// Subdir holding framework package bundles, served as `/frameworks/<id>.zip`:
/// `<root>/frameworks/<id>.zip`. The daemon downloads an independently built
/// framework package from here and validates its manifest before installing it.
pub const FRAMEWORKS_DIR: &str = "frameworks";
pub const GLOBAL_ART_IDS_FILE: &str = "global-art-ids.json";
pub const OFFICIAL_ART_CERTIFICATIONS_FILE: &str = "official-art-certifications.json";
pub const PUBLISHER_DIRECTORY_FILE: &str = "publisher-directory.json";
const MANIFEST_NAME: &str = "manifest.json";
const OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION: u32 = 1;
const PUBLISHER_DIRECTORY_SCHEMA_VERSION: u32 = 1;
const FIRST_GLOBAL_ART_NUMBER: u64 = 40_000_000_000;
const LAST_GLOBAL_ART_NUMBER: u64 = 99_999_999_999;
const FIRST_PUBLISHER_NUMBER: u64 = 10_000_000_000;
const LAST_PUBLISHER_NUMBER: u64 = 39_999_999_999;

/// A catalog entry surfaced by `GET /catalog`. Mirrors the daemon's
/// `ArtStoreEntry` (camelCase over the wire is unnecessary — these are all
/// lowercase single words).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    pub version: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    pub qualified_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_id: Option<String>,
    pub name: String,
    pub description: String,
    pub framework: String,
    pub latest_version: String,
    pub versions: Vec<CatalogVersion>,
    pub official: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedArt {
    pub art_id: String,
    pub global_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublisherKeyStatus {
    Active,
    Retired,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherPublicKey {
    pub key_id: String,
    pub public_key: String,
    pub status: PublisherKeyStatus,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherDirectoryEntry {
    pub user_id: String,
    pub keys: Vec<PublisherPublicKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherRotationRequest {
    pub current_key_id: String,
    pub new_key_id: String,
    pub new_public_key: String,
    pub signature: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublisherDirectory {
    #[serde(default = "publisher_directory_schema_version")]
    schema_version: u32,
    #[serde(default = "first_publisher_number")]
    next_numeric: u64,
    #[serde(default)]
    publishers: std::collections::BTreeMap<String, PublisherDirectoryEntry>,
}

impl Default for PublisherDirectory {
    fn default() -> Self {
        Self {
            schema_version: publisher_directory_schema_version(),
            next_numeric: first_publisher_number(),
            publishers: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalArtIdIndex {
    #[serde(default = "global_art_id_schema_version")]
    schema_version: u32,
    #[serde(default = "first_global_art_number")]
    next_numeric: u64,
    #[serde(default)]
    assignments: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialArtCertificationIndex {
    #[serde(default = "official_art_certification_schema_version")]
    schema_version: u32,
    #[serde(default)]
    certifications: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

impl Default for OfficialArtCertificationIndex {
    fn default() -> Self {
        Self {
            schema_version: official_art_certification_schema_version(),
            certifications: std::collections::BTreeMap::new(),
        }
    }
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
    #[error("published package `{id}` has invalid or missing SemVer version `{version}`")]
    InvalidVersion { id: String, version: String },
    #[error("published package `{0}` is missing a publisher identity")]
    MissingPublisher(String),
    #[error("published package `{0}` is missing its canonical framework dependency")]
    MissingFramework(String),
    #[error("published package `{id}` version `{version}` already exists with different content")]
    VersionConflict { id: String, version: String },
    #[error(
        "published package id `{id}` is already owned by `{existing}` instead of `{incoming}`"
    )]
    IdentityConflict {
        id: String,
        existing: String,
        incoming: String,
    },
    #[error("the platform Art ID namespace is exhausted")]
    GlobalIdExhausted,
    #[error("unsupported official Art certification schema version `{0}`")]
    UnsupportedOfficialCertificationSchema(u32),
    #[error("unsupported publisher directory schema version `{0}`")]
    UnsupportedPublisherDirectorySchema(u32),
    #[error("invalid publisher user id `{0}`")]
    InvalidPublisherId(String),
    #[error("invalid publisher key id `{0}`")]
    InvalidPublisherKeyId(String),
    #[error("invalid publisher public key")]
    InvalidPublisherPublicKey,
    #[error("publisher `{0}` was not found")]
    PublisherNotFound(String),
    #[error("publisher `{0}` has no matching active key")]
    PublisherActiveKeyMissing(String),
    #[error("publisher rotation signature verification failed")]
    PublisherRotationSignature,
    #[error("publisher `{publisher}` already contains key id `{key_id}`")]
    PublisherKeyConflict { publisher: String, key_id: String },
    #[error("the platform publisher ID namespace is exhausted")]
    PublisherIdExhausted,
    #[error("published package is missing a platform publisher signature")]
    MissingPublisherSignature,
    #[error("published package signature metadata is invalid")]
    InvalidPublisherSignatureMetadata,
    #[error("published package signature is not valid for the publisher's active key")]
    PublisherSignatureVerification,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

const fn global_art_id_schema_version() -> u32 {
    1
}

const fn first_global_art_number() -> u64 {
    FIRST_GLOBAL_ART_NUMBER
}

const fn official_art_certification_schema_version() -> u32 {
    OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION
}

const fn publisher_directory_schema_version() -> u32 {
    PUBLISHER_DIRECTORY_SCHEMA_VERSION
}

const fn first_publisher_number() -> u64 {
    FIRST_PUBLISHER_NUMBER
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

/// Derive catalog metadata and its SemVer from an art package's manifest.json.
/// The current catalog contract requires `metadata.dependencies.framework` and
/// `metadata.packageSecurity.publisher.id`.
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
        .filter(|framework| !framework.trim().is_empty())
        .ok_or_else(|| StoreError::MissingFramework(id.clone()))?;
    let version = value
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|security| security.get("version"))
        .and_then(|version| version.as_str())
        .unwrap_or_default()
        .to_owned();
    if semver::Version::parse(&version).is_err() {
        return Err(StoreError::InvalidVersion {
            id: id.clone(),
            version,
        });
    }
    let publisher = value
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .and_then(|security| security.get("publisher"))
        .and_then(|publisher| publisher.get("id"))
        .and_then(|publisher| publisher.as_str())
        .map(str::trim)
        .filter(|publisher| !publisher.is_empty())
        .ok_or_else(|| StoreError::MissingPublisher(id.clone()))?;
    let qualified_id = format!("{publisher}/{id}");
    Ok(CatalogEntry {
        id,
        qualified_id,
        global_id: None,
        name,
        description,
        framework,
        latest_version: version.clone(),
        versions: vec![CatalogVersion {
            version,
            sha256: String::new(),
        }],
        official: false,
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

fn load_global_art_id_index(root: &Path) -> Result<GlobalArtIdIndex, StoreError> {
    let path = root.join(GLOBAL_ART_IDS_FILE);
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(GlobalArtIdIndex::default())
        }
        Err(error) => Err(error.into()),
    }
}

fn write_global_art_id_index(root: &Path, index: &GlobalArtIdIndex) -> Result<(), StoreError> {
    std::fs::create_dir_all(root)?;
    let path = root.join(GLOBAL_ART_IDS_FILE);
    let temporary = root.join(format!("{GLOBAL_ART_IDS_FILE}.tmp"));
    let mut bytes = serde_json::to_vec_pretty(index)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    if path.is_file() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn load_official_art_certifications(
    root: &Path,
) -> Result<OfficialArtCertificationIndex, StoreError> {
    let path = root.join(OFFICIAL_ART_CERTIFICATIONS_FILE);
    let index = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            OfficialArtCertificationIndex::default()
        }
        Err(error) => return Err(error.into()),
    };
    if index.schema_version != OFFICIAL_ART_CERTIFICATION_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedOfficialCertificationSchema(
            index.schema_version,
        ));
    }
    Ok(index)
}

fn catalog_entry_is_official(
    entry: &CatalogEntry,
    certifications: &OfficialArtCertificationIndex,
) -> bool {
    let Some(actual_digest) = entry
        .versions
        .iter()
        .find(|version| version.version == entry.latest_version)
        .map(|version| version.sha256.as_str())
    else {
        return false;
    };
    certifications
        .certifications
        .get(&entry.qualified_id)
        .and_then(|versions| versions.get(&entry.latest_version))
        .map(|digest| digest.trim().to_ascii_lowercase())
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .is_some_and(|digest| digest == actual_digest)
}

fn assign_global_art_id(root: &Path, qualified_id: &str) -> Result<String, StoreError> {
    let mut index = load_global_art_id_index(root)?;
    if let Some(existing) = index.assignments.get(qualified_id) {
        return Ok(existing.clone());
    }
    let used = index
        .assignments
        .values()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut numeric = index.next_numeric.max(FIRST_GLOBAL_ART_NUMBER);
    let global_id = loop {
        if numeric > LAST_GLOBAL_ART_NUMBER {
            return Err(StoreError::GlobalIdExhausted);
        }
        let candidate = format!("NA{numeric:011}");
        numeric += 1;
        if !used.contains(&candidate) {
            break candidate;
        }
    };
    index.schema_version = global_art_id_schema_version();
    index.next_numeric = numeric;
    index
        .assignments
        .insert(qualified_id.to_owned(), global_id.clone());
    write_global_art_id_index(root, &index)?;
    Ok(global_id)
}

fn load_publisher_directory(root: &Path) -> Result<PublisherDirectory, StoreError> {
    let path = root.join(PUBLISHER_DIRECTORY_FILE);
    let directory = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => PublisherDirectory::default(),
        Err(error) => return Err(error.into()),
    };
    if directory.schema_version != PUBLISHER_DIRECTORY_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedPublisherDirectorySchema(
            directory.schema_version,
        ));
    }
    Ok(directory)
}

fn write_publisher_directory(
    root: &Path,
    directory: &PublisherDirectory,
) -> Result<(), StoreError> {
    std::fs::create_dir_all(root)?;
    let path = root.join(PUBLISHER_DIRECTORY_FILE);
    let temporary = root.join(format!("{PUBLISHER_DIRECTORY_FILE}.tmp"));
    let mut bytes = serde_json::to_vec_pretty(directory)?;
    bytes.push(b'\n');
    std::fs::write(&temporary, bytes)?;
    if path.is_file() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn validate_publisher_key(key_id: &str, public_key: &str) -> Result<(), StoreError> {
    if !is_safe_art_id(key_id) {
        return Err(StoreError::InvalidPublisherKeyId(key_id.to_owned()));
    }
    let decoded = BASE64
        .decode(public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    Ok(())
}

pub fn is_platform_publisher_id(value: &str) -> bool {
    (value.len() == 13
        && value.starts_with("NU")
        && value[2..].bytes().all(|byte| byte.is_ascii_digit()))
        || (value.len() == 11
            && value.starts_with('L')
            && value[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

pub fn publisher_rotation_message(
    user_id: &str,
    current_key_id: &str,
    new_key_id: &str,
    new_public_key: &str,
) -> String {
    format!("loom.publisher.rotate.v1\n{user_id}\n{current_key_id}\n{new_key_id}\n{new_public_key}")
}

pub fn register_publisher(
    root: &Path,
    key_id: &str,
    public_key: &str,
) -> Result<PublisherDirectoryEntry, StoreError> {
    register_publisher_with_id(root, None, key_id, public_key)
}

pub fn register_publisher_with_id(
    root: &Path,
    requested_user_id: Option<&str>,
    key_id: &str,
    public_key: &str,
) -> Result<PublisherDirectoryEntry, StoreError> {
    validate_publisher_key(key_id, public_key)?;
    let mut directory = load_publisher_directory(root)?;
    if let Some(requested_user_id) = requested_user_id {
        if !is_platform_publisher_id(requested_user_id) {
            return Err(StoreError::InvalidPublisherId(requested_user_id.to_owned()));
        }
        if let Some(existing) = directory.publishers.get(requested_user_id) {
            if existing
                .keys
                .iter()
                .any(|key| key.key_id == key_id && key.public_key == public_key)
            {
                return Ok(existing.clone());
            }
            return Err(StoreError::PublisherKeyConflict {
                publisher: requested_user_id.to_owned(),
                key_id: key_id.to_owned(),
            });
        }
        let entry = PublisherDirectoryEntry {
            user_id: requested_user_id.to_owned(),
            keys: vec![PublisherPublicKey {
                key_id: key_id.to_owned(),
                public_key: public_key.to_owned(),
                status: PublisherKeyStatus::Active,
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            }],
        };
        directory
            .publishers
            .insert(requested_user_id.to_owned(), entry.clone());
        write_publisher_directory(root, &directory)?;
        return Ok(entry);
    }
    if let Some(existing) = directory.publishers.values().find(|publisher| {
        publisher
            .keys
            .iter()
            .any(|key| key.public_key == public_key)
    }) {
        return Ok(existing.clone());
    }
    let used = directory
        .publishers
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut numeric = directory.next_numeric.max(FIRST_PUBLISHER_NUMBER);
    let user_id = loop {
        if numeric > LAST_PUBLISHER_NUMBER {
            return Err(StoreError::PublisherIdExhausted);
        }
        let candidate = format!("NU{numeric:011}");
        numeric += 1;
        if !used.contains(&candidate) {
            break candidate;
        }
    };
    let entry = PublisherDirectoryEntry {
        user_id: user_id.clone(),
        keys: vec![PublisherPublicKey {
            key_id: key_id.to_owned(),
            public_key: public_key.to_owned(),
            status: PublisherKeyStatus::Active,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }],
    };
    directory.schema_version = PUBLISHER_DIRECTORY_SCHEMA_VERSION;
    directory.next_numeric = numeric;
    directory.publishers.insert(user_id, entry.clone());
    write_publisher_directory(root, &directory)?;
    Ok(entry)
}

pub fn read_publisher(
    root: &Path,
    user_id: &str,
) -> Result<Option<PublisherDirectoryEntry>, StoreError> {
    if !is_platform_publisher_id(user_id) {
        return Err(StoreError::InvalidPublisherId(user_id.to_owned()));
    }
    Ok(load_publisher_directory(root)?
        .publishers
        .get(user_id)
        .cloned())
}

pub fn rotate_publisher_key(
    root: &Path,
    user_id: &str,
    request: &PublisherRotationRequest,
) -> Result<PublisherDirectoryEntry, StoreError> {
    if !is_platform_publisher_id(user_id) {
        return Err(StoreError::InvalidPublisherId(user_id.to_owned()));
    }
    validate_publisher_key(&request.new_key_id, &request.new_public_key)?;
    let mut directory = load_publisher_directory(root)?;
    let publisher = directory
        .publishers
        .get_mut(user_id)
        .ok_or_else(|| StoreError::PublisherNotFound(user_id.to_owned()))?;
    if publisher
        .keys
        .iter()
        .any(|key| key.key_id == request.new_key_id)
    {
        return Err(StoreError::PublisherKeyConflict {
            publisher: user_id.to_owned(),
            key_id: request.new_key_id.clone(),
        });
    }
    let current = publisher
        .keys
        .iter()
        .find(|key| {
            key.key_id == request.current_key_id && key.status == PublisherKeyStatus::Active
        })
        .ok_or_else(|| StoreError::PublisherActiveKeyMissing(user_id.to_owned()))?;
    let current_bytes = BASE64
        .decode(current.public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let current_bytes: [u8; 32] = current_bytes
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let verifying_key = VerifyingKey::from_bytes(&current_bytes)
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let signature_bytes = BASE64
        .decode(request.signature.as_bytes())
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    verifying_key
        .verify(
            publisher_rotation_message(
                user_id,
                &request.current_key_id,
                &request.new_key_id,
                &request.new_public_key,
            )
            .as_bytes(),
            &signature,
        )
        .map_err(|_| StoreError::PublisherRotationSignature)?;
    for key in &mut publisher.keys {
        if key.status == PublisherKeyStatus::Active {
            key.status = PublisherKeyStatus::Retired;
        }
    }
    publisher.keys.push(PublisherPublicKey {
        key_id: request.new_key_id.clone(),
        public_key: request.new_public_key.clone(),
        status: PublisherKeyStatus::Active,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    });
    let entry = publisher.clone();
    write_publisher_directory(root, &directory)?;
    Ok(entry)
}

/// Build the catalog by scanning immutable version directories.
/// Packages that fail to parse are skipped (a bad upload shouldn't 500 the list).
pub fn build_catalog(root: &Path) -> Result<Vec<CatalogEntry>, StoreError> {
    let arts_dir = root.join(ARTS_DIR);
    let mut entries = std::collections::BTreeMap::<String, CatalogEntry>::new();
    if !arts_dir.is_dir() {
        return Ok(Vec::new());
    }
    for dir_entry in std::fs::read_dir(&arts_dir)? {
        let path = dir_entry?.path();
        if path.is_dir() {
            for version_entry in std::fs::read_dir(&path)? {
                let version_path = version_entry?.path();
                if version_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("zip")
                {
                    merge_catalog_zip(&mut entries, &version_path)?;
                }
            }
        }
    }
    let global_ids = load_global_art_id_index(root)?.assignments;
    let official_certifications = load_official_art_certifications(root)?;
    for entry in entries.values_mut() {
        entry.global_id = global_ids.get(&entry.qualified_id).cloned();
        entry.official = catalog_entry_is_official(entry, &official_certifications);
    }
    Ok(entries.into_values().collect())
}

fn merge_catalog_zip(
    entries: &mut std::collections::BTreeMap<String, CatalogEntry>,
    path: &Path,
) -> Result<(), StoreError> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    let Ok(manifest) = read_manifest_bytes(&bytes) else {
        return Ok(());
    };
    let Ok(mut incoming) = catalog_entry_from_manifest(&manifest) else {
        return Ok(());
    };
    if incoming.id.is_empty() {
        return Ok(());
    }
    incoming.versions[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
    let version = incoming.versions[0].clone();
    let entry = entries
        .entry(incoming.id.clone())
        .or_insert_with(|| incoming.clone());
    if entry.qualified_id != incoming.qualified_id {
        return Err(StoreError::IdentityConflict {
            id: incoming.id,
            existing: entry.qualified_id.clone(),
            incoming: incoming.qualified_id,
        });
    }
    if let Some(existing) = entry
        .versions
        .iter()
        .find(|existing| existing.version == version.version)
    {
        if existing.sha256 != version.sha256 {
            return Err(StoreError::VersionConflict {
                id: incoming.id,
                version: version.version,
            });
        }
    } else {
        entry.versions.push(version.clone());
    }
    entry.versions.sort_by(|left, right| {
        semver::Version::parse(&left.version)
            .ok()
            .cmp(&semver::Version::parse(&right.version).ok())
            .then_with(|| left.version.cmp(&right.version))
    });
    if let Some(latest) = entry.versions.last() {
        entry.latest_version = latest.version.clone();
        if latest.version == version.version {
            entry.qualified_id = incoming.qualified_id;
            entry.name = incoming.name;
            entry.description = incoming.description;
            entry.framework = incoming.framework;
        }
    }
    Ok(())
}

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
    let path = art_version_zip_path(root, id, version)?;
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn read_art_zip_version_sha256(
    root: &Path,
    id: &str,
    version: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    let zip_path = art_version_zip_path(root, id, version)?;
    let sidecar_path = zip_path.with_extension("zip.sha256");
    match std::fs::read(&sidecar_path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
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

fn verify_published_package_signature(root: &Path, zip_bytes: &[u8]) -> Result<(), StoreError> {
    let manifest_bytes = read_manifest_bytes(zip_bytes)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let security = manifest
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .ok_or(StoreError::MissingPublisherSignature)?;
    let publisher: PublisherIdentity = security
        .get("publisher")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or(StoreError::MissingPublisherSignature)?;
    let signature: PackageSignature = security
        .get("signature")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or(StoreError::MissingPublisherSignature)?;
    if !is_platform_publisher_id(&publisher.id)
        || publisher.key_id.as_deref() != Some(signature.key_id.as_str())
        || signature.algorithm != "ed25519"
        || !is_safe_resource_name(&signature.file)
    {
        return Err(StoreError::InvalidPublisherSignatureMetadata);
    }
    let platform_publisher = read_publisher(root, &publisher.id)?
        .ok_or_else(|| StoreError::PublisherNotFound(publisher.id.clone()))?;
    let active_key = platform_publisher
        .keys
        .iter()
        .find(|key| key.key_id == signature.key_id && key.status == PublisherKeyStatus::Active)
        .ok_or_else(|| StoreError::PublisherActiveKeyMissing(publisher.id.clone()))?;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    let mut files = Vec::<(String, Vec<u8>)>::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut signature_document = None;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let raw_name = file.name().to_owned();
        let name = raw_name.replace('\\', "/");
        if raw_name.contains('\\')
            || file.enclosed_name().is_none()
            || !is_safe_resource_name(&name)
        {
            return Err(StoreError::InvalidResourceName(name));
        }
        let folded = name.to_ascii_lowercase();
        if !seen.insert(folded.clone()) {
            return Err(StoreError::InvalidResourceName(name));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if folded == signature.file.to_ascii_lowercase() {
            signature_document = Some(serde_json::from_slice::<PackageSignatureDocument>(&bytes)?);
        } else {
            files.push((name, bytes));
        }
    }
    let document = signature_document.ok_or(StoreError::MissingPublisherSignature)?;
    if document.algorithm != signature.algorithm
        || document.key_id != signature.key_id
        || document.digest_algorithm != "sha256"
        || document.public_key != active_key.public_key
    {
        return Err(StoreError::InvalidPublisherSignatureMetadata);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (name, bytes) in files {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != document.digest {
        return Err(StoreError::PublisherSignatureVerification);
    }
    let public_key = BASE64
        .decode(document.public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let signature_bytes = BASE64
        .decode(document.signature.as_bytes())
        .map_err(|_| StoreError::PublisherSignatureVerification)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| StoreError::PublisherSignatureVerification)?;
    verifying_key
        .verify(digest.as_bytes(), &signature)
        .map_err(|_| StoreError::PublisherSignatureVerification)
}

pub fn store_verified_published_zip(
    root: &Path,
    declared_id: Option<&str>,
    zip_bytes: &[u8],
) -> Result<PublishedArt, StoreError> {
    verify_published_package_signature(root, zip_bytes)?;
    store_published_zip(root, declared_id, zip_bytes)
}

/// Persist a published art package as an immutable version.
pub fn store_published_zip(
    root: &Path,
    declared_id: Option<&str>,
    zip_bytes: &[u8],
) -> Result<PublishedArt, StoreError> {
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
    if let Some(existing) = build_catalog(root)?
        .into_iter()
        .find(|existing| existing.id == entry.id)
    {
        if existing.qualified_id != entry.qualified_id {
            return Err(StoreError::IdentityConflict {
                id: entry.id,
                existing: existing.qualified_id,
                incoming: entry.qualified_id,
            });
        }
    }
    let version = entry.latest_version.clone();
    let path = art_version_zip_path(root, &entry.id, &version)?;
    std::fs::create_dir_all(path.parent().expect("versioned Art package parent"))?;
    if path.is_file() {
        let existing = std::fs::read(&path)?;
        if Sha256::digest(&existing) != Sha256::digest(zip_bytes) {
            return Err(StoreError::VersionConflict {
                id: entry.id,
                version,
            });
        }
    }
    std::fs::write(&path, zip_bytes)?;
    std::fs::write(
        path.with_extension("zip.sha256"),
        art_zip_sha256_sidecar(&format!("{}/{}", entry.id, entry.latest_version), zip_bytes),
    )?;
    let global_id = assign_global_art_id(root, &entry.qualified_id)?;
    Ok(PublishedArt {
        art_id: entry.id,
        global_id,
    })
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
        let mut manifest: serde_json::Value = serde_json::from_str(manifest).unwrap();
        let execution_framework = manifest
            .get("execution")
            .and_then(|execution| execution.get("framework"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("process")
            .to_owned();
        let metadata = manifest
            .as_object_mut()
            .unwrap()
            .entry("metadata")
            .or_insert_with(|| serde_json::json!({}));
        let metadata = metadata.as_object_mut().unwrap();
        metadata
            .entry("dependencies")
            .or_insert_with(|| serde_json::json!({ "framework": execution_framework }));
        let security = metadata
            .entry("packageSecurity")
            .or_insert_with(|| serde_json::json!({}));
        security
            .as_object_mut()
            .unwrap()
            .entry("publisher")
            .or_insert_with(|| serde_json::json!({ "id": "publisher.test" }));
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            writer.start_file(MANIFEST_NAME, opts).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    fn build_signed_zip(
        root: &Path,
        art_id: &str,
        version: &str,
        user_id: &str,
        key: &loom_plugin_security::SigningKeyDocument,
    ) -> Vec<u8> {
        let package = root.join(format!("package-{version}"));
        std::fs::create_dir_all(&package).unwrap();
        let manifest = serde_json::json!({
            "id": art_id,
            "name": "Signed",
            "description": version,
            "execution": { "type": "framework_art", "framework": "process" },
            "metadata": {
                "dependencies": { "framework": "process" },
                "packageSecurity": {
                    "version": version,
                    "publisher": { "id": user_id, "keyId": key.key_id },
                    "signature": {
                        "algorithm": "ed25519",
                        "keyId": key.key_id,
                        "file": "signature.json"
                    }
                }
            }
        });
        std::fs::write(
            package.join(MANIFEST_NAME),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(package.join("payload.txt"), version.as_bytes()).unwrap();
        loom_plugin_security::sign_package(&package, "signature.json", key).unwrap();
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            for name in [MANIFEST_NAME, "payload.txt", "signature.json"] {
                writer.start_file(name, options).unwrap();
                writer
                    .write_all(&std::fs::read(package.join(name)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
        }
        bytes
    }

    fn write_official_certification(root: &Path, qualified_id: &str, version: &str, digest: &str) {
        let document = serde_json::json!({
            "schemaVersion": 1,
            "certifications": {
                (qualified_id): {
                    (version): digest,
                }
            }
        });
        std::fs::write(
            root.join(OFFICIAL_ART_CERTIFICATIONS_FILE),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
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
    fn catalog_entry_requires_declared_framework_and_publisher() {
        let manifest = r#"{"id":"wf","name":"WF","description":"d",
            "execution":{"type":"workflow","workflowId":"x"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"publisher.test"}},"dependencies":{"framework":"workflow"}}}"#;
        let entry = catalog_entry_from_manifest(manifest.as_bytes()).unwrap();
        assert_eq!(entry.id, "wf");
        assert_eq!(entry.framework, "workflow");
    }

    #[test]
    fn catalog_entry_rejects_missing_canonical_framework_dependency() {
        let manifest = r#"{"id":"p","name":"P","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"publisher.test"}}}}"#;
        assert!(matches!(
            catalog_entry_from_manifest(manifest.as_bytes()),
            Err(StoreError::MissingFramework(id)) if id == "p"
        ));
    }

    #[test]
    fn build_catalog_lists_published_zips_sorted() {
        let root = temp_root();
        let a = build_zip(
            r#"{"id":"b-art","name":"B","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let b = build_zip(
            r#"{"id":"a-art","name":"A","description":"d",
            "execution":{"type":"cloud_api","endpoint":"https://x","method":"POST"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        std::fs::create_dir_all(root.join(ARTS_DIR).join("b-art")).unwrap();
        std::fs::create_dir_all(root.join(ARTS_DIR).join("a-art")).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("b-art/1.0.0.zip"), &a).unwrap();
        std::fs::write(root.join(ARTS_DIR).join("a-art/1.0.0.zip"), &b).unwrap();
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].id, "a-art");
        assert_eq!(catalog[1].id, "b-art");
        assert_eq!(catalog[1].framework, "process");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn package_manifest_cannot_self_certify_as_official() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"claimed-official","name":"Claimed","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "official":true,
            "metadata":{"official":true,"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("claimed-official"), &zip).unwrap();

        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(!catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn official_certification_requires_the_exact_latest_package_digest() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"certified-art","name":"Certified","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("certified-art"), &zip).unwrap();
        let digest = format!("{:x}", Sha256::digest(&zip));
        write_official_certification(&root, "neuro.official/certified-art", "1.0.0", &digest);

        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_new_latest_version_does_not_inherit_an_older_certification() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"version-certified","name":"Certified","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        let second = build_zip(
            r#"{"id":"version-certified","name":"Certified","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.1.0","publisher":{"id":"neuro.official","name":"Neuro"}}}}"#,
        );
        store_published_zip(&root, Some("version-certified"), &first).unwrap();
        write_official_certification(
            &root,
            "neuro.official/version-certified",
            "1.0.0",
            &format!("{:x}", Sha256::digest(&first)),
        );
        assert!(build_catalog(&root).unwrap()[0].official);

        store_published_zip(&root, Some("version-certified"), &second).unwrap();
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog[0].latest_version, "1.1.0");
        assert!(!catalog[0].official);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn store_and_read_roundtrip() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"pingo-art","name":"Pingo","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let published = store_published_zip(&root, Some("pingo-art"), &zip).unwrap();
        assert_eq!(published.art_id, "pingo-art");
        assert!(published.global_id.starts_with("NA"));
        assert_eq!(published.global_id.len(), 13);
        let read = read_art_zip_version(&root, "pingo-art", "1.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(read, zip);
        let sidecar = read_art_zip_version_sha256(&root, "pingo-art", "1.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(sidecar).unwrap(),
            art_zip_sha256_sidecar("pingo-art/1.0.0", &zip)
        );
        assert!(read_art_zip_version(&root, "missing", "1.0.0")
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_assigns_one_stable_platform_global_id_per_repository() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"stable-art","name":"Stable","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"},"art":{"globalId":"NA00000000001"}}}"#,
        );
        let second = build_zip(
            r#"{"id":"stable-art","name":"Stable","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.1.0"}}}"#,
        );
        let published_first = store_published_zip(&root, Some("stable-art"), &first).unwrap();
        let published_second = store_published_zip(&root, Some("stable-art"), &second).unwrap();
        assert_eq!(published_first.global_id, published_second.global_id);
        assert_ne!(published_first.global_id, "NA00000000001");
        assert!(published_first.global_id.starts_with("NA"));
        assert_eq!(published_first.global_id.len(), 13);
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(
            catalog[0].global_id.as_deref(),
            Some(published_first.global_id.as_str())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_version_digest_sidecar_is_not_synthesized() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"canonical-art","name":"Canonical","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let directory = root.join(ARTS_DIR).join("canonical-art");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("1.0.0.zip"), &zip).unwrap();

        assert!(read_art_zip_version_sha256(&root, "canonical-art", "1.0.0")
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publish_rejects_id_mismatch() {
        let root = temp_root();
        let zip = build_zip(
            r#"{"id":"real-id","name":"R","description":"d",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
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

    #[test]
    fn publishing_multiple_versions_preserves_history_and_orders_semver() {
        let root = temp_root();
        for version in ["1.0.0", "1.10.0", "1.2.0"] {
            let zip = build_zip(&format!(
                r#"{{"id":"versioned-art","name":"Versioned","description":"{version}",
                "execution":{{"type":"framework_art","framework":"process"}},
                "metadata":{{"packageSecurity":{{"version":"{version}"}}}}}}"#
            ));
            store_published_zip(&root, Some("versioned-art"), &zip).unwrap();
        }
        let catalog = build_catalog(&root).unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].latest_version, "1.10.0");
        assert_eq!(catalog[0].description, "1.10.0");
        assert_eq!(
            catalog[0]
                .versions
                .iter()
                .map(|version| version.version.as_str())
                .collect::<Vec<_>>(),
            vec!["1.0.0", "1.2.0", "1.10.0"]
        );
        assert!(read_art_zip_version(&root, "versioned-art", "1.2.0")
            .unwrap()
            .is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn catalog_rejects_same_version_with_different_content() {
        let root = temp_root();
        let first = build_zip(
            r#"{"id":"conflict-art","name":"First","description":"one",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        let second = build_zip(
            r#"{"id":"conflict-art","name":"Second","description":"two",
            "execution":{"type":"framework_art","framework":"process"},
            "metadata":{"packageSecurity":{"version":"1.0.0"}}}"#,
        );
        store_published_zip(&root, Some("conflict-art"), &first).unwrap();
        assert!(matches!(
            store_published_zip(&root, Some("conflict-art"), &second).unwrap_err(),
            StoreError::VersionConflict { .. }
        ));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publisher_directory_accepts_the_default_test_user_id() {
        let root = temp_root();
        let key = loom_plugin_security::generate_signing_key("default-user-key");
        let publisher =
            register_publisher_with_id(&root, Some("L0000000000"), &key.key_id, &key.public_key)
                .expect("register default test publisher");
        assert_eq!(publisher.user_id, "L0000000000");
        assert_eq!(
            read_publisher(&root, "L0000000000")
                .expect("read publisher")
                .expect("publisher exists")
                .keys[0]
                .public_key,
            key.public_key
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn publisher_directory_rotates_keys_and_verified_publish_requires_the_active_key() {
        let root = temp_root();
        let first_key = loom_plugin_security::generate_signing_key("key-1");
        let publisher = register_publisher(&root, &first_key.key_id, &first_key.public_key)
            .expect("register publisher");
        assert!(is_platform_publisher_id(&publisher.user_id));

        let first_zip =
            build_signed_zip(&root, "signed-art", "1.0.0", &publisher.user_id, &first_key);
        store_verified_published_zip(&root, Some("signed-art"), &first_zip)
            .expect("publish with active key");

        let second_key = loom_plugin_security::generate_signing_key("key-2");
        let message = publisher_rotation_message(
            &publisher.user_id,
            &first_key.key_id,
            &second_key.key_id,
            &second_key.public_key,
        );
        let rotated = rotate_publisher_key(
            &root,
            &publisher.user_id,
            &PublisherRotationRequest {
                current_key_id: first_key.key_id.clone(),
                new_key_id: second_key.key_id.clone(),
                new_public_key: second_key.public_key.clone(),
                signature: loom_plugin_security::sign_message(&first_key, message.as_bytes())
                    .expect("rotation signature"),
            },
        )
        .expect("rotate publisher key");
        assert_eq!(rotated.keys[0].status, PublisherKeyStatus::Retired);
        assert_eq!(rotated.keys[1].status, PublisherKeyStatus::Active);

        let stale_zip =
            build_signed_zip(&root, "signed-art", "2.0.0", &publisher.user_id, &first_key);
        assert!(matches!(
            store_verified_published_zip(&root, Some("signed-art"), &stale_zip),
            Err(StoreError::PublisherActiveKeyMissing(_))
        ));

        let current_zip = build_signed_zip(
            &root,
            "signed-art",
            "2.0.0",
            &publisher.user_id,
            &second_key,
        );
        store_verified_published_zip(&root, Some("signed-art"), &current_zip)
            .expect("publish with rotated key");
        assert_eq!(build_catalog(&root).unwrap()[0].versions.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }
}
