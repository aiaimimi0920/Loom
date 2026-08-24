// Public wire models and stable domain errors for the local Art Store.
use serde::{Deserialize, Serialize};

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

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid art id `{0}`")]
    InvalidArtId(String),
    #[error("invalid resource name `{0}`")]
    InvalidResourceName(String),
    #[error("published package missing manifest.json")]
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
    #[error("published package exceeds the compressed size limit of {0} bytes")]
    PackageTooLarge(u64),
    #[error("published package contains too many archive entries")]
    ArchiveEntryCount,
    #[error("archive entry `{name}` exceeds the limit of {limit} bytes")]
    ArchiveEntryTooLarge { name: String, limit: u64 },
    #[error("published package exceeds the expanded size limit of {0} bytes")]
    ArchiveExpandedTooLarge(u64),
    #[error("archive entry `{0}` has a suspicious compression ratio")]
    ArchiveCompressionRatio(String),
    #[error("archive entry `{0}` is a symbolic link")]
    ArchiveSymbolicLink(String),
    #[error("stored resource exceeds the size limit of {0} bytes")]
    StoredResourceTooLarge(u64),
    #[error("stored resource path crosses a symbolic link or reparse point")]
    UnsafeStoredPath,
    #[error("timed out acquiring the Art Store persistence lock")]
    PersistenceLockTimeout,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
